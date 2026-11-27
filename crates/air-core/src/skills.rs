//! Restricted integer procedure IR, interpreter, constant-folding compiler and trace-based library induction.
use crate::indexed::Result;
use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(tag="op",rename_all="snake_case",deny_unknown_fields)]
pub enum Expr{Arg{index:u8},Const{value:i64},Add{a:Box<Expr>,b:Box<Expr>},Sub{a:Box<Expr>,b:Box<Expr>},Mul{a:Box<Expr>,b:Box<Expr>}}
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skill{pub name:String,pub arity:u8,pub body:Expr}
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum Instruction{Arg(u8),Const(i64),Add,Sub,Mul}
fn binary(op:&Instruction,a:i64,b:i64)->Result<i64>{match op{Instruction::Add=>a.checked_add(b),Instruction::Sub=>a.checked_sub(b),Instruction::Mul=>a.checked_mul(b),_=>None}.ok_or_else(||"integer overflow or invalid instruction".into())}
fn emit(e:&Expr,arity:u8,depth:u8,out:&mut Vec<Instruction>)->Result<()>{
 if depth>24||out.len()>=256{return Err("procedure complexity limit".into())}
 match e{Expr::Arg{index} if *index<arity=>out.push(Instruction::Arg(*index)),Expr::Arg{..}=>return Err("argument index".into()),Expr::Const{value}=>out.push(Instruction::Const(*value)),Expr::Add{a,b}|Expr::Sub{a,b}|Expr::Mul{a,b}=>{emit(a,arity,depth+1,out)?;emit(b,arity,depth+1,out)?;out.push(match e{Expr::Add{..}=>Instruction::Add,Expr::Sub{..}=>Instruction::Sub,_=>Instruction::Mul});}}
 if out.len()>256{return Err("procedure size limit".into())}Ok(())
}
pub fn compile(s:&Skill)->Result<Vec<Instruction>>{
 if s.name.is_empty()||s.name.len()>128||s.arity>8{return Err("invalid skill metadata".into())}
 let mut raw=vec![];emit(&s.body,s.arity,0,&mut raw)?;
 let mut stack:Vec<Vec<Instruction>>=vec![];
 for op in raw{match op{Instruction::Arg(_)|Instruction::Const(_)=>stack.push(vec![op]),_=>{let b=stack.pop().ok_or("stack underflow")?;let mut a=stack.pop().ok_or("stack underflow")?;if let ([Instruction::Const(x)],[Instruction::Const(y)])=(&a[..],&b[..]){if let Ok(v)=binary(&op,*x,*y){stack.push(vec![Instruction::Const(v)]);continue;}}a.extend(b);a.push(op);stack.push(a);}}}
 stack.pop().ok_or_else(||"empty program".into())
}
pub fn run(code:&[Instruction],args:&[i64],budget:usize)->Result<i64>{
 if code.len()>budget||code.len()>256||args.len()>8{return Err("execution budget".into())}let mut stack=vec![];
 for op in code{match op{Instruction::Arg(i)=>stack.push(*args.get(*i as usize).ok_or("missing argument")?),Instruction::Const(v)=>stack.push(*v),_=>{let b=stack.pop().ok_or("stack underflow")?;let a=stack.pop().ok_or("stack underflow")?;stack.push(binary(op,a,b)?);}}}
 if stack.len()!=1{return Err("invalid final stack".into())}Ok(stack[0])
}
pub fn execute(s:&Skill,args:&[i64])->Result<i64>{if args.len()!=s.arity as usize{return Err("arity mismatch".into())}run(&compile(s)?,args,256)}
/// Enumerate a tiny grammar of binary integer expressions; accept only a unique matching candidate.
pub fn induce_binary(name:&str,examples:&[(i64,i64,i64)])->Result<Option<Skill>>{
 if examples.len()<3||examples.len()>128{return Err("need 3..128 demonstrations".into())}
 if examples.iter().map(|(a,b,_)|(*a,*b)).collect::<std::collections::BTreeSet<_>>().len()<3{return Err("need distinct inputs".into())}
 let mut matches=vec![];
 for kind in 0..4{let a=Box::new(Expr::Arg{index:0});let b=Box::new(Expr::Arg{index:1});let body=match kind{0=>Expr::Add{a,b},1=>Expr::Sub{a,b},2=>Expr::Sub{a:b,b:a},_=>Expr::Mul{a,b}};let s=Skill{name:name.into(),arity:2,body};if examples.iter().all(|(a,b,y)|execute(&s,&[*a,*b]).ok()==Some(*y)){matches.push(s);}}
 Ok(if matches.len()==1{matches.pop()}else{None})
}
#[cfg(test)]mod tests{use super::*;
#[test]fn generalises(){let s=induce_binary("multiply",&[(2,3,6),(3,5,15),(4,7,28)]).unwrap().unwrap();assert_eq!(execute(&s,&[11,13]).unwrap(),143);assert!(execute(&s,&[i64::MAX,2]).is_err());}
#[test]fn ambiguous(){assert!(induce_binary("ambiguous",&[(0,0,0),(0,1,0),(1,0,0)]).unwrap().is_some());assert!(induce_binary("none",&[(1,1,9),(2,2,9),(3,3,9)]).unwrap().is_none());}
#[test]fn folds_and_bounds(){let s=Skill{name:"c".into(),arity:0,body:Expr::Add{a:Box::new(Expr::Const{value:2}),b:Box::new(Expr::Const{value:3})}};assert_eq!(compile(&s).unwrap(),vec![Instruction::Const(5)]);assert!(run(&compile(&s).unwrap(),&[],0).is_err());}}
