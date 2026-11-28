//! CPU llama.cpp server integration, resource bounds, and verified escalation.
use air_core::verification::GeneratedOutputVerifier;
use serde_json::{json, Value};
use std::{fs, io::Read, path::Path, process::{Child, Command, Stdio}, sync::Mutex, time::{Duration, Instant}};
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
impl ThinkingMode { fn budget(self)->Result<u16,String>{match self {Self::Direct=>Ok(0),Self::Budgeted{reasoning_tokens} if (1..=1024).contains(&reasoning_tokens)=>Ok(reasoning_tokens),_=>Err("thinking budget must be in 1..=1024".into())}} }
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ResourceLimits { pub max_rss_bytes:u64,pub context_tokens:u32,pub max_new_tokens:u32 }
impl Default for ResourceLimits { fn default()->Self{Self{max_rss_bytes:6*1024*1024*1024,context_tokens:2048,max_new_tokens:256}} }
impl ResourceLimits {
 pub fn validate(self)->Result<Self,String>{
  if self.max_rss_bytes==0||self.max_rss_bytes>MAX_RSS_BYTES{return Err(format!("RSS cap must be in 1..={MAX_RSS_BYTES} bytes"))}
  if self.context_tokens==0||self.context_tokens>MAX_CONTEXT_TOKENS{return Err(format!("context cap must be in 1..={MAX_CONTEXT_TOKENS} tokens"))}
  if self.max_new_tokens==0||self.max_new_tokens>self.context_tokens{return Err("generation cap must fit within context cap".into())} Ok(self)
 }
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct GenerationOptions {pub thinking:ThinkingMode,pub max_new_tokens:u32}
impl Default for GenerationOptions {fn default()->Self{Self{thinking:ThinkingMode::Direct,max_new_tokens:256}}}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Completion {pub text:String,pub elapsed:Duration}
pub trait CompletionBackend:Send+Sync {fn complete(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>}

/// Launches a local GGUF through llama.cpp's loopback OpenAI-compatible server.
/// RSS is sampled around each request and the server is killed if over its configured ceiling.
pub struct LlamaCppRunner {child:Mutex<Child>,endpoint:String,model_name:String,limits:ResourceLimits}
impl LlamaCppRunner {
 pub fn launch(executable:impl AsRef<Path>,gguf:impl AsRef<Path>,family:ModelFamily,port:u16,limits:ResourceLimits)->Result<Self,String>{
  let limits=limits.validate()?;if port==0{return Err("port must be nonzero".into())}
  let file=gguf.as_ref();if !file.is_file()||file.extension().and_then(|x|x.to_str())!=Some("gguf"){return Err("model must be an existing GGUF file".into())}
  let file=fs::canonicalize(file).map_err(|e|format!("GGUF path: {e}"))?;
  let threads=std::thread::available_parallelism().map_or(2,usize::from).min(4).to_string();
  let child=Command::new(executable).arg("--model").arg(file).arg("--host").arg("127.0.0.1").arg("--port").arg(port.to_string())
   .arg("--ctx-size").arg(limits.context_tokens.to_string()).arg("--n-gpu-layers").arg("0").arg("--threads").arg(threads).arg("--no-webui")
   .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e|format!("starting llama.cpp server: {e}"))?;
  Ok(Self{child:Mutex::new(child),endpoint:format!("http://127.0.0.1:{port}/v1/chat/completions"),model_name:family.as_str().into(),limits})
 }
 pub fn limits(&self)->ResourceLimits{self.limits}
 fn rss_bytes(&self)->Result<u64,String>{
  let c=self.child.lock().map_err(|_|"runner lock poisoned")?;
  let status=fs::read_to_string(format!("/proc/{}/status",c.id())).map_err(|e|format!("reading llama.cpp RSS: {e}"))?;
  let kb=status.lines().find(|x|x.starts_with("VmRSS:")).and_then(|x|x.split_whitespace().nth(1)).ok_or("VmRSS missing")?.parse::<u64>().map_err(|e|e.to_string())?;
  Ok(kb.saturating_mul(1024))
 }
 fn stop(&self){if let Ok(mut c)=self.child.lock(){let _=c.kill();let _=c.wait();}}
 fn request(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>{
  let token_cap=options.max_new_tokens.min(self.limits.max_new_tokens);if token_cap==0||token_cap>self.limits.context_tokens{return Err("invalid output token budget".into())}
  let thinking=options.thinking.budget()?;
  if let Ok(rss)=self.rss_bytes(){if rss>self.limits.max_rss_bytes{self.stop();return Err(format!("RSS cap exceeded: {rss} > {}",self.limits.max_rss_bytes))}}
  let started=Instant::now();
  let system=if thinking==0{"Respond directly and concisely; do not emit a reasoning trace."}else{"Reason within the configured thinking budget, then provide a concise answer."};
  let body=json!({"model":self.model_name,"messages":[{"role":"system","content":system},{"role":"user","content":prompt}],"temperature":0,"max_tokens":token_cap,"stream":false,
     "chat_template_kwargs":{"enable_thinking":thinking>0,"reasoning_budget":thinking}});
  let agent=ureq::AgentBuilder::new().redirects(0).timeout_connect(Duration::from_secs(2)).timeout(Duration::from_secs(120)).build();
  let response=agent.post(&self.endpoint).send_json(body).map_err(|e|e.to_string())?;
  let mut raw=String::new();response.into_reader().take((MAX_OUTPUT_BYTES*4+1) as u64).read_to_string(&mut raw).map_err(|e|e.to_string())?;
  if raw.len()>MAX_OUTPUT_BYTES*4{return Err("llama.cpp response exceeds byte cap".into())}
  let v:Value=serde_json::from_str(&raw).map_err(|e|e.to_string())?;
  let text=v.pointer("/choices/0/message/content").and_then(Value::as_str).ok_or("missing llama.cpp message content")?;
  if text.len()>MAX_OUTPUT_BYTES{return Err("generated output exceeds byte cap".into())}
  if let Ok(rss)=self.rss_bytes(){if rss>self.limits.max_rss_bytes{self.stop();return Err(format!("RSS cap exceeded after generation: {rss} > {}",self.limits.max_rss_bytes))}}
  Ok(Completion{text:text.to_owned(),elapsed:started.elapsed()})
 }
}
impl CompletionBackend for LlamaCppRunner {
 fn complete(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>{
  if prompt.is_empty()||prompt.len()>MAX_PROMPT_BYTES{return Err("prompt is empty or exceeds byte cap".into())}self.request(prompt,options)
 }
}
impl Drop for LlamaCppRunner {fn drop(&mut self){self.stop()}}
/// Stable headless backend; every request returns the same configured result.
pub struct DeterministicMock{response:String}
impl DeterministicMock{pub fn new(response:impl Into<String>)->Self{Self{response:response.into()}}}
impl Default for DeterministicMock{fn default()->Self{Self::new("mock completion")}}
impl CompletionBackend for DeterministicMock{
 fn complete(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>{
  if prompt.is_empty()||prompt.len()>MAX_PROMPT_BYTES{return Err("prompt is empty or exceeds byte cap".into())}
  options.thinking.budget()?;Ok(Completion{text:self.response.clone(),elapsed:Duration::ZERO})
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
#[cfg(test)]
mod tests{
 use super::*;
 struct Accept;
 impl GeneratedOutputVerifier for Accept{type Error=&'static str;fn verify(&self,_:&str,c:&str)->Result<(),Self::Error>{if c=="mock completion"{Ok(())}else{Err("unexpected")}}}
 struct Reject;
 impl GeneratedOutputVerifier for Reject{type Error=&'static str;fn verify(&self,_:&str,_:&str)->Result<(),Self::Error>{Err("unverified")}}
 #[test]fn caps_are_bounded(){assert!(ResourceLimits::default().validate().is_ok());assert!(ResourceLimits{max_rss_bytes:MAX_RSS_BYTES+1,..Default::default()}.validate().is_err());assert!(ResourceLimits{context_tokens:MAX_CONTEXT_TOKENS+1,..Default::default()}.validate().is_err())}
 #[test]fn mock_must_pass_air_core_verifier(){let x=run_verified(&DeterministicMock::default(),&Accept,"deep task",&GenerationOptions::default()).unwrap();assert_eq!(x.completion.text,"mock completion");assert!(run_verified(&DeterministicMock::default(),&Reject,"deep task",&GenerationOptions::default()).is_err())}
}
