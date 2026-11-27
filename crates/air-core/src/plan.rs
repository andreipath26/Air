//! Typed, bounded multi-stage execution; no arbitrary program or host calls.
use crate::{indexed::Result,engine::Engine};
use serde::{Deserialize,Serialize};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Plan{pub steps:Vec<Step>}
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Step{pub question:String}
#[derive(Debug,Serialize)]pub struct StepResult{pub text:String,pub path:String,pub model_called:bool,pub complete:bool}
#[derive(Debug,Serialize)]pub struct Report{pub steps:Vec<StepResult>,pub stopped:Option<String>}
pub fn execute(e:&Engine<'_>,plan:&Plan)->Result<Report>{
 if plan.steps.is_empty()||plan.steps.len()>8{return Err("plan requires 1..8 steps".into())}
 let mut report=Report{steps:vec![],stopped:None};let mut bytes=0usize;
 for step in &plan.steps{
 if step.question.len()>4096{return Err("plan question limit".into())}
 // Plans are operator-authored and cannot recursively execute plans or mutate memory.
 let r=e.ask(&step.question)?;
 bytes=bytes.saturating_add(r.text.len());
 if bytes>512*1024{report.stopped=Some("output budget".into());break}
 let complete=r.complete;report.steps.push(StepResult{text:r.text,path:r.path.into(),model_called:r.model_called,complete});
 if !complete{report.stopped=Some("unresolved step".into());break}
 }Ok(report)
}
