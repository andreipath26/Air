//! Typed, finite dataflow over approved skills, reviewed knowledge and native operations.
use crate::{engine::Engine,indexed::Result,policy::Verdict};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Deserialize)]#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Input{Integer{value:i64},Text{value:String},Previous{index:usize}}
#[derive(Debug,Clone,Deserialize)]#[serde(tag="op",rename_all="snake_case",deny_unknown_fields)]
pub enum Node{Lookup{subject:Input,relation:String},Add{a:Input,b:Input},Multiply{a:Input,b:Input},Skill{name:String,args:Vec<Input>}}
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Program{pub nodes:Vec<Node>}
#[derive(Debug,Clone,Serialize,PartialEq,Eq)]#[serde(untagged)]pub enum Value{Integer(i64),Text(String)}
#[derive(Debug,Serialize)]pub struct Report{pub values:Vec<Value>,pub stopped:Option<String>}
fn input(i:&Input,v:&[Value])->Result<Value>{match i{Input::Integer{value}=>Ok(Value::Integer(*value)),Input::Text{value} if !value.is_empty()&&value.len()<=256=>Ok(Value::Text(value.clone())),Input::Previous{index}=>Ok(v.get(*index).ok_or("invalid forward reference")?.clone()),_=>Err("invalid input".into())}}
fn integer(i:&Input,v:&[Value])->Result<i64>{match input(i,v)?{Value::Integer(n)=>Ok(n),Value::Text(s)=>Ok(s.parse()?)}}
fn text(i:&Input,v:&[Value])->Result<String>{match input(i,v)?{Value::Text(s)=>Ok(s),Value::Integer(n)=>Ok(n.to_string())}}
pub fn execute(e:&Engine<'_>,p:&Program)->Result<Report>{
 if p.nodes.is_empty()||p.nodes.len()>16{return Err("dataflow node count".into())}
 let mut report=Report{values:vec![],stopped:None};
 for node in &p.nodes{let v=&report.values;let result=match node{
 Node::Lookup{subject,relation}=>{let s=e.policy.canonical("subject",&text(subject,v)?)?;let r=e.policy.canonical("relation",relation)?;let hits=e.knowledge.lookup(&s,&r)?;if hits.truncated||hits.facts.len()!=1||e.policy.assessment(&hits.facts[0])?.map(|x|x.verdict)!=Some(Verdict::Accepted){report.stopped=Some("lookup not uniquely reviewed".into());return Ok(report)}let o=&hits.facts[0].object;if o.len()>256{return Err("lookup value limit".into())}Value::Text(o.clone())},
 Node::Add{a,b}=>Value::Integer(integer(a,v)?.checked_add(integer(b,v)?).ok_or("overflow")?),
 Node::Multiply{a,b}=>Value::Integer(integer(a,v)?.checked_mul(integer(b,v)?).ok_or("overflow")?),
 Node::Skill{name,args}=>{if args.len()>8{return Err("arity limit".into())}let n=e.memory.latest_version(name)?.ok_or("unknown skill")?;if !e.policy.skill_approved(name,n)?{report.stopped=Some("skill not approved".into());return Ok(report)}let a=args.iter().map(|x|integer(x,v)).collect::<Result<Vec<_>>>()?;Value::Integer(e.memory.run_skill(name,Some(n),&a)?)}};
 report.values.push(result);
 }Ok(report)
}
#[test]fn forward_and_composition(){use crate::{indexed::{Indexed,Limits},cognition::Memory,policy::{Policy,Review},language::Language,store::parse_tsv};use std::path::Path;let mut d=Indexed::create(Path::new(":memory:"),Limits::default()).unwrap();d.insert(&parse_tsv("a\tvalue\t7\tfixture\ts").unwrap()).unwrap();let m=Memory::open(Path::new(":memory:")).unwrap();let p=Policy::open(Path::new(":memory:")).unwrap();p.review("a","value","7",&Review{verdict:Verdict::Accepted,reviewer:"test".into(),rationale:"fixture".into()}).unwrap();let e=Engine{knowledge:&d,memory:&m,policy:&p,model:None,language:Language::English};let program=Program{nodes:vec![Node::Lookup{subject:Input::Text{value:"a".into()},relation:"value".into()},Node::Multiply{a:Input::Previous{index:0},b:Input::Integer{value:6}}]};assert_eq!(execute(&e,&program).unwrap().values[1],Value::Integer(42));}
