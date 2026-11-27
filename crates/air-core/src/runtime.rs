//! Resident bounded JSON-line runtime over stdin/stdout; no network exposure.
use crate::{engine::Engine,indexed::Result,dataflow,plan};
use serde::{Deserialize,Serialize};
use std::io::{BufRead,Read,Write};
#[derive(Deserialize)]#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]enum Request{Ask{question:String},Plan{plan:plan::Plan},Dataflow{program:dataflow::Program},Health}
#[derive(Serialize)]struct Reply{ok:bool,result:Option<serde_json::Value>,error:Option<String>}
pub fn dispatch(e:&Engine<'_>,raw:&str)->Result<serde_json::Value>{if raw.len()>65536{return Err("request limit".into())}match serde_json::from_str::<Request>(raw)?{Request::Ask{question}=>{let r=e.ask(&question)?;Ok(serde_json::json!({"text":r.text,"path":r.path,"complete":r.complete,"model_called":r.model_called}))},Request::Plan{plan:p}=>Ok(serde_json::to_value(plan::execute(e,&p)?)?),Request::Dataflow{program:p}=>Ok(serde_json::to_value(dataflow::execute(e,&p)?)?),Request::Health=>Ok(serde_json::json!({"status":"ready","protocol":1}))}}
pub fn serve(e:&Engine<'_>,mut input:impl BufRead,mut output:impl Write)->Result<()>{loop{let mut line=String::new();let n=Read::take(&mut input,65537).read_line(&mut line)?;if n==0{return Ok(())}if n>65536{return Err("request too large".into())}let reply=match dispatch(e,&line){Ok(v)=>Reply{ok:true,result:Some(v),error:None},Err(_)=>Reply{ok:false,result:None,error:Some("invalid or failed request".into())}};serde_json::to_writer(&mut output,&reply)?;output.write_all(b"\n")?;output.flush()?;}}
