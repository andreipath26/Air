//! CPU llama.cpp server integration, resource bounds, and verified escalation.
use air_core::verification::GeneratedOutputVerifier;
use serde_json::{json, Value};
use std::{
    fs, io::Read, path::Path, process::{Child, Command, Stdio},
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    thread::{self, JoinHandle}, time::{Duration, Instant},
};
const MAX_PROMPT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_BYTES: usize = 32 * 1024;
const MAX_RSS_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_CONTEXT_TOKENS: u32 = 4096;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ModelFamily { Gemma4E2b, SmolLm3B }
impl ModelFamily {
    pub fn as_str(self)->&'static str { match self {Self::Gemma4E2b=>"Gemma 4 E2B",Self::SmolLm3B=>"SmolLM3 3B"} }
    pub fn recommended_context(self)->u32 {4096}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ThinkingMode { Direct, Budgeted{reasoning_tokens:u16} }
impl ThinkingMode {
    fn budget(self)->Result<u16,String>{match self {
        Self::Direct=>Ok(0),
        Self::Budgeted{reasoning_tokens} if (1..=1024).contains(&reasoning_tokens)=>Ok(reasoning_tokens),
        _=>Err("thinking budget must be in 1..=1024".into())
    }}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ResourceLimits {pub max_rss_bytes:u64,pub context_tokens:u32,pub max_new_tokens:u32}
impl Default for ResourceLimits {
    fn default()->Self{Self{max_rss_bytes:6*1024*1024*1024,context_tokens:2048,max_new_tokens:256}}
}
impl ResourceLimits {
    pub fn validate(self)->Result<Self,String>{
        if self.max_rss_bytes==0||self.max_rss_bytes>MAX_RSS_BYTES{return Err(format!("RSS cap must be in 1..={MAX_RSS_BYTES} bytes"))}
        if self.context_tokens==0||self.context_tokens>MAX_CONTEXT_TOKENS{return Err(format!("context cap must be in 1..={MAX_CONTEXT_TOKENS} tokens"))}
        if self.max_new_tokens==0||self.max_new_tokens>self.context_tokens{return Err("generation cap must fit within context cap".into())}
        Ok(self)
    }
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct GenerationOptions {pub thinking:ThinkingMode,pub max_new_tokens:u32}
impl Default for GenerationOptions {fn default()->Self{Self{thinking:ThinkingMode::Direct,max_new_tokens:256}}}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Completion {pub text:String,pub elapsed:Duration}
pub trait CompletionBackend:Send+Sync {fn complete(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>}

/// Owns a CPU-only llama.cpp loopback server loading a local GGUF file.
/// An RSS watchdog polls every 10ms and terminates the server when it crosses
/// the configured cap; context and generation are also bounded per request.
pub struct LlamaCppRunner {
    child:Arc<Mutex<Child>>, endpoint:String, model_name:String, limits:ResourceLimits,
    watchdog_active:Arc<AtomicBool>, rss_exceeded:Arc<AtomicBool>,
    watchdog:Mutex<Option<JoinHandle<()>>>,
}
impl LlamaCppRunner {
    pub fn launch(executable:impl AsRef<Path>,gguf:impl AsRef<Path>,family:ModelFamily,port:u16,limits:ResourceLimits)->Result<Self,String>{
        let limits=limits.validate()?;
        if port==0{return Err("port must be nonzero".into())}
        let file=gguf.as_ref();
        if !file.is_file()||file.extension().and_then(|x|x.to_str())!=Some("gguf"){return Err("model must be an existing GGUF file".into())}
        let file=fs::canonicalize(file).map_err(|e|format!("GGUF path: {e}"))?;
        let threads=thread::available_parallelism().map_or(2,usize::from).min(4).to_string();
        let child=Command::new(executable).arg("--model").arg(file).arg("--host").arg("127.0.0.1")
            .arg("--port").arg(port.to_string()).arg("--ctx-size").arg(limits.context_tokens.to_string())
            .arg("--n-gpu-layers").arg("0").arg("--threads").arg(threads).arg("--no-webui")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
            .map_err(|e|format!("starting llama.cpp server: {e}"))?;
        let child=Arc::new(Mutex::new(child));
        let active=Arc::new(AtomicBool::new(true));
        let exceeded=Arc::new(AtomicBool::new(false));
        let watch_child=Arc::clone(&child);
        let watch_active=Arc::clone(&active);
        let watch_exceeded=Arc::clone(&exceeded);
        let rss_cap=limits.max_rss_bytes;
        let watchdog=thread::spawn(move||{
            while watch_active.load(Ordering::Acquire) {
                let mut should_stop=false;
                if let Ok(mut process)=watch_child.lock() {
                    match fs::read_to_string(format!("/proc/{}/status",process.id())) {
                        Ok(status)=>{
                            let rss=status.lines().find(|line|line.starts_with("VmRSS:"))
                                .and_then(|line|line.split_whitespace().nth(1))
                                .and_then(|kb|kb.parse::<u64>().ok())
                                .map(|kb|kb.saturating_mul(1024));
                            if rss.is_none() || rss.is_some_and(|bytes|bytes>rss_cap) {
                                watch_exceeded.store(true,Ordering::Release);
                                should_stop=true;
                            }
                        }
                        Err(_) => {
                            if process.try_wait().ok().flatten().is_none() {
                                watch_exceeded.store(true,Ordering::Release);
                                should_stop=true;
                            } else { break; }
                        }
                    }
                    if should_stop {
                        let _=process.kill();
                        let _=process.wait();
                        break;
                    }
                } else {
                    watch_exceeded.store(true,Ordering::Release);
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        Ok(Self{child,endpoint:format!("http://127.0.0.1:{port}/v1/chat/completions"),
            model_name:family.as_str().into(),limits,watchdog_active:active,rss_exceeded:exceeded,
            watchdog:Mutex::new(Some(watchdog))})
    }
    pub fn limits(&self)->ResourceLimits{self.limits}
    fn stop(&self){
        self.watchdog_active.store(false,Ordering::Release);
        if let Ok(mut c)=self.child.lock(){let _=c.kill();let _=c.wait();}
        if let Ok(mut w)=self.watchdog.lock(){if let Some(handle)=w.take(){let _=handle.join();}}
    }
    fn request(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>{
        if self.rss_exceeded.load(Ordering::Acquire){return Err("llama.cpp RSS watchdog terminated the server at its configured cap".into())}
        let token_cap=options.max_new_tokens.min(self.limits.max_new_tokens);
        if token_cap==0||token_cap>self.limits.context_tokens{return Err("invalid output token budget".into())}
        let thinking=options.thinking.budget()?;
        let started=Instant::now();
        let system=if thinking==0{"Respond directly and concisely; do not emit a reasoning trace."}
            else{"Reason within the configured thinking budget, then provide a concise answer."};
        let body=json!({"model":self.model_name,"messages":[{"role":"system","content":system},{"role":"user","content":prompt}],
            "temperature":0,"max_tokens":token_cap,"stream":false,
            "chat_template_kwargs":{"enable_thinking":thinking>0,"reasoning_budget":thinking}});
        let agent=ureq::AgentBuilder::new().redirects(0).timeout_connect(Duration::from_secs(2))
            .timeout(Duration::from_secs(120)).build();
        let response=agent.post(&self.endpoint).send_json(body).map_err(|e|e.to_string())?;
        let mut raw=String::new();
        response.into_reader().take((MAX_OUTPUT_BYTES*4+1) as u64).read_to_string(&mut raw).map_err(|e|e.to_string())?;
        if self.rss_exceeded.load(Ordering::Acquire){return Err("llama.cpp RSS watchdog terminated the server at its configured cap".into())}
        if raw.len()>MAX_OUTPUT_BYTES*4{return Err("llama.cpp response exceeds byte cap".into())}
        let value:Value=serde_json::from_str(&raw).map_err(|e|e.to_string())?;
        let text=value.pointer("/choices/0/message/content").and_then(Value::as_str).ok_or("missing llama.cpp message content")?;
        if text.len()>MAX_OUTPUT_BYTES{return Err("generated output exceeds byte cap".into())}
        Ok(Completion{text:text.to_owned(),elapsed:started.elapsed()})
    }
}
impl CompletionBackend for LlamaCppRunner {
    fn complete(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>{
        if prompt.is_empty()||prompt.len()>MAX_PROMPT_BYTES{return Err("prompt is empty or exceeds byte cap".into())}
        self.request(prompt,options)
    }
}
impl Drop for LlamaCppRunner {fn drop(&mut self){self.stop()}}

/// Stable headless backend; every request returns the same configured result.
pub struct DeterministicMock{response:String}
impl DeterministicMock{pub fn new(response:impl Into<String>)->Self{Self{response:response.into()}}}
impl Default for DeterministicMock{fn default()->Self{Self::new("mock completion")}}
impl CompletionBackend for DeterministicMock {
    fn complete(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>{
        if prompt.is_empty()||prompt.len()>MAX_PROMPT_BYTES{return Err("prompt is empty or exceeds byte cap".into())}
        options.thinking.budget()?;
        Ok(Completion{text:self.response.clone(),elapsed:Duration::ZERO})
    }
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct VerifiedCompletion{pub completion:Completion}
/// Never returns an accepted SLM answer unless AIR core's task-specific verifier passes it.
pub fn run_verified<B:CompletionBackend,V:GeneratedOutputVerifier>(backend:&B,verifier:&V,prompt:&str,options:&GenerationOptions)->Result<VerifiedCompletion,String>{
    let completion=backend.complete(prompt,options)?;
    if completion.text.trim().is_empty()||completion.text.len()>MAX_OUTPUT_BYTES{return Err("candidate output is empty or exceeds output cap".into())}
    verifier.verify(prompt,&completion.text).map_err(|e|format!("air-core verification rejected candidate: {e}"))?;
    Ok(VerifiedCompletion{completion})
}

#[derive(Debug,Clone,PartialEq,Eq)]
pub enum RoutedAnswer { NeuroSymbolic{output:String}, VerifiedSlm{output:String,elapsed:Duration} }
/// Run the AIR answer path first. Confident requests return that result; low
/// confidence or explicit deep/! requests escalate and require AIR-core verification.
pub fn route_and_run_verified<B,V,F>(
    backend:&B,verifier:&V,prompt:&str,confidence:f32,threshold:f32,options:&GenerationOptions,
    neuro_symbolic:F,
)->Result<RoutedAnswer,String>
where B:CompletionBackend,V:GeneratedOutputVerifier,F:FnOnce()->Result<String,String> {
    match crate::routing::route_prompt(prompt,confidence,threshold) {
        crate::routing::RouteDecision::NeuroSymbolicOnly=>{
            let output=neuro_symbolic()?;
            if output.trim().is_empty()||output.len()>MAX_OUTPUT_BYTES{return Err("AIR core output is empty or exceeds output cap".into())}
            Ok(RoutedAnswer::NeuroSymbolic{output})
        }
        crate::routing::RouteDecision::EscalateToSlm{..}=>{
            let user_prompt=prompt.trim_start();
            let clean=if let Some(rest)=user_prompt.strip_prefix('!'){rest.trim_start()}
                else if user_prompt.split_whitespace().next().is_some_and(|w|w.eq_ignore_ascii_case("deep")){
                    user_prompt.split_once(char::is_whitespace).map_or("",|(_,rest)|rest.trim_start())
                }else{user_prompt};
            let verified=run_verified(backend,verifier,clean,options)?;
            Ok(RoutedAnswer::VerifiedSlm{output:verified.completion.text,elapsed:verified.completion.elapsed})
        }
    }
}

#[cfg(test)]
mod behavior_tests {
 use super::*;
 struct Accept;
 impl GeneratedOutputVerifier for Accept { type Error=&'static str; fn verify(&self,q:&str,c:&str)->Result<(),Self::Error>{if q=="task"&&c=="answer"{Ok(())}else{Err("mismatch")}} }
 struct Reject;
 impl GeneratedOutputVerifier for Reject { type Error=&'static str; fn verify(&self,_:&str,_:&str)->Result<(),Self::Error>{Err("rejected")} }
 #[test] fn model_names_and_recommended_context(){assert_eq!(ModelFamily::Gemma4E2b.as_str(),"Gemma 4 E2B");assert_eq!(ModelFamily::SmolLm3B.as_str(),"SmolLM3 3B");assert_eq!(ModelFamily::Gemma4E2b.recommended_context(),4096);assert_eq!(ModelFamily::SmolLm3B.recommended_context(),4096);}
 #[test] fn rss_can_lower_but_not_raise_cap(){assert!(ResourceLimits{max_rss_bytes:1024,..Default::default()}.validate().is_ok());assert!(ResourceLimits{max_rss_bytes:MAX_RSS_BYTES,..Default::default()}.validate().is_ok());assert!(ResourceLimits{max_rss_bytes:MAX_RSS_BYTES+1,..Default::default()}.validate().is_err());assert!(ResourceLimits{max_rss_bytes:0,..Default::default()}.validate().is_err());}
 #[test] fn context_and_generation_caps(){assert!(ResourceLimits{context_tokens:1,max_new_tokens:1,..Default::default()}.validate().is_ok());assert!(ResourceLimits{context_tokens:MAX_CONTEXT_TOKENS,max_new_tokens:1,..Default::default()}.validate().is_ok());assert!(ResourceLimits{context_tokens:MAX_CONTEXT_TOKENS+1,..Default::default()}.validate().is_err());assert!(ResourceLimits{context_tokens:128,max_new_tokens:129,..Default::default()}.validate().is_err());assert!(ResourceLimits{max_new_tokens:0,..Default::default()}.validate().is_err());}
 #[test] fn reasoning_budget_toggle_is_bounded(){assert_eq!(ThinkingMode::Direct.budget().unwrap(),0);assert_eq!(ThinkingMode::Budgeted{reasoning_tokens:1}.budget().unwrap(),1);assert_eq!(ThinkingMode::Budgeted{reasoning_tokens:1024}.budget().unwrap(),1024);assert!(ThinkingMode::Budgeted{reasoning_tokens:0}.budget().is_err());assert!(ThinkingMode::Budgeted{reasoning_tokens:1025}.budget().is_err());}
 #[test] fn mock_is_repeatable_and_zero_latency(){let m=DeterministicMock::new("fixed");let a=m.complete("same",&GenerationOptions::default()).unwrap();let b=m.complete("same",&GenerationOptions::default()).unwrap();assert_eq!(a.text,"fixed");assert_eq!(a,b);assert_eq!(a.elapsed,Duration::ZERO);}
 #[test] fn mock_rejects_empty_large_prompt_and_invalid_mode(){let m=DeterministicMock::default();assert!(m.complete("",&GenerationOptions::default()).is_err());assert!(m.complete(&"x".repeat(MAX_PROMPT_BYTES+1),&GenerationOptions::default()).is_err());assert!(m.complete("q",&GenerationOptions{thinking:ThinkingMode::Budgeted{reasoning_tokens:0},max_new_tokens:1}).is_err());}
 #[test] fn air_core_verifier_accepts_exact_answer_and_rejects_other(){assert_eq!(run_verified(&DeterministicMock::new("answer"),&Accept,"task",&GenerationOptions::default()).unwrap().completion.text,"answer");assert!(run_verified(&DeterministicMock::new("wrong"),&Accept,"task",&GenerationOptions::default()).is_err());assert!(run_verified(&DeterministicMock::new("answer"),&Reject,"task",&GenerationOptions::default()).is_err());}
 #[test] fn verifier_not_called_for_empty_or_oversized_candidate(){assert!(run_verified(&DeterministicMock::new(" "),&Reject,"task",&GenerationOptions::default()).is_err());assert!(run_verified(&DeterministicMock::new("x".repeat(MAX_OUTPUT_BYTES+1)),&Accept,"task",&GenerationOptions::default()).is_err());}
 #[test] fn mock_thinking_toggle_preserves_fixture_output(){let m=DeterministicMock::new("stable");let direct=GenerationOptions{thinking:ThinkingMode::Direct,max_new_tokens:32};let budgeted=GenerationOptions{thinking:ThinkingMode::Budgeted{reasoning_tokens:32},max_new_tokens:32};assert_eq!(m.complete("p",&direct).unwrap().text,m.complete("p",&budgeted).unwrap().text);}
 #[test] fn high_confidence_uses_neuro_symbolic_callback(){let a=route_and_run_verified(&DeterministicMock::new("unused"),&Accept,"task",0.9,0.3,&GenerationOptions::default(),||Ok("symbolic".into())).unwrap();assert_eq!(a,RoutedAnswer::NeuroSymbolic{output:"symbolic".into()});}
 #[test] fn explicit_deep_uses_verified_slm_path(){let a=route_and_run_verified(&DeterministicMock::new("answer"),&Accept,"deep task",0.9,0.3,&GenerationOptions::default(),||panic!("should escalate")).unwrap();assert_eq!(a,RoutedAnswer::VerifiedSlm{output:"answer".into(),elapsed:Duration::ZERO});}
 #[test] fn bang_trigger_strips_prefix_and_checks_output(){let a=route_and_run_verified(&DeterministicMock::new("answer"),&Accept,"! task",0.9,0.3,&GenerationOptions::default(),||panic!("should escalate")).unwrap();assert!(matches!(a,RoutedAnswer::VerifiedSlm{output,..} if output=="answer"));}
 #[test] fn uncertain_request_escalates_and_rejected_slm_never_returns(){let a=route_and_run_verified(&DeterministicMock::new("answer"),&Accept,"task",0.1,0.3,&GenerationOptions::default(),||panic!("should escalate")).unwrap();assert!(matches!(a,RoutedAnswer::VerifiedSlm{..}));assert!(route_and_run_verified(&DeterministicMock::new("answer"),&Reject,"deep task",0.9,0.3,&GenerationOptions::default(),||Ok("unused".into())).is_err());}
}
