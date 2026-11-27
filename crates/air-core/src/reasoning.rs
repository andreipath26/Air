//! Bounded two-hop composition; paths carry evidence, not asserted new facts.
use crate::{indexed::{Indexed,Result},store::Fact};
#[derive(Debug)]pub struct Path{pub first:Fact,pub second:Fact}
#[derive(Debug)]pub struct Paths{pub paths:Vec<Path>,pub truncated:bool}
pub fn chain(db:&Indexed,subject:&str,first:&str,second:&str)->Result<Paths>{
 let a=db.lookup(subject,first)?;let mut out=Paths{paths:vec![],truncated:a.truncated};let mut bytes=0usize;
 for f in a.facts.into_iter().take(8){let b=db.lookup(&f.object,second)?;out.truncated|=b.truncated;for g in b.facts{let size=|x:&Fact|x.subject.len()+x.relation.len()+x.object.len()+x.categories.iter().chain(x.sources.iter()).map(String::len).sum::<usize>();bytes+=size(&f)+size(&g);if out.paths.len()>=32||bytes>256*1024{out.truncated=true;return Ok(out)}out.paths.push(Path{first:f.clone(),second:g});}}
 Ok(out)
}
#[test]fn chain_provenance(){use crate::{indexed::Limits,store::parse_tsv};let mut db=Indexed::create(std::path::Path::new(":memory:"),Limits::default()).unwrap();for l in ["France\tcapital\tParis\tgeo\ts1","Paris\tcountry\tFrance\tgeo\ts2"]{db.insert(&parse_tsv(l).unwrap()).unwrap();}let p=chain(&db,"France","capital","country").unwrap();assert_eq!(p.paths.len(),1);assert!(p.paths[0].second.sources.contains("s2"));}
