//! Extract teacher answers into a separate reference-checked candidate knowledge store.
use crate::{indexed::{Indexed,Result},model::LocalModel,store::Fact,consolidation::digest};
use serde::{Deserialize,Serialize};
use std::{collections::BTreeSet,time::Instant};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Case{pub subject:String,pub relation:String,pub expected:String,pub category:String,pub reference:String,pub question:String,pub paraphrase:String}
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Spec{pub teacher_id:String,pub cases:Vec<Case>}
pub trait Teacher{fn answer(&self,q:&str)->Result<Option<String>>;}
impl Teacher for LocalModel{fn answer(&self,q:&str)->Result<Option<String>>{let raw=self.complete_plain("Answer with only the short name or term, or unknown. Do not explain.",q)?;Ok(crate::elicitation::text(&raw,"answer").value)}}
fn norm(s:&str)->String{s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()}
#[derive(Debug,Serialize)]pub struct Outcome{pub subject:String,pub relation:String,pub teacher_answer:Option<String>,pub paraphrase_answer:Option<String>,pub candidate:bool,pub retrieved_correct:bool,pub teacher_paraphrase_correct:bool}
#[derive(Debug,Serialize)]pub struct Report{pub teacher_id:String,pub spec_sha256:String,pub total:usize,pub extracted:usize,pub teacher_paraphrase_correct:usize,pub retrieved_correct:usize,pub teacher_ns:u128,pub retrieval_ns:u128,pub outcomes:Vec<Outcome>}
pub fn convert(t:&dyn Teacher,s:&Spec,raw:&[u8],db:&mut Indexed)->Result<Report>{
 if s.teacher_id.trim().is_empty()||s.teacher_id.len()>512||s.cases.is_empty()||s.cases.len()>128||db.len()?!=0{return Err("knowledge spec bounds or nonempty store".into())}
 let mut seen=BTreeSet::new();for c in &s.cases{if [&c.subject,&c.relation,&c.expected,&c.category,&c.reference,&c.question,&c.paraphrase].iter().any(|x|x.trim().is_empty()||x.len()>1024||x.chars().any(char::is_control))||norm(&c.question)==norm(&c.paraphrase)||!seen.insert((norm(&c.subject),norm(&c.relation))){return Err("invalid or duplicate fact case".into())}}
 let mut candidates=vec![];let mut outcomes=vec![];let mut teacher_ns=0;let mut retrieval_ns=0;
 for c in &s.cases{let now=Instant::now();let a=t.answer(&c.question)?;teacher_ns+=now.elapsed().as_nanos();let matched=a.as_ref().is_some_and(|x|x.len()<=1024&&norm(x)==norm(&c.expected));
 if matched{candidates.push(Fact{subject:c.subject.clone(),relation:c.relation.clone(),object:a.as_ref().unwrap().trim().to_owned(),categories:[c.category.clone(),"amt-reference-matched".into()].into(),sources:[format!("teacher: {}",s.teacher_id),format!("reference: {}",c.reference)].into()});}
 outcomes.push(Outcome{subject:c.subject.clone(),relation:c.relation.clone(),teacher_answer:a,paraphrase_answer:None,candidate:matched,retrieved_correct:false,teacher_paraphrase_correct:false});}
 db.insert_batch(&candidates)?;
 for(c,o)in s.cases.iter().zip(outcomes.iter_mut()){let now=Instant::now();let a=t.answer(&c.paraphrase)?;teacher_ns+=now.elapsed().as_nanos();o.teacher_paraphrase_correct=a.as_ref().is_some_and(|x|norm(x)==norm(&c.expected));o.paraphrase_answer=a;
 let now=Instant::now();let r=db.lookup(&c.subject,&c.relation)?;retrieval_ns+=now.elapsed().as_nanos();o.retrieved_correct=!r.truncated&&r.facts.len()==1&&norm(&r.facts[0].object)==norm(&c.expected);}
 Ok(Report{teacher_id:s.teacher_id.clone(),spec_sha256:digest(raw),total:s.cases.len(),extracted:candidates.len(),teacher_paraphrase_correct:outcomes.iter().filter(|x|x.teacher_paraphrase_correct).count(),retrieved_correct:outcomes.iter().filter(|x|x.retrieved_correct).count(),teacher_ns,retrieval_ns,outcomes})
}
#[test]fn wrong_teacher_not_imported(){use crate::indexed::Limits;use std::path::Path;struct Wrong;impl Teacher for Wrong{fn answer(&self,_:&str)->Result<Option<String>>{Ok(Some("Lyon".into()))}}let s=Spec{teacher_id:"fixture".into(),cases:vec![Case{subject:"France".into(),relation:"capital".into(),expected:"Paris".into(),category:"geo".into(),reference:"fixture".into(),question:"Capital of France?".into(),paraphrase:"Which city is the French capital?".into()}]};let mut db=Indexed::create(Path::new(":memory:"),Limits::default()).unwrap();let r=convert(&Wrong,&s,b"test",&mut db).unwrap();assert_eq!(r.extracted,0);assert_eq!(db.len().unwrap(),0);}
