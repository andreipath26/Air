//! Optional local SLM adapter. Not yet wired into air-core; no hosted API or model download.

use std::{env, fmt, io::{Read, Write}, net::{SocketAddr, TcpListener, TcpStream}, path::PathBuf,
          process::{Child, Command, Stdio}, thread, time::{Duration, Instant}};

const MAX_HTTP_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

fn bounded_value(value:u32,min:u32,max:u32)->u32{value.clamp(min,max)}
fn bounded_env(name:&str,default:u32,min:u32,max:u32)->Result<u32,SlmError>{
 let raw=match env::var(name){Ok(v)=>v,Err(env::VarError::NotPresent)=>return Ok(bounded_value(default,min,max)),Err(e)=>return Err(SlmError::Config(format!("{name}: {e}")))};
 let value=raw.parse::<u32>().map_err(|e|SlmError::Config(format!("{name}: {e}")))?;
 Ok(bounded_value(value,min,max))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlmError { Config(String), Io(String), Http(u16, String), Protocol(String), Backend(String) }
impl fmt::Display for SlmError {
 fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { match self {
  Self::Config(s)=>write!(f,"configuration: {s}"),Self::Io(s)=>write!(f,"I/O: {s}"),
  Self::Http(c,s)=>write!(f,"HTTP {c}: {s}"),Self::Protocol(s)=>write!(f,"protocol: {s}"),Self::Backend(s)=>write!(f,"backend: {s}") } }
}
impl std::error::Error for SlmError {}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ThinkingMode { Off{max_tokens:u32}, Bounded{max_tokens:u32} }
impl ThinkingMode { fn limit(self)->u32 { match self {Self::Off{max_tokens}|Self::Bounded{max_tokens}=>max_tokens} } }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct SlmRequest { pub system:String, pub prompt:String, pub mode:ThinkingMode, pub template_thinking_key:Option<String> }
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum SlmRole { Translator, TieBreaker, PatchFallback }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct RoleReply { pub role:SlmRole, pub content:String }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct ParsedPrompt { pub text:String, pub explicit_deep:bool }

pub trait SlmBackend { fn complete(&mut self,request:&SlmRequest)->Result<String,SlmError>; fn shutdown(&mut self)->Result<(),SlmError>{Ok(())} }
impl<T:SlmBackend+?Sized> SlmBackend for &mut T {
 fn complete(&mut self,r:&SlmRequest)->Result<String,SlmError>{(**self).complete(r)}
 fn shutdown(&mut self)->Result<(),SlmError>{(**self).shutdown()}
}

/// Consume only a leading deep token or exclamation mark.
pub fn parse_escalation(input:&str)->ParsedPrompt {
 let s=input.trim_start();
 if let Some(rest)=s.strip_prefix('!'){return ParsedPrompt{text:rest.trim_start().into(),explicit_deep:true};}
 if s.get(..4).map(|x|x.eq_ignore_ascii_case("deep")).unwrap_or(false)
 && s.as_bytes().get(4).map(|b|b.is_ascii_whitespace()).unwrap_or(s.len()==4) {
  return ParsedPrompt{text:s.get(4..).unwrap_or("").trim_start().into(),explicit_deep:true};
 }
 ParsedPrompt{text:input.into(),explicit_deep:false}
}

/// Remove common thought/channel wrappers before every role response is parsed.
pub fn strip_thought_blocks(input:&str)->String {
 let mut s=input.to_owned();
 for (open,closes) in [("<think>",&["</think>"][..]),("<|channel>thought",&["<|channel>final","<channel|>"][..]),("<|channel>analysis",&["<|channel>final","<channel|>"][..])] {
  loop {
   let Some(i)=s.find(open) else {break};
   let tail=&s[i+open.len()..];
   let close=closes.iter().filter_map(|m|tail.find(m).map(|p|(p,*m))).min_by_key(|(p,_)|*p);
   if let Some((p,m))=close { let mut n=s[..i].to_owned(); n.push_str(&tail[p+m.len()..]); s=n; }
   else {s.truncate(i);break;}
  }
 }
 s
}

#[derive(Debug,Clone)]
pub struct LlamaServerConfig {
 pub server_bin:PathBuf,pub model_path:PathBuf,pub context_tokens:u32,pub threads:u32,
 pub startup_timeout:Duration,pub request_timeout:Duration,
 /// Model-specific and opt-in; server/template may ignore it.
 pub thinking_template_key:Option<String>,
}
impl LlamaServerConfig {
 pub fn from_env()->Result<Self,SlmError>{
  let model_path=env::var_os("AIR_SLM_MODEL_PATH").map(PathBuf::from).ok_or_else(||SlmError::Config("AIR_SLM_MODEL_PATH is required".into()))?;
  let server_bin=env::var_os("LLAMA_SERVER_BIN").map(PathBuf::from).unwrap_or_else(||"llama-server".into());
  let context_tokens=bounded_env("AIR_SLM_CONTEXT",4096,256,8192)?;
  let default_threads=thread::available_parallelism().map(|n|n.get() as u32).unwrap_or(2);
  let threads=bounded_env("AIR_SLM_THREADS",default_threads,1,8)?;
  Ok(Self{server_bin,model_path,context_tokens,threads,startup_timeout:Duration::from_secs(90),request_timeout:Duration::from_secs(120),thinking_template_key:env::var("AIR_SLM_THINKING_TEMPLATE_KEY").ok()})
 }
}

/// Lazy local llama-server; loaded on first request, kept until shutdown or drop.
pub struct LlamaServerBackend { cfg:LlamaServerConfig, child:Option<Child>, port:Option<u16> }
impl LlamaServerBackend {
 pub fn new(cfg:LlamaServerConfig)->Self{Self{cfg,child:None,port:None}}
 fn start(&mut self)->Result<u16,SlmError>{
  if let (Some(c),Some(p))=(self.child.as_mut(),self.port){if c.try_wait().map_err(ioerr)?.is_none(){return Ok(p)} self.child=None;self.port=None;}
  let probe=TcpListener::bind(("127.0.0.1",0)).map_err(ioerr)?;let port=probe.local_addr().map_err(ioerr)?.port();drop(probe);
  self.child=Some(Command::new(&self.cfg.server_bin).arg("-m").arg(&self.cfg.model_path).arg("--host").arg("127.0.0.1").arg("--port").arg(port.to_string()).arg("-ngl").arg("0").arg("-c").arg(self.cfg.context_tokens.to_string()).arg("-t").arg(self.cfg.threads.to_string()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(ioerr)?);
  self.port=Some(port);let t=Instant::now();
  while t.elapsed()<self.cfg.startup_timeout {
   if let Some(c)=self.child.as_mut(){if let Some(st)=c.try_wait().map_err(ioerr)?{self.child=None;self.port=None;return Err(SlmError::Backend(format!("llama-server exited: {st}")));}}
   if http(port,"GET","/health","",self.cfg.request_timeout).map(|(s,_)|(200..300).contains(&s)).unwrap_or(false){return Ok(port)}
   thread::sleep(Duration::from_millis(100));
  }
  let _=self.shutdown();Err(SlmError::Backend("llama-server startup timed out".into()))
 }
}
impl SlmBackend for LlamaServerBackend {
 fn complete(&mut self,r:&SlmRequest)->Result<String,SlmError>{
  let p=self.start()?;let mut req=r.clone();if req.template_thinking_key.is_none(){req.template_thinking_key=self.cfg.thinking_template_key.clone();}
  let (status,body)=http(p,"POST","/v1/chat/completions",&request_json(&req),self.cfg.request_timeout)?;
  if !(200..300).contains(&status){return Err(SlmError::Http(status,body))}
  Ok(strip_thought_blocks(&extract_content(&body)?))
 }
 fn shutdown(&mut self)->Result<(),SlmError>{if let Some(mut c)=self.child.take(){if c.try_wait().map_err(ioerr)?.is_none(){c.kill().map_err(ioerr)?;}c.wait().map_err(ioerr)?;}self.port=None;Ok(())}
}
impl Drop for LlamaServerBackend{fn drop(&mut self){let _=self.shutdown();}}

pub struct RoleOrchestrator<B>{backend:B,fast:u32,deep:u32}
impl<B:SlmBackend> RoleOrchestrator<B>{
 pub fn new(backend:B)->Self{Self{backend,fast:256,deep:1024}}
 pub fn with_budgets(mut self,fast:u32,deep:u32)->Self{self.fast=fast.max(1);self.deep=deep.max(1);self}
 /// Caller supplies AIR confidence and candidates; this crate does not decide AIR routing.
 pub fn route(&mut self,prompt:&str,low_confidence:bool,ambiguous:bool,candidates:&[String])->Result<Option<RoleReply>,SlmError>{
  let p=parse_escalation(prompt);
  let (role,system,task,mode)=if p.explicit_deep{
   (SlmRole::PatchFallback,"Propose a minimal patch or ABSTAIN. Do not claim verification.",p.text,ThinkingMode::Bounded{max_tokens:self.deep})
  }else if ambiguous{
   (SlmRole::Translator,"Translate into a concise canonical coding specification; ask or abstain if essential details are missing.",p.text,ThinkingMode::Off{max_tokens:self.fast})
  }else if low_confidence&&!candidates.is_empty(){
   let options=candidates.iter().enumerate().map(|(i,c)|format!("{}. {}",i+1,c)).collect::<Vec<_>>().join("\n");
   (SlmRole::TieBreaker,"Choose only from supplied candidates or ABSTAIN.",format!("Task: {}\nCandidates:\n{}",p.text,options),ThinkingMode::Off{max_tokens:self.fast})
  }else if low_confidence {
   (SlmRole::PatchFallback,"AIR has no candidate; propose a minimal patch or ABSTAIN. Do not claim verification.",p.text,ThinkingMode::Bounded{max_tokens:self.deep})
  }else{return Ok(None)};
  let req=SlmRequest{system:system.into(),prompt:task,mode,template_thinking_key:None};
  let content=strip_thought_blocks(&self.backend.complete(&req)?);
  Ok(Some(RoleReply{role,content}))
 }
 pub fn backend_mut(&mut self)->&mut B{&mut self.backend}
}

fn request_json(r:&SlmRequest)->String{
 let kw=r.template_thinking_key.as_deref().map(|k|format!(",\"chat_template_kwargs\":{{{}:{}}}",json_str(k),matches!(r.mode,ThinkingMode::Bounded{..}))).unwrap_or_default();
 format!("{{\"model\":\"local\",\"messages\":[{{\"role\":\"system\",\"content\":{}}},{{\"role\":\"user\",\"content\":{}}}],\"temperature\":0,\"max_tokens\":{}{} }}",json_str(&r.system),json_str(&r.prompt),r.mode.limit(),kw)
}
fn json_str(s:&str)->String{let mut o=String::from("\"");for c in s.chars(){match c{'"'=>o.push_str("\\\""),'\\'=>o.push_str("\\\\"),'\n'=>o.push_str("\\n"),'\r'=>o.push_str("\\r"),'\t'=>o.push_str("\\t"),c if c<' '=>o.push_str(&format!("\\u{:04x}",c as u32)),c=>o.push(c)}}o.push('"');o}
fn extract_content(s:&str)->Result<String,SlmError>{
 let i=s.find("\"content\"").ok_or_else(||SlmError::Protocol("missing content".into()))?+9;
 let v=s[i..].trim_start().strip_prefix(':').ok_or_else(||SlmError::Protocol("invalid content field".into()))?.trim_start();
 let b=v.as_bytes();if b.first()!=Some(&b'"'){return Err(SlmError::Protocol("content is not a string".into()))}
 let(mut o,mut i)=(String::new(),1);
 while i<b.len(){match b[i]{b'"'=>return Ok(o),b'\\'=>{i+=1;match *b.get(i).ok_or_else(||SlmError::Protocol("truncated escape".into()))?{b'"'=>o.push('"'),b'\\'=>o.push('\\'),b'/'=>o.push('/'),b'n'=>o.push('\n'),b'r'=>o.push('\r'),b't'=>o.push('\t'),_=>return Err(SlmError::Protocol("unsupported JSON escape".into()))}},_=>{let tail=std::str::from_utf8(&b[i..]).map_err(|e|SlmError::Protocol(e.to_string()))?;let c=tail.chars().next().ok_or_else(||SlmError::Protocol("invalid UTF-8".into()))?;o.push(c);i+=c.len_utf8()-1;}}i+=1}
 Err(SlmError::Protocol("unterminated JSON content".into()))
}
fn http(port:u16,method:&str,path:&str,body:&str,timeout:Duration)->Result<(u16,String),SlmError>{
 let addr=SocketAddr::from(([127,0,0,1],port));let mut s=TcpStream::connect_timeout(&addr,Duration::from_secs(2)).map_err(ioerr)?;
 s.set_read_timeout(Some(timeout)).map_err(ioerr)?;s.set_write_timeout(Some(Duration::from_secs(10))).map_err(ioerr)?;
 let req=format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
 s.write_all(req.as_bytes()).map_err(ioerr)?;let mut b=Vec::new();let mut chunk=[0u8;8192];loop{let n=s.read(&mut chunk).map_err(ioerr)?;if n==0{break}if b.len()+n>MAX_HTTP_RESPONSE_BYTES{return Err(SlmError::Protocol("HTTP response exceeds 8 MiB cap".into()))}b.extend_from_slice(&chunk[..n]);}
 let i=b.windows(4).position(|w|w==b"\r\n\r\n").ok_or_else(||SlmError::Protocol("invalid HTTP response".into()))?;
 let head=std::str::from_utf8(&b[..i]).map_err(|e|SlmError::Protocol(e.to_string()))?;
 let status=head.lines().next().and_then(|l|l.split_whitespace().nth(1)).and_then(|x|x.parse().ok()).ok_or_else(||SlmError::Protocol("invalid HTTP status".into()))?;
 let mut bytes=b[i+4..].to_vec();
 if head.to_ascii_lowercase().contains("transfer-encoding: chunked"){bytes=decode_chunks(&bytes)?;}
 let body=String::from_utf8(bytes).map_err(|e|SlmError::Protocol(e.to_string()))?;Ok((status,body))
}
fn decode_chunks(mut rest:&[u8])->Result<Vec<u8>,SlmError>{
 let mut out=Vec::new();loop{
  let end=rest.windows(2).position(|w|w==b"\r\n").ok_or_else(||SlmError::Protocol("invalid chunk header".into()))?;
  let line=std::str::from_utf8(&rest[..end]).map_err(|e|SlmError::Protocol(e.to_string()))?;
  let size=usize::from_str_radix(line.split(';').next().unwrap_or("").trim(),16).map_err(|e|SlmError::Protocol(e.to_string()))?;
  rest=&rest[end+2..];if size==0{return Ok(out)}
  if rest.len()<size+2||&rest[size..size+2]!=b"\r\n"{return Err(SlmError::Protocol("truncated chunk".into()))}
  out.extend_from_slice(&rest[..size]);rest=&rest[size+2..];
 }
}
fn ioerr(e:std::io::Error)->SlmError{SlmError::Io(e.to_string())}

#[cfg(test)] mod tests{
 use super::*;
 #[derive(Default)] struct Mock{calls:Vec<SlmRequest>,answer:String}
 impl SlmBackend for Mock{fn complete(&mut self,r:&SlmRequest)->Result<String,SlmError>{self.calls.push(r.clone());Ok(self.answer.clone())}}
 fn run(prompt:&str,ambiguous:bool,low:bool,candidates:&[String])->(RoleReply,SlmRequest){
  let mut mock=Mock{answer:"<think>hidden</think><|channel>thoughtsecret<|channel>finalpatch".into(),..Mock::default()};
  let mut roles=RoleOrchestrator::new(&mut mock).with_budgets(32,96);
  let reply=roles.route(prompt,low,ambiguous,candidates).unwrap().unwrap();
  assert_eq!(reply.content,"patch");
  let request=roles.backend_mut().calls[0].clone();
  (reply,request)
 }
 #[test]fn deep_and_bang_trigger_patch_fallback(){
  for prompt in ["deep repair","! repair"] {let (r,q)=run(prompt,false,false,&[]);assert_eq!(r.role,SlmRole::PatchFallback);assert_eq!(q.mode,ThinkingMode::Bounded{max_tokens:96});}
 }
 #[test]fn ambiguous_translator_strips_thought(){
  let (r,q)=run("repair",true,false,&[]);assert_eq!(r.role,SlmRole::Translator);assert_eq!(q.mode,ThinkingMode::Off{max_tokens:32});
 }
 #[test]fn candidate_tiebreaker_strips_thought(){
  let (r,q)=run("repair",false,true,&["candidate".into()]);assert_eq!(r.role,SlmRole::TieBreaker);assert!(q.prompt.contains("candidate"));assert_eq!(q.mode,ThinkingMode::Off{max_tokens:32});
 }
 #[test]fn low_confidence_without_candidates_uses_novel_patch_fallback(){
  let (r,q)=run("repair",false,true,&[]);assert_eq!(r.role,SlmRole::PatchFallback);assert_eq!(q.mode,ThinkingMode::Bounded{max_tokens:96});
 }
 #[test]fn high_confidence_does_not_call_backend(){
  let mut m=Mock::default();let mut o=RoleOrchestrator::new(&mut m);assert!(o.route("repair",false,false,&[]).unwrap().is_none());assert!(o.backend_mut().calls.is_empty());
 }
 #[test]fn parser_only_consumes_leading_trigger(){
  assert_eq!(parse_escalation("  deep repair").text,"repair");assert!(parse_escalation("! repair").explicit_deep);
  assert!(!parse_escalation("deeply repair").explicit_deep);assert!(!parse_escalation("please go deep").explicit_deep);
 }
 #[test]fn every_returned_role_path_strips_thought(){
  for (ambiguous,low,candidates,prompt,expected) in [
   (true,false,&[][..],"repair",SlmRole::Translator),
   (false,true,&["a".to_owned()][..],"repair",SlmRole::TieBreaker),
   (false,true,&[][..],"repair",SlmRole::PatchFallback),
   (false,false,&[][..],"! repair",SlmRole::PatchFallback),
  ] {let (r,_)=run(prompt,ambiguous,low,candidates);assert_eq!(r.role,expected);assert!(!r.content.contains("hidden"));assert!(!r.content.contains("secret"));}
 }
 #[test]fn response_budget_settings_are_hard_capped(){
  assert_eq!(bounded_value(100_000,256,8192),8192);assert_eq!(bounded_value(100,1,8),8);assert_eq!(bounded_value(0,1,8),1);
 }
 #[test]fn extracts_chat_content(){
  assert_eq!(extract_content("{\"choices\":[{\"message\":{\"content\":\"hi\\nthere\"}}]}").unwrap(),"hi\nthere");
 }
}
