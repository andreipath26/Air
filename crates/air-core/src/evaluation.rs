//! Reproducible task-level fixture evaluator; never interprets latency as intelligence.
use crate::{engine::Engine,indexed::Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Case{pub id:String,pub question:String,pub expected:String,pub path:String}
#[derive(Debug,Serialize)]pub struct Outcome{pub id:String,pub correct:bool,pub path:String,pub model_called:bool,pub elapsed_ns:u128}
#[derive(Debug,Serialize)]pub struct Evaluation{pub total:usize,pub correct:usize,pub model_calls:usize,pub outcomes:Vec<Outcome>}
pub fn run(e:&Engine<'_>,cases:&[Case])->Result<Evaluation>{if cases.is_empty()||cases.len()>1000{return Err("case count".into())}let mut out=Evaluation{total:cases.len(),correct:0,model_calls:0,outcomes:vec![]};for c in cases{if c.id.len()>128||c.expected.is_empty()||c.expected.len()>4096{return Err("case bounds".into())}let start=std::time::Instant::now();let r=e.ask(&c.question)?;let elapsed_ns=start.elapsed().as_nanos();let correct=r.path==c.path&&r.text.contains(&c.expected);out.correct+=usize::from(correct);out.model_calls+=usize::from(r.model_called);out.outcomes.push(Outcome{id:c.id.clone(),correct,path:r.path.into(),model_called:r.model_called,elapsed_ns});}Ok(out)}
