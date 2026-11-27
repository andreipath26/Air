//! Deterministic Graphmaster-inspired fast path with a typed, optional local SLM adapter.
use crate::{indexed::{Indexed,Result,Retrieved},curate::categories};
use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Route{Lookup{subject:String,relation:String},Category{category:String},Arithmetic{left:i64,right:i64,operator:String},Chain{subject:String,first:String,second:String},Unknown}
pub trait SemanticRouter {fn route(&self,question:&str)->Result<Route>;}
pub fn deterministic(q:&str)->Route{
 let q=q.trim().trim_end_matches(['?','.']).to_lowercase();
 for prefix in ["what is the ","what's the ","tell me the "]{if let Some(x)=q.strip_prefix(prefix){if let Some((r,s))=x.split_once(" of "){if !r.is_empty()&&!s.is_empty(){return Route::Lookup{subject:s.into(),relation:r.into()}}}}}
 if let Some(x)=q.strip_prefix("category "){return Route::Category{category:x.into()}}
 let x=q.strip_prefix("calculate ").unwrap_or(&q);
 let tokens:Vec<_>=x.split_whitespace().collect();if tokens.len()==3 {if let (Ok(left),Ok(right))=(tokens[0].parse(),tokens[2].parse()){if ["+","-","*","×","/"].contains(&tokens[1]){return Route::Arithmetic{left,right,operator:tokens[1].into()}}}}
 Route::Unknown
}
pub fn validate(route:Route)->Result<Route>{let valid=|s:&str|!s.trim().is_empty()&&s.len()<=256&&!s.chars().any(char::is_control);match &route{Route::Lookup{subject,relation} if valid(subject)&&valid(relation)=>{},Route::Category{category} if valid(category)=>{},Route::Arithmetic{operator,..} if ["+","-","*","×","/"].contains(&operator.as_str())=>{},Route::Chain{subject,first,second} if valid(subject)&&valid(first)&&valid(second)=>{},Route::Unknown=>{},_=>return Err("invalid route".into())}Ok(route)}
pub fn parse_model_route(json:&str)->Result<Route>{if json.len()>4096{return Err("router output too long".into())}validate(serde_json::from_str(json)?)}
pub fn choose(q:&str,slm:Option<&dyn SemanticRouter>)->Result<Route>{if q.len()>4096{return Err("question too long".into())}let r=deterministic(q);if r!=Route::Unknown{return validate(r)}if let Some(m)=slm{return validate(m.route(q)?)}Ok(Route::Unknown)}
#[derive(Debug)]pub enum Answer{Claims(Retrieved),Number(i64),Paths(crate::reasoning::Paths),Unknown{suggested_categories:Vec<&'static str>}}
pub fn execute(db:&Indexed,q:&str,route:Route)->Result<Answer>{match validate(route)?{Route::Lookup{subject,relation}=>Ok(Answer::Claims(db.lookup(&subject,&relation)?)),Route::Category{category}=>Ok(Answer::Claims(db.category(&category)?)),Route::Arithmetic{left,right,operator}=>{let n=match operator.as_str(){"+"=>left.checked_add(right),"-"=>left.checked_sub(right),"*"|"×"=>left.checked_mul(right),"/"=>left.checked_div(right),_=>None};Ok(Answer::Number(n.ok_or("arithmetic overflow or division by zero")?))},Route::Chain{subject,first,second}=>Ok(Answer::Paths(crate::reasoning::chain(db,&subject,&first,&second)?)),Route::Unknown=>Ok(Answer::Unknown{suggested_categories:categories(q)})}}
#[test]fn routes(){assert_eq!(deterministic("What is the capital of France?"),Route::Lookup{subject:"france".into(),relation:"capital".into()});assert!(parse_model_route(r#"{"kind":"shell","command":"x"}"#).is_err());assert!(parse_model_route(r#"{"kind":"category","category":"geo","extra":true}"#).is_err());}
