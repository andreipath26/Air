//! Bounded, reference-driven capability interrogation. Negative results remain visible.
use crate::{indexed::Result,model::LocalModel,consolidation::digest,elicitation::{self,Status}};
use serde::{Deserialize,Serialize};
use std::{collections::{BTreeMap,BTreeSet},time::Instant};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Probe{pub id:String,pub domain:String,pub question:String,pub expected:Option<String>}
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Spec{pub teacher_id:String,pub probes:Vec<Probe>}
pub trait Teacher{fn answer(&self,q:&str)->Result<String>;}
impl Teacher for LocalModel{fn answer(&self,q:&str)->Result<String>{self.complete_plain("Answer with only a short term or integer, or unknown. Do not explain.",q)}}
#[derive(Debug,Serialize)]pub struct Outcome{pub id:String,pub domain:String,pub raw:String,pub extracted:Option<String>,pub extraction_status:Status,pub correct:bool,pub elapsed_ns:u128}
#[derive(Debug,Default,Serialize)]pub struct Domain{pub total:usize,pub correct:usize,pub malformed:usize,pub abstained:usize,pub unexpected_answers:usize}
#[derive(Debug,Serialize)]pub struct Report{pub teacher_id:String,pub spec_sha256:String,pub domains:BTreeMap<String,Domain>,pub outcomes:Vec<Outcome>}
fn norm(s:&str)->String{s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()}
pub fn interrogate(t:&dyn Teacher,s:&Spec,raw:&[u8])->Result<Report>{
 if s.teacher_id.is_empty()||s.teacher_id.len()>512||s.probes.is_empty()||s.probes.len()>256{return Err("probe bounds".into())}
 let mut ids=BTreeSet::new();let mut questions=BTreeSet::new();
 for p in &s.probes{if p.id.is_empty()||p.id.len()>128||p.domain.is_empty()||p.domain.len()>128||p.question.trim().is_empty()||p.question.len()>1024||p.expected.as_ref().is_some_and(|x|x.trim().is_empty()||x.len()>256)||!ids.insert(&p.id)||!questions.insert(norm(&p.question)){return Err("invalid or overlapping probe".into())}}
 let mut domains:BTreeMap<String,Domain>=BTreeMap::new();let mut outcomes=vec![];
 for p in &s.probes{let now=Instant::now();let response=t.answer(&p.question)?;let elapsed_ns=now.elapsed().as_nanos();if response.len()>16384{return Err("response bounds".into())}let x=elicitation::text(&response,"answer");let correct=match(&p.expected,&x.value){(Some(a),Some(b))=>norm(a)==norm(b),(None,None)=>x.status==Status::Abstained,_=>false};let d=domains.entry(p.domain.clone()).or_default();d.total+=1;d.correct+=usize::from(correct);d.malformed+=usize::from(x.status==Status::Malformed);d.abstained+=usize::from(x.status==Status::Abstained);d.unexpected_answers+=usize::from(!correct&&x.value.is_some());outcomes.push(Outcome{id:p.id.clone(),domain:p.domain.clone(),raw:response,extracted:x.value,extraction_status:x.status,correct,elapsed_ns});}
 Ok(Report{teacher_id:s.teacher_id.clone(),spec_sha256:digest(raw),domains,outcomes})
}
#[test]fn separates_errors(){struct Mock;impl Teacher for Mock{fn answer(&self,q:&str)->Result<String>{Ok(match q{"a"=>"Paris","b"=>"unknown",_=>"{bad"}.into())}}let s=Spec{teacher_id:"fixture".into(),probes:vec![Probe{id:"a".into(),domain:"geo".into(),question:"a".into(),expected:Some("Paris".into())},Probe{id:"b".into(),domain:"geo".into(),question:"b".into(),expected:None},Probe{id:"c".into(),domain:"geo".into(),question:"c".into(),expected:None}]};let r=interrogate(&Mock,&s,b"fixture").unwrap();assert_eq!(r.domains["geo"].correct,2);assert_eq!(r.domains["geo"].malformed,1);}
