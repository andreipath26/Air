//! Conservative extraction independent of reference answers.
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]#[serde(rename_all="snake_case")]pub enum Status{Structured,Plain,Abstained,Malformed}
#[derive(Debug,Clone,Serialize,Deserialize)]pub struct Extracted<T>{pub value:Option<T>,pub status:Status}
fn result<T>(value:Option<T>,status:Status)->Extracted<T>{Extracted{value,status}}
fn cleaned(raw:&str)->&str{let s=raw.trim();let fence="\x60\x60\x60";if let Some(x)=s.strip_prefix(fence).and_then(|x|x.strip_suffix(fence)){if let Some((_,body))=x.split_once('\n'){return body.trim()}}s}
fn unknown(s:&str)->bool{matches!(s.trim().trim_end_matches('.').to_ascii_lowercase().as_str(),"null"|"unknown"|"i don't know"|"uncertain"|"n/a")}
fn payload<'a>(v:&'a serde_json::Value,key:&str)->Option<&'a serde_json::Value>{let o=v.as_object()?;if o.len()!=1{return None}o.get(key)}
pub fn integer(raw:&str)->Extracted<i64>{
 if raw.len()>16384{return result(None,Status::Malformed)}let s=cleaned(raw);
 if let Ok(v)=serde_json::from_str::<serde_json::Value>(s){if let Some(x)=payload(&v,"answer"){return if x.is_null(){result(None,Status::Abstained)}else if let Some(n)=x.as_i64().or_else(||x.as_str().and_then(|s|s.parse().ok())){result(Some(n),Status::Structured)}else{result(None,Status::Malformed)}}}
 if unknown(s){return result(None,Status::Abstained)}
 let lower=s.to_ascii_lowercase();let body=if lower.starts_with("answer:"){s[7..].trim()}else if lower.starts_with("the answer is "){s[14..].trim()}else{s};
 match body.trim_end_matches('.').parse::<i64>(){Ok(n)=>result(Some(n),Status::Plain),Err(_)=>result(None,Status::Malformed)}
}
pub fn text(raw:&str,key:&str)->Extracted<String>{
 if raw.len()>16384{return result(None,Status::Malformed)}let s=cleaned(raw);
 if let Ok(v)=serde_json::from_str::<serde_json::Value>(s){if let Some(x)=payload(&v,key){return if x.is_null(){result(None,Status::Abstained)}else if let Some(t)=x.as_str().filter(|t|valid(t)){result(Some(t.trim().into()),Status::Structured)}else{result(None,Status::Malformed)}}}
 if unknown(s){return result(None,Status::Abstained)}
 let lower=s.to_ascii_lowercase();let prefix=format!("{key}:");let body=if lower.starts_with(&prefix){s[prefix.len()..].trim()}else{s};
 let body=body.trim_end_matches('.').trim();
 if valid(body)&&body.split_whitespace().count()<=8&&!body.chars().any(|c|matches!(c,'{'|'}'|'['|']'|':'|'?'|'!'|'"')){result(Some(body.into()),Status::Plain)}else{result(None,Status::Malformed)}
}
fn valid(s:&str)->bool{!s.trim().is_empty()&&s.len()<=256&&!s.chars().any(char::is_control)}
pub fn label(raw:&str,labels:&[String])->Extracted<String>{let x=text(raw,"label");if let Some(v)=x.value{if let Some(l)=labels.iter().find(|l|l.eq_ignore_ascii_case(&v)){return result(Some(l.clone()),x.status)}return result(None,Status::Malformed)}result(None,x.status)}
#[test]fn strict_and_plain(){assert_eq!(integer("The answer is -42.").value,Some(-42));assert_eq!(integer("2 + 3 = 5").value,None);assert_eq!(integer("null").status,Status::Abstained);assert_eq!(text("Paris.","answer").value.as_deref(),Some("Paris"));assert_eq!(text("{\"answer\":\"Paris\",\"extra\":1}","answer").status,Status::Malformed);assert!(label("vehicle engineering",&["automotive".into()]).value.is_none());assert_eq!(label("Automotive",&["automotive".into()]).value.as_deref(),Some("automotive"));}
