//! Portable bounded compiled capability packages. Hashes detect corruption, not publisher authenticity.
use crate::{indexed::Result,skills::{Skill,compile,execute},consolidation::digest};
use serde::{Deserialize,Serialize};
use std::{fs::File,io::Read,path::Path};
#[derive(Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Package{pub format:u32,pub report_sha256:String,pub teacher_id:String,pub skill:Skill,pub skill_sha256:String,pub test_total:usize,pub compiled_correct:usize}
impl Package{
 pub fn validate(&self)->Result<()>{if self.format!=1||self.report_sha256.len()!=64||!self.report_sha256.bytes().all(|b|b.is_ascii_hexdigit())||self.teacher_id.is_empty()||self.teacher_id.len()>512||self.test_total<10||self.test_total>256||self.compiled_correct!=self.test_total||digest(&serde_json::to_vec(&self.skill)?)!=self.skill_sha256{return Err("package integrity or metadata failure".into())}compile(&self.skill)?;Ok(())}
 /// Explicit experimental execution, not operator approval for integrated sessions.
 pub fn run(&self,args:&[i64])->Result<i64>{self.validate()?;execute(&self.skill,args)}
}
pub fn build(raw:&[u8],expected_hash:&str)->Result<Package>{if raw.len()>1048576||digest(raw)!=expected_hash{return Err("report integrity failed".into())}let v:serde_json::Value=serde_json::from_slice(raw)?;if v["pilot_passed"]!=true{return Err("pilot not passed".into())}let skill:Skill=serde_json::from_value(v["candidate"].clone())?;let p=Package{format:1,report_sha256:expected_hash.into(),teacher_id:v["teacher_id"].as_str().ok_or("teacher")?.into(),skill_sha256:digest(&serde_json::to_vec(&skill)?),skill,test_total:v["test_total"].as_u64().ok_or("test total")?.try_into()?,compiled_correct:v["compiled_correct"].as_u64().ok_or("correct")?.try_into()?};p.validate()?;Ok(p)}
pub fn load(path:&Path)->Result<Package>{let mut raw=vec![];File::open(path)?.take(1048577).read_to_end(&mut raw)?;if raw.len()>1048576{return Err("package size".into())}let p:Package=serde_json::from_slice(&raw)?;p.validate()?;Ok(p)}
#[test]fn integrity(){let skill=crate::skills::Skill{name:"fixture".into(),arity:0,body:crate::skills::Expr::Const{value:7}};let v=serde_json::json!({"pilot_passed":true,"teacher_id":"mock","candidate":skill,"test_total":10,"compiled_correct":10});let b=serde_json::to_vec(&v).unwrap();let mut p=build(&b,&digest(&b)).unwrap();assert_eq!(p.run(&[]).unwrap(),7);p.skill.body=crate::skills::Expr::Const{value:9};assert!(p.run(&[]).is_err());assert!(build(&b,"bad").is_err());}
