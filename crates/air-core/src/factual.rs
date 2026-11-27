//! Closed-world factual retrieval with explicit relation templates and entity aliases.
//! No test question or reference answer is required by the router.
use crate::{indexed::{Indexed,Result},policy::{Policy,Verdict}};
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,BTreeSet};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Entity{pub canonical:String,pub aliases:Vec<String>}
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Relation{pub canonical:String,pub patterns:Vec<String>}
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Spec{pub entities:Vec<Entity>,pub relations:Vec<Relation>}
#[derive(Debug,Serialize)]pub struct Answer{pub answer:Option<String>,pub reason:&'static str,pub subject:Option<String>,pub relation:Option<String>,pub sources:Vec<String>}
pub struct Router{entities:BTreeMap<String,String>,patterns:Vec<(String,String,String)>}
fn normalize(s:&str)->String{s.trim().trim_end_matches(['?','.','!']).split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()}
fn valid(s:&str)->bool{!s.trim().is_empty()&&s.len()<=256&&!s.chars().any(char::is_control)}
impl Router{
 pub fn new(spec:Spec)->Result<Self>{
 if spec.entities.is_empty()||spec.entities.len()>10000||spec.relations.is_empty()||spec.relations.len()>128{return Err("router capacity".into())}
 let mut entities=BTreeMap::new();let mut patterns=vec![];
 for e in spec.entities{if !valid(&e.canonical)||e.aliases.len()>32{return Err("entity bounds".into())}let canonical=normalize(&e.canonical);for a in std::iter::once(e.canonical).chain(e.aliases){if !valid(&a){return Err("alias bounds".into())}let a=normalize(&a);if a.is_empty(){return Err("empty normalized alias".into())}if let Some(old)=entities.insert(a,canonical.clone()){if old!=canonical{return Err("ambiguous entity alias".into())}}}}
 for r in spec.relations{if !valid(&r.canonical)||r.patterns.is_empty()||r.patterns.len()>32{return Err("relation bounds".into())}for p in r.patterns{if !valid(&p){return Err("pattern bounds".into())}let p=normalize(&p);let (pre,post)=p.split_once("{entity}").ok_or("missing entity placeholder")?;if post.contains("{entity}")||pre.is_empty()&&post.is_empty(){return Err("invalid template".into())}patterns.push((pre.into(),post.into(),normalize(&r.canonical)));}}
 Ok(Self{entities,patterns})
 }
 pub fn route(&self,q:&str)->Result<Option<(String,String)>>{if q.len()>4096{return Err("question limit".into())}let q=normalize(q);let mut hits=BTreeSet::new();for (pre,post,r) in &self.patterns{if let Some(x)=q.strip_prefix(pre).and_then(|s|s.strip_suffix(post)){if let Some(s)=self.entities.get(x.trim()){hits.insert((s.clone(),r.clone()));}}}Ok(if hits.len()==1{hits.into_iter().next()}else{None})}
 pub fn ask(&self,db:&Indexed,policy:&Policy,q:&str)->Result<Answer>{let Some((s,r))=self.route(q)? else{return Ok(Answer{answer:None,reason:"unsupported_or_ambiguous_question",subject:None,relation:None,sources:vec![]})};
 let found=db.lookup(&s,&r)?;let mut answer=Answer{answer:None,reason:"missing_or_conflicting_fact",subject:Some(s),relation:Some(r),sources:vec![]};
 if found.truncated||found.facts.len()!=1{return Ok(answer)}let f=&found.facts[0];if policy.assessment(f)?.map(|x|x.verdict)!=Some(Verdict::Accepted){answer.reason="unreviewed_fact";return Ok(answer)}answer.answer=Some(f.object.clone());answer.sources=f.sources.iter().cloned().collect();answer.reason="reviewed_fact";Ok(answer)}
}
#[test]fn paraphrase_unknown_and_conflict(){use crate::{indexed::Limits,store::parse_tsv,policy::Review};use std::path::Path;let router=Router::new(Spec{entities:vec![Entity{canonical:"France".into(),aliases:vec!["French Republic".into()]}],relations:vec![Relation{canonical:"capital".into(),patterns:vec!["what is the capital of {entity}?".into(),"which city serves as the capital of {entity}?".into()]}]}).unwrap();let mut db=Indexed::create(Path::new(":memory:"),Limits::default()).unwrap();let p=Policy::open(Path::new(":memory:")).unwrap();db.insert(&parse_tsv("France\tcapital\tParis\tgeography\tfixture").unwrap()).unwrap();assert!(router.ask(&db,&p,"Which city serves as the capital of French Republic?").unwrap().answer.is_none());p.review("France","capital","Paris",&Review{verdict:Verdict::Accepted,reviewer:"fixture".into(),rationale:"synthetic".into()}).unwrap();assert_eq!(router.ask(&db,&p,"Which city serves as the capital of French Republic?").unwrap().answer.as_deref(),Some("Paris"));assert!(router.ask(&db,&p,"What is the population of France?").unwrap().answer.is_none());db.insert(&parse_tsv("France\tcapital\tLyon\tgeography\tfixture").unwrap()).unwrap();assert!(router.ask(&db,&p,"What is the capital of France?").unwrap().answer.is_none());}
