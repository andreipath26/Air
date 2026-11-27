//! Bounded enumerative expression synthesis with separate demonstration and validation sets.
use crate::{indexed::Result,skills::{Expr,Skill,execute}};
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Deserialize)]#[serde(deny_unknown_fields)]pub struct Samples{pub train:Vec<(i64,i64,i64)>,pub validation:Vec<(i64,i64,i64)>}
#[derive(Debug)]pub struct Candidate{pub skill:Skill,pub examined:usize,pub train_matches:usize}
fn make(op:u8,a:Expr,b:Expr)->Expr{let(a,b)=(Box::new(a),Box::new(b));match op{0=>Expr::Add{a,b},1=>Expr::Sub{a,b},_=>Expr::Mul{a,b}}}
pub fn discover(name:&str,s:&Samples)->Result<Option<Candidate>>{
 if s.train.len()<3||s.train.len()>64||s.validation.len()<2||s.validation.len()>64{return Err("sample bounds".into())}
 let train:BTreeSet<_>=s.train.iter().map(|(a,b,_)|(*a,*b)).collect();let test:BTreeSet<_>=s.validation.iter().map(|(a,b,_)|(*a,*b)).collect();if train.len()!=s.train.len()||test.len()!=s.validation.len()||!train.is_disjoint(&test){return Err("duplicate or overlapping samples".into())}
 let mut leaves=vec![Expr::Arg{index:0},Expr::Arg{index:1}];for value in -2..=3{leaves.push(Expr::Const{value});}
 let mut level=vec![];for a in &leaves{for b in &leaves{for op in 0..3{level.push(make(op,a.clone(),b.clone()));}}}
 let mut examined=0;let mut matches=0;let mut best:Option<Skill>=None;let mut best_size=usize::MAX;
 let mut evaluate=|body:Expr|{examined+=1;let skill=Skill{name:name.into(),arity:2,body};if !s.train.iter().all(|(a,b,y)|execute(&skill,&[*a,*b]).ok()==Some(*y)){return;}matches+=1;let size=crate::skills::compile(&skill).map(|x|x.len()).unwrap_or(usize::MAX);if size<best_size&&s.validation.iter().all(|(a,b,y)|execute(&skill,&[*a,*b]).ok()==Some(*y)){best_size=size;best=Some(skill);}};
 for x in &leaves{evaluate(x.clone());}for x in &level{evaluate(x.clone());}for x in &level{for y in &leaves{for op in 0..3{evaluate(make(op,x.clone(),y.clone()));evaluate(make(op,y.clone(),x.clone()));}}}
 Ok(best.map(|skill|Candidate{skill,examined,train_matches:matches}))
}
#[test]fn compound_and_disjoint(){let s=Samples{train:vec![(2,3,7),(3,5,16),(4,7,29)],validation:vec![(6,9,55),(11,13,144)]};let c=discover("mul_plus_one",&s).unwrap().unwrap();assert_eq!(execute(&c.skill,&[8,10]).unwrap(),81);let bad=Samples{train:s.train.clone(),validation:vec![(2,3,7),(11,13,144)]};assert!(discover("bad",&bad).is_err());}
