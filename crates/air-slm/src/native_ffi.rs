//! Optional direct local llama.cpp C++ worker backend.
//!
//! Enable with `llama-cpp-ffi` and set LLAMA_CPP_INCLUDE_DIR,
//! LLAMA_CPP_GGML_INCLUDE_DIR, and LLAMA_CPP_LIB_DIR at build time.
//! The persistent worker is launched under prlimit, uses mmap and Q8_0 KV,
//! and requires a caller-provided local GGUF. No model is downloaded.
use crate::grammar::OutputGrammar;
use crate::prompt::{StablePrompt, StablePromptBuilder};
use crate::runner::{Completion, CompletionBackend, GenerationOptions, ModelFamily, ResourceLimits, ThinkingMode};
use std::{
    fs, io::{BufReader, Read, Write}, path::Path, process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{atomic::{AtomicU32, Ordering}, Mutex}, time::{Duration, Instant},
};

const CODE_TEMPERATURE:f32=0.2;
const CODE_TOP_P:f32=0.95;
const MAX_PROMPT_BYTES:usize=16*1024;
const MAX_SYSTEM_BYTES:usize=8*1024;
const MAX_OUTPUT_BYTES:usize=32*1024;

struct Worker { child:Child, input:ChildStdin, output:BufReader<ChildStdout> }
pub struct NativeLlamaCppRunner { worker:Mutex<Worker>, limits:ResourceLimits, family:ModelFamily, last_tokens:AtomicU32 }

impl NativeLlamaCppRunner {
    pub fn launch(model_path:impl AsRef<Path>,family:ModelFamily,limits:ResourceLimits)->Result<Self,String>{
        let limits=limits.effective_for(family)?;
        let model=model_path.as_ref();
        if !model.is_file()||model.extension().and_then(|x|x.to_str())!=Some("gguf") {
            return Err("native llama.cpp requires a caller-provided local GGUF file".into());
        }
        let model=fs::canonicalize(model).map_err(|e|format!("GGUF path: {e}"))?;
        let worker_path=env!("AIR_LLAMA_CPP_WORKER_PATH");
        let child=Command::new("prlimit")
            .arg(format!("--as={}",limits.max_rss_bytes))
            .arg("--")
            .arg(worker_path).arg(model).arg(limits.context_tokens.to_string()).arg("4")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .spawn().map_err(|e|format!("prlimit-capped llama.cpp worker unavailable: {e}"))?;
        let mut child=child;
        let input=child.stdin.take().ok_or("missing worker stdin")?;
        let stdout=child.stdout.take().ok_or("missing worker stdout")?;
        let mut output=BufReader::new(stdout);
        let mut ready=[0u8;4];
        output.read_exact(&mut ready).map_err(|e|format!("llama.cpp worker failed during model load: {e}"))?;
        if &ready!=b"AIR1" { let _=child.kill(); let _=child.wait(); return Err("llama.cpp worker returned invalid readiness marker".into()); }
        Ok(Self{worker:Mutex::new(Worker{child,input,output}),limits,family,last_tokens:AtomicU32::new(0)})
    }

    pub fn family(&self)->ModelFamily { self.family }
    pub fn limits(&self)->ResourceLimits { self.limits }
    pub fn last_generated_tokens(&self)->u32 { self.last_tokens.load(Ordering::Relaxed) }

