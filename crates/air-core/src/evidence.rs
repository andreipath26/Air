//! Explicit review decisions and ambiguity handling. Source count alone never verifies a claim.
use crate::{indexed::Retrieved,store::Fact};
#[derive(Debug,PartialEq,Eq)]pub enum Assessment{Missing,Incomplete,SingleUnverified,MultipleValues}
pub fn assess(r:&Retrieved)->Assessment{if r.truncated{return Assessment::Incomplete}match r.facts.len(){0=>Assessment::Missing,1=>Assessment::SingleUnverified,_=>Assessment::MultipleValues}}
pub fn quote_anchored(f:&Fact,text:&str)->bool{f.sources.iter().any(|s|s.split_once(" | quote: ").map(|(_,q)|!q.is_empty()&&text.contains(q)).unwrap_or(false))}
#[test]fn no_false_verification(){let r=Retrieved{facts:vec![],bytes:0,truncated:true};assert_eq!(assess(&r),Assessment::Incomplete);}
