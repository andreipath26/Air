//! Explicit quality-first selection. Test-set evaluation is not model selection.
use crate::indexed::Result;
use serde::{Deserialize,Serialize};
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Candidate{pub name:String,pub total:u64,pub correct:u64,pub abstained:u64,pub runtime_ns:u64,pub bytes:u64}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Criteria{pub min_samples:u64,pub min_accuracy_percent:u64,pub min_coverage_percent:u64,pub max_bytes:u64}
pub fn choose(candidates:&[Candidate],criteria:&Criteria)->Result<Option<Candidate>>{
 if candidates.is_empty()||candidates.len()>32||criteria.min_samples==0||criteria.min_accuracy_percent>100||criteria.min_coverage_percent>100||criteria.max_bytes==0{return Err("invalid selection criteria".into())}
 let mut eligible=vec![];let mut names=std::collections::BTreeSet::new();for c in candidates{if c.name.is_empty()||c.name.len()>128||!names.insert(&c.name)||c.total==0||c.abstained>c.total||c.correct>c.total-c.abstained{return Err("invalid evaluation evidence".into())}if c.total>=criteria.min_samples&&c.bytes<=criteria.max_bytes&&(c.correct as u128)*100>=(c.total as u128)*(criteria.min_accuracy_percent as u128)&&((c.total-c.abstained)as u128)*100>=(c.total as u128)*(criteria.min_coverage_percent as u128){eligible.push(c);}}
 eligible.sort_by(|a,b|((a.runtime_ns as u128)*(b.correct.max(1)as u128)).cmp(&((b.runtime_ns as u128)*(a.correct.max(1)as u128))).then(a.bytes.cmp(&b.bytes)).then(a.name.cmp(&b.name)));Ok(eligible.first().map(|x|(*x).clone()))
}
#[test]fn quality_before_speed(){let a=Candidate{name:"fast".into(),total:100,correct:60,abstained:0,runtime_ns:1,bytes:1};let b=Candidate{name:"slow".into(),total:100,correct:95,abstained:0,runtime_ns:100,bytes:100};assert_eq!(choose(&[a,b],&Criteria{min_samples:100,min_accuracy_percent:90,min_coverage_percent:90,max_bytes:100}).unwrap().unwrap().name,"slow");}
