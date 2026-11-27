//! Bounded explicit rule evaluation. No implicit transitivity or probabilistic truth claims.
use crate::{indexed::{Indexed,Result},store::Fact,policy::{Policy,Verdict}};
use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Rule{pub name:String,pub first:String,pub second:String,pub conclusion:String}
#[derive(Debug)]pub struct Derivation{pub subject:String,pub relation:String,pub object:String,pub premises:[Fact;2]}
#[derive(Debug)]pub struct Deductions{pub results:Vec<Derivation>,pub incomplete:bool}
pub fn apply(db:&Indexed,policy:&Policy,subject:&str,rule:&Rule)->Result<Deductions>{
 for s in [&rule.name,&rule.first,&rule.second,&rule.conclusion]{if s.trim().is_empty()||s.len()>256{return Err("invalid rule".into())}}
 let a=db.lookup(subject,&rule.first)?;let mut out=Deductions{results:vec![],incomplete:a.truncated||a.facts.len()>8};let mut bytes=0;
 let accepted=|f:&Fact|->Result<bool>{Ok(policy.assessment(f)?.map(|r|r.verdict)==Some(Verdict::Accepted))};
 for f in a.facts.into_iter().take(8){if !accepted(&f)?{continue}let b=db.lookup(&f.object,&rule.second)?;out.incomplete|=b.truncated;for g in b.facts{if !accepted(&g)?{continue}
 let size=|x:&Fact|x.subject.len()+x.relation.len()+x.object.len()+x.sources.iter().chain(x.categories.iter()).map(String::len).sum::<usize>();
 bytes+=size(&f)+size(&g)+f.subject.len()+rule.conclusion.len()+g.object.len();
 if out.results.len()>=32||bytes>256*1024{out.incomplete=true;return Ok(out)}
 out.results.push(Derivation{subject:f.subject.clone(),relation:rule.conclusion.clone(),object:g.object.clone(),premises:[f.clone(),g]});}}
 Ok(out)
}
#[test]fn only_reviewed_premises(){use crate::{indexed::Limits,store::parse_tsv,policy::Review};use std::path::Path;let mut d=Indexed::create(Path::new(":memory:"),Limits::default()).unwrap();let p=Policy::open(Path::new(":memory:")).unwrap();let facts=[parse_tsv("A\tparent\tB\tfamily\ts1").unwrap(),parse_tsv("B\tparent\tC\tfamily\ts2").unwrap()];for f in &facts{d.insert(f).unwrap();}let r=Rule{name:"grandparent".into(),first:"parent".into(),second:"parent".into(),conclusion:"grandparent".into()};assert!(apply(&d,&p,"A",&r).unwrap().results.is_empty());for f in &facts{p.review(&f.subject,&f.relation,&f.object,&Review{verdict:Verdict::Accepted,reviewer:"test".into(),rationale:"fixture".into()}).unwrap();}assert_eq!(apply(&d,&p,"A",&r).unwrap().results[0].object,"C");}
