//! Bounded, deterministic prototype. In-memory indexes; not yet a disk-backed engine.
use std::collections::{BTreeMap,BTreeSet};
use std::io::{self,Read,Write};
const MAGIC: &[u8;8]=b"AIRIR001";
const MAX_FIELD: usize=65536;
const MAX_RECORDS: usize=100000;
const MAX_ITEMS: usize=1024;
const MAX_BYTES: usize=64*1024*1024;
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Fact { pub subject:String,pub relation:String,pub object:String,pub categories:BTreeSet<String>,pub sources:BTreeSet<String> }
fn normalize(s:&str)->String { s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }
fn invalid(msg:&str)->io::Error { io::Error::new(io::ErrorKind::InvalidData,msg) }
fn validate_field(s:&str)->io::Result<()> { if s.trim().is_empty() || s.len()>MAX_FIELD { Err(invalid("invalid field size")) } else { Ok(()) } }
type Key=(String,String,String);
#[derive(Default)]
pub struct Store { facts:BTreeMap<Key,Fact>, categories:BTreeMap<String,BTreeSet<Key>> }
impl Store {
 pub fn len(&self)->usize { self.facts.len() }
 pub fn is_empty(&self)->bool { self.facts.is_empty() }
 pub fn insert(&mut self,mut f:Fact)->io::Result<bool> {
  for s in [&f.subject,&f.relation,&f.object] {validate_field(s)?;}
  if f.sources.is_empty() || f.categories.is_empty() || f.sources.len()>MAX_ITEMS || f.categories.len()>MAX_ITEMS {return Err(invalid("missing or excessive metadata"));}
  for s in f.sources.iter().chain(f.categories.iter()) {validate_field(s)?;}
  f.categories=f.categories.iter().map(|s|normalize(s)).collect();
  let k=(normalize(&f.subject),normalize(&f.relation),normalize(&f.object));
  let new=!self.facts.contains_key(&k);
  if new && self.len()>=MAX_RECORDS {return Err(invalid("record limit"));}
  if let Some(old)=self.facts.get(&k) {
    if old.categories.union(&f.categories).count()>MAX_ITEMS || old.sources.union(&f.sources).count()>MAX_ITEMS {return Err(invalid("merged metadata limit"));}
  }
  for c in &f.categories {self.categories.entry(c.clone()).or_default().insert(k.clone());}
  self.facts.entry(k).and_modify(|old|{old.categories.extend(f.categories.iter().cloned());old.sources.extend(f.sources.iter().cloned());}).or_insert(f);
  Ok(new)
 }
 pub fn category(&self,c:&str,limit:usize)->Vec<&Fact> { self.categories.get(&normalize(c)).into_iter().flatten().take(limit).filter_map(|k|self.facts.get(k)).collect() }
 pub fn lookup(&self,s:&str,r:&str,limit:usize)->Vec<&Fact> {let s=normalize(s);let r=normalize(r);self.facts.range((s.clone(),r.clone(),String::new())..).take_while(|(k,_)| k.0==s && k.1==r).take(limit).map(|(_,f)|f).collect()}
 pub fn write(&self,mut w:impl Write)->io::Result<()> {
  let mut size=12usize;
  for f in self.facts.values() {size=size.checked_add(8+ [&f.subject,&f.relation,&f.object].into_iter().chain(f.categories.iter()).chain(f.sources.iter()).map(|s|4+s.len()).sum::<usize>()).ok_or_else(||invalid("size overflow"))?;if size>MAX_BYTES{return Err(invalid("store byte limit"));}}
  w.write_all(MAGIC)?;put_u32(&mut w,self.len())?;
  for f in self.facts.values(){for s in [&f.subject,&f.relation,&f.object]{put_str(&mut w,s)?;}for set in [&f.categories,&f.sources]{put_u32(&mut w,set.len())?;for s in set {put_str(&mut w,s)?;}}}Ok(())
 }
 pub fn read(r:impl Read)->io::Result<Self>{
  let mut r=r.take((MAX_BYTES+1) as u64);let mut magic=[0;8];r.read_exact(&mut magic)?;if &magic!=MAGIC{return Err(invalid("format version"));}
  let n=get_u32(&mut r)?;if n>MAX_RECORDS{return Err(invalid("record limit"));}
  let mut store=Self::default();for _ in 0..n {let f=Fact{subject:get_str(&mut r)?,relation:get_str(&mut r)?,object:get_str(&mut r)?,categories:get_set(&mut r)?,sources:get_set(&mut r)?};if !store.insert(f)?{return Err(invalid("duplicate record"));}}
  let mut extra=[0];if r.read(&mut extra)?!=0 || r.limit()==0{return Err(invalid("trailing data or byte limit"));}Ok(store)
 }
}
fn put_u32(w:&mut impl Write,n:usize)->io::Result<()> {w.write_all(&(n as u32).to_le_bytes())}
fn get_u32(r:&mut impl Read)->io::Result<usize>{let mut b=[0;4];r.read_exact(&mut b)?;Ok(u32::from_le_bytes(b) as usize)}
fn put_str(w:&mut impl Write,s:&str)->io::Result<()>{put_u32(w,s.len())?;w.write_all(s.as_bytes())}
fn get_str(r:&mut impl Read)->io::Result<String>{let n=get_u32(r)?;if n>MAX_FIELD{return Err(invalid("field limit"));}let mut b=vec![0;n];r.read_exact(&mut b)?;String::from_utf8(b).map_err(|_|invalid("UTF-8"))}
fn get_set(r:&mut impl Read)->io::Result<BTreeSet<String>>{let n=get_u32(r)?;if n>MAX_ITEMS{return Err(invalid("metadata limit"));}let mut set=BTreeSet::new();for _ in 0..n{if !set.insert(get_str(r)?){return Err(invalid("duplicate metadata"));}}Ok(set)}
/// TSV ingestion: subject, relation, object, comma-separated categories, source.
/// This is structured import, not automatic semantic extraction or verification.
pub fn parse_tsv(line:&str)->io::Result<Fact>{let p:Vec<_>=line.split('\t').collect();if p.len()!=5{return Err(invalid("expected five TSV columns"));}Ok(Fact{subject:p[0].into(),relation:p[1].into(),object:p[2].into(),categories:p[3].split(',').map(str::trim).map(str::to_owned).collect(),sources:[p[4].to_owned()].into()})}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn roundtrip_and_dedup(){let mut s=Store::default();assert!(s.insert(parse_tsv("France\tcapital\tParis\tgeography,Europe\tsource-a").unwrap()).unwrap());assert!(!s.insert(parse_tsv(" france \tCAPITAL\tPARIS\tpolitics\tsource-b").unwrap()).unwrap());assert_eq!(s.len(),1);assert_eq!(s.category("POLITICS",2).len(),1);assert_eq!(s.lookup("France","capital",1)[0].sources.len(),2);let mut b=vec![];s.write(&mut b).unwrap();let t=Store::read(&b[..]).unwrap();assert_eq!(s.lookup("France","capital",1),t.lookup("France","capital",1));}
 #[test] fn conflicts_preserved(){let mut s=Store::default();for x in ["Paris","Lyon"]{s.insert(parse_tsv(&format!("France\tcapital\t{x}\tgeo\tsrc")).unwrap()).unwrap();}assert_eq!(s.lookup("France","capital",10).len(),2);}
 #[test] fn rejects_corruption(){assert!(Store::read(&b"wrong"[..]).is_err());let mut b=vec![];Store::default().write(&mut b).unwrap();b.push(0);assert!(Store::read(&b[..]).is_err());assert!(Store::read(&b[..7]).is_err());}
 #[test] fn invalid_metadata(){let mut s=Store::default();assert!(s.insert(parse_tsv("a\tb\tc\t\tsrc").unwrap()).is_err());assert!(s.is_empty());}
 #[test] fn bounded_lookup(){let mut s=Store::default();s.insert(parse_tsv("a\tb\tc\td\te").unwrap()).unwrap();assert!(s.category("d",0).is_empty());assert!(s.lookup("a","b",0).is_empty());assert!(s.lookup("a","x",10).is_empty());}
}
