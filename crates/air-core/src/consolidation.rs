//! Conservative canonicalisation and independently reproducible source integrity checks.
use crate::{indexed::{Indexed,Result},policy::Policy,store::Fact};
use sha2::{Digest,Sha256};
use std::collections::BTreeSet;
pub fn digest(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
#[derive(Debug,PartialEq,Eq)]pub enum EvidenceCheck{Matches,WrongDigest,MissingQuote,WrongSource}
pub fn check_source(f:&Fact,source:&str,document:&str,expected_sha256:&str)->EvidenceCheck{
 if digest(document.as_bytes())!=expected_sha256{return EvidenceCheck::WrongDigest}
 let mut found=false;
 for s in &f.sources{if let Some((id,q))=s.split_once(" | quote: "){if id==source{found=true;if !q.trim().is_empty()&&document.contains(q){return EvidenceCheck::Matches}}}}
 if found{EvidenceCheck::MissingQuote}else{EvidenceCheck::WrongSource}
}
pub fn canonicalise(f:&Fact,p:&Policy)->Result<Fact>{
 let mut cats=BTreeSet::new();for c in &f.categories{cats.insert(p.canonical("category",c)?);}
 Ok(Fact{subject:p.canonical("subject",&f.subject)?,relation:p.canonical("relation",&f.relation)?,object:p.canonical("object",&f.object)?,categories:cats,sources:f.sources.clone()})
}
#[derive(Debug)]pub struct Consolidation{pub inserted:usize,pub merged:usize}
pub fn merge_batch(db:&mut Indexed,p:&Policy,facts:&[Fact])->Result<Consolidation>{
 if facts.len()>128{return Err("consolidation batch limit".into())}
 let canonical=facts.iter().map(|f|canonicalise(f,p)).collect::<Result<Vec<_>>>()?;
 let inserted=db.insert_batch(&canonical)?;Ok(Consolidation{inserted,merged:facts.len()-inserted})
}
#[test]fn aliases_and_source_integrity(){use crate::{indexed::Limits,store::parse_tsv};use std::path::Path;let p=Policy::open(Path::new(":memory:")).unwrap();p.alias("relation","capital city","capital").unwrap();let mut db=Indexed::create(Path::new(":memory:"),Limits::default()).unwrap();let a=parse_tsv("France\tcapital\tParis\tgeo\ts1").unwrap();let b=parse_tsv("France\tcapital city\tParis\tgeo\ts2 | quote: Paris is the capital.").unwrap();let r=merge_batch(&mut db,&p,&[a,b.clone()]).unwrap();assert_eq!(r.inserted,1);assert_eq!(r.merged,1);let doc="Paris is the capital.";assert_eq!(check_source(&b,"s2",doc,&digest(doc.as_bytes())),EvidenceCheck::Matches);assert_eq!(check_source(&b,"s2","tampered",&digest(doc.as_bytes())),EvidenceCheck::WrongDigest);}
