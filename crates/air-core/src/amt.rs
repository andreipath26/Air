//! First behavioural transcompiler: restricted binary integer functions.
use crate::{indexed::Result,model::LocalModel,synthesis::{self,Samples},skills::{self,Skill},consolidation::digest};
use serde::{Deserialize,Serialize};
use std::{collections::BTreeSet,time::Instant};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Case{pub a:i64,pub b:i64,pub expected:i64}
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Spec{pub name:String,pub task:String,pub teacher_id:String,pub teacher_license:String,pub train:Vec<Case>,pub validation:Vec<Case>,pub test:Vec<Case>}
pub trait Teacher{fn answer(&self,task:&str,a:i64,b:i64)->Result<Option<i64>>;}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Reply{answer:Option<i64>}
impl Teacher for LocalModel{fn answer(&self,task:&str,a:i64,b:i64)->Result<Option<i64>>{let raw=self.complete("Solve the supplied task for the two integer inputs. Return JSON with only answer: integer or null if uncertain.",&serde_json::json!({"task":task,"a":a,"b":b}).to_string())?;Ok(serde_json::from_str::<Reply>(&raw)?.answer)}}
#[derive(Debug,Serialize)]pub struct Report{pub teacher_id:String,pub teacher_license:String,pub spec_sha256:String,pub train_total:usize,pub train_usable:usize,pub validation_total:usize,pub validation_usable:usize,pub test_total:usize,pub teacher_correct:usize,pub compiled_correct:usize,pub agreement:usize,pub teacher_ns:u128,pub compiled_ns:u128,pub conversion_ns:u128,pub candidate:Option<Skill>,pub pilot_passed:bool}
pub fn convert(t:&dyn Teacher,s:&Spec,spec_bytes:&[u8])->Result<Report>{
 if s.name.is_empty()||s.name.len()>128||s.task.is_empty()||s.task.len()>1024||s.teacher_id.trim().is_empty()||s.teacher_id.len()>512||s.teacher_license.trim().is_empty()||s.teacher_license.len()>256||!(3..=64).contains(&s.train.len())||!(2..=64).contains(&s.validation.len())||!(10..=256).contains(&s.test.len()){return Err("invalid pilot spec".into())}
 let mut unique=BTreeSet::new();for c in s.train.iter().chain(&s.validation).chain(&s.test){if !unique.insert((c.a,c.b)){return Err("split overlap".into())}}
 let start=Instant::now();
 let gather=|cases:&[Case]|->Result<Vec<(i64,i64,i64)>>{let mut out=vec![];for c in cases{if t.answer(&s.task,c.a,c.b)?==Some(c.expected){out.push((c.a,c.b,c.expected));}}Ok(out)};
 let train=gather(&s.train)?;let validation=gather(&s.validation)?;
 let (nt,nv)=(train.len(),validation.len());
 let candidate=if nt>=3&&nv>=2{synthesis::discover(&s.name,&Samples{train,validation})?.map(|c|c.skill)}else{None};
 let conversion_ns=start.elapsed().as_nanos();
 let(mut tc,mut cc,mut agreement,mut tn,mut cn)=(0,0,0,0,0);
 for c in &s.test{let now=Instant::now();let a=t.answer(&s.task,c.a,c.b)?;tn+=now.elapsed().as_nanos();let now=Instant::now();let b=candidate.as_ref().and_then(|sk|skills::execute(sk,&[c.a,c.b]).ok());cn+=now.elapsed().as_nanos();tc+=usize::from(a==Some(c.expected));cc+=usize::from(b==Some(c.expected));agreement+=usize::from(a.is_some()&&a==b);}
 let passed=candidate.is_some()&&cc==s.test.len()&&tc>0;
 Ok(Report{teacher_id:s.teacher_id.clone(),teacher_license:s.teacher_license.clone(),spec_sha256:digest(spec_bytes),train_total:s.train.len(),train_usable:nt,validation_total:s.validation.len(),validation_usable:nv,test_total:s.test.len(),teacher_correct:tc,compiled_correct:cc,agreement,teacher_ns:tn,compiled_ns:cn,conversion_ns,candidate,pilot_passed:passed})
}
#[test]fn fixture_transfer(){struct Mock;impl Teacher for Mock{fn answer(&self,_:&str,a:i64,b:i64)->Result<Option<i64>>{Ok(Some(a*b+1))}}let c=|a,b|Case{a,b,expected:a*b+1};let s=Spec{name:"fixture".into(),task:"multiply and add one".into(),teacher_id:"mock-not-a-model".into(),teacher_license:"fixture".into(),train:vec![c(2,3),c(3,5),c(4,7)],validation:vec![c(6,9),c(11,13)],test:(20..30).map(|a|c(a,2)).collect()};let r=convert(&Mock,&s,b"fixture").unwrap();assert!(r.pilot_passed);assert_eq!(r.compiled_correct,10);}