    pub fn complete_with_stable_prompt(&self,prompt:&StablePrompt,options:&GenerationOptions)->Result<Completion,String>{
        let system=prompt.system_prefix();
        let user=match options.thinking {
            ThinkingMode::Direct => prompt.user_prompt().to_owned(),
            ThinkingMode::Budgeted{reasoning_tokens} if (1..=1024).contains(&reasoning_tokens) =>
                format!("Reasoning budget: {reasoning_tokens} tokens. Return the concise final code only.\n{}",prompt.user_prompt()),
            _=>return Err("thinking budget must be in 1..=1024".into()),
        };
        if system.len()>MAX_SYSTEM_BYTES||user.is_empty()||user.len()>MAX_PROMPT_BYTES{return Err("native prompt exceeds byte limits".into())}
        let grammar=options.grammar.map(OutputGrammar::gbnf).unwrap_or("");
        let output_cap=MAX_OUTPUT_BYTES;
        let started=Instant::now();
        let mut worker=self.worker.lock().map_err(|_|"llama.cpp worker lock poisoned")?;
        if worker.child.try_wait().map_err(|e|e.to_string())?.is_some(){return Err("llama.cpp worker exited".into())}
        write_u32(&mut worker.input,system.len() as u32)?;
        write_u32(&mut worker.input,user.len() as u32)?;
        write_u32(&mut worker.input,grammar.len() as u32)?;
        write_u32(&mut worker.input,options.max_new_tokens.min(self.limits.max_new_tokens))?;
        worker.input.write_all(&CODE_TEMPERATURE.to_le_bytes()).map_err(|e|e.to_string())?;
        worker.input.write_all(&CODE_TOP_P.to_le_bytes()).map_err(|e|e.to_string())?;
        worker.input.write_all(system.as_bytes()).map_err(|e|e.to_string())?;
        worker.input.write_all(user.as_bytes()).map_err(|e|e.to_string())?;
        worker.input.write_all(grammar.as_bytes()).map_err(|e|e.to_string())?;
        worker.input.flush().map_err(|e|e.to_string())?;
        let status=read_u32(&mut worker.output)?;
        let length=read_u32(&mut worker.output)? as usize;
        let elapsed_us=read_u64(&mut worker.output)?;
        let tokens=read_u32(&mut worker.output)?;
        self.last_tokens.store(tokens,Ordering::Relaxed);
        if length>output_cap {return Err("native output exceeds byte cap".into())}
        let mut bytes=vec![0u8;length];
        worker.output.read_exact(&mut bytes).map_err(|e|e.to_string())?;
        let text=String::from_utf8(bytes).map_err(|e|e.to_string())?;
        if status!=0 {return Err(text)}
        let text=if let Some(grammar)=options.grammar {grammar.filter_output(&text)?}else{text};
        Ok(Completion{text,elapsed:if elapsed_us==0{started.elapsed()}else{Duration::from_micros(elapsed_us)}})
    }
}
impl CompletionBackend for NativeLlamaCppRunner {
    fn complete(&self,prompt:&str,options:&GenerationOptions)->Result<Completion,String>{
        let stable=StablePromptBuilder::default().build(prompt)?;
        self.complete_with_stable_prompt(&stable,options)
    }
}
impl Drop for NativeLlamaCppRunner {
    fn drop(&mut self){ if let Ok(worker)=self.worker.get_mut(){let _=worker.child.kill();let _=worker.child.wait();} }
}
fn write_u32(w:&mut impl Write,v:u32)->Result<(),String>{w.write_all(&v.to_le_bytes()).map_err(|e|e.to_string())}
fn read_u32(r:&mut impl Read)->Result<u32,String>{let mut b=[0;4];r.read_exact(&mut b).map_err(|e|e.to_string())?;Ok(u32::from_le_bytes(b))}
fn read_u64(r:&mut impl Read)->Result<u64,String>{let mut b=[0;8];r.read_exact(&mut b).map_err(|e|e.to_string())?;Ok(u64::from_le_bytes(b))}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn native_config_is_cpu_mmap_q8_and_four_threads(){
  let source=include_str!("llama_cpp_worker.cpp");
  for piece in ["LLAMA_LOAD_MODE_MMAP","n_gpu_layers=0","n_threads=n_threads","GGML_TYPE_Q8_0","llama_memory_seq_rm","llama_sampler_init_grammar"] {assert!(source.contains(piece),"missing native setting {piece}");}
 }

 /// Optional real-model measurement. It runs only when the feature is enabled
 /// and AIR_GGUF_PATH points to caller-owned local weights; CI never downloads them.
 #[test]
 fn local_gguf_throughput_opt_in() {
  let path=match std::env::var("AIR_GGUF_PATH") {Ok(path)=>path,Err(_)=>{eprintln!("AIR_GGUF_PATH unset; local GGUF measurement skipped");return;}};
  let family=match std::env::var("AIR_GGUF_FAMILY").as_deref(){Ok("smollm3")=>ModelFamily::SmolLm3B,_=>ModelFamily::Gemma4E2b};
  let limits=ResourceLimits{max_rss_bytes:6*1024*1024*1024,context_tokens:2048,max_new_tokens:128};
  let runner=NativeLlamaCppRunner::launch(path,family,limits).expect("load caller-provided local GGUF");
  let prompt=StablePromptBuilder::default().build("Write a Rust function that reverses a string slice.").unwrap();
  let options=GenerationOptions{max_new_tokens:128,..GenerationOptions::default()};
  let started=Instant::now();
  let result=runner.complete_with_stable_prompt(&prompt,&options).unwrap();
  let elapsed=result.elapsed.as_secs_f64().max(started.elapsed().as_secs_f64());
  let tokens=runner.last_generated_tokens();
  println!("AIR_GGUF_BENCH family={:?} generated_tokens={} latency_ms={:.3} tokens_per_second={:.3}",family,tokens,elapsed*1000.0,tokens as f64/elapsed.max(0.001));
  assert!(tokens>0);
 }
}
