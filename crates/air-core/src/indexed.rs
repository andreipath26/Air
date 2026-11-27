//! SQLite-backed selective retrieval. Page cache and query work bounded; not a process RSS guarantee.
use crate::store::Fact;
use rusqlite::{params,Connection,OpenFlags,OptionalExtension,TransactionBehavior};
use std::{collections::BTreeSet,path::Path,time::Duration};
pub type Result<T>=std::result::Result<T,Box<dyn std::error::Error>>;
#[derive(Clone,Copy,Debug)]
pub struct Limits {pub cache_kib:u32,pub rows:u32,pub bytes:usize,pub max_steps:u32}
impl Default for Limits {fn default()->Self{Self{cache_kib:2048,rows:32,bytes:256*1024,max_steps:100000}}}
fn norm(s:&str)->String{s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()}
const SCHEMA:&str="PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS facts(id INTEGER PRIMARY KEY, s TEXT NOT NULL,r TEXT NOT NULL,o TEXT NOT NULL,ns TEXT NOT NULL,nr TEXT NOT NULL,no TEXT NOT NULL, UNIQUE(ns,nr,no)); CREATE TABLE IF NOT EXISTS categories(category TEXT NOT NULL, fact INTEGER NOT NULL REFERENCES facts(id),PRIMARY KEY(category,fact)) WITHOUT ROWID; CREATE TABLE IF NOT EXISTS sources(fact INTEGER NOT NULL REFERENCES facts(id),source TEXT NOT NULL,PRIMARY KEY(fact,source)) WITHOUT ROWID; CREATE INDEX IF NOT EXISTS category_by_fact ON categories(fact,category); CREATE INDEX IF NOT EXISTS fact_lookup ON facts(ns,nr,id); PRAGMA user_version=1;";
pub struct Indexed {conn:Connection,limits:Limits,remaining:std::sync::Arc<std::sync::atomic::AtomicU32>}
#[derive(Debug)]
pub struct Retrieved {pub facts:Vec<Fact>,pub bytes:usize,pub truncated:bool}
fn check(l:Limits)->Result<()> {if !(64..=65536).contains(&l.cache_kib)||!(1..=1000).contains(&l.rows)||!(1..=16*1024*1024).contains(&l.bytes)||!(100..=10000000).contains(&l.max_steps){return Err("invalid query limits".into())}Ok(())}
impl Indexed {
 pub fn create(path:&Path,limits:Limits)->Result<Self>{check(limits)?;let conn=Connection::open(path)?;let version:i64=conn.query_row("PRAGMA user_version",[],|r|r.get(0))?;if version!=0&&version!=1{return Err("unsupported schema".into())}conn.execute_batch(SCHEMA)?;Self::configure(conn,limits)}
 pub fn open(path:&Path,limits:Limits)->Result<Self>{check(limits)?;let conn=Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY)?;let version:i64=conn.query_row("PRAGMA user_version",[],|r|r.get(0))?;if version!=1{return Err("unsupported schema".into())}Self::configure(conn,limits)}
 fn configure(conn:Connection,limits:Limits)->Result<Self>{conn.busy_timeout(Duration::from_secs(2))?;conn.pragma_update(None,"cache_size",-(limits.cache_kib as i64))?;conn.pragma_update(None,"temp_store","FILE")?;conn.pragma_update(None,"foreign_keys",true)?;conn.pragma_update(None,"mmap_size",0)?;conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,1048576); let remaining=std::sync::Arc::new(std::sync::atomic::AtomicU32::new(limits.max_steps)); let budget=remaining.clone(); conn.progress_handler(100,Some(move||budget.fetch_update(std::sync::atomic::Ordering::Relaxed,std::sync::atomic::Ordering::Relaxed,|n|n.checked_sub(100)).is_err()));Ok(Self{conn,limits,remaining})}
 pub fn insert(&mut self,f:&Fact)->Result<bool>{Ok(self.insert_batch(std::slice::from_ref(f))?==1)}
 pub fn insert_batch(&mut self,facts:&[Fact])->Result<usize>{if facts.len()>128{return Err("batch limit".into())}let mut inserted=0;let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;for f in facts{self.remaining.store(self.limits.max_steps,std::sync::atomic::Ordering::Relaxed);for s in [&f.subject,&f.relation,&f.object].into_iter().chain(f.categories.iter()).chain(f.sources.iter()){if s.trim().is_empty()||s.len()>4096{return Err("invalid field".into())}}if f.categories.is_empty()||f.sources.is_empty()||f.categories.len()>64||f.sources.len()>64{return Err("invalid metadata".into())}
 let (ns,nr,no)=(norm(&f.subject),norm(&f.relation),norm(&f.object));
 let n=tx.execute("INSERT OR IGNORE INTO facts(s,r,o,ns,nr,no) VALUES (?1,?2,?3,?4,?5,?6)",params![f.subject,f.relation,f.object,ns,nr,no])?;
 let id:i64=tx.query_row("SELECT id FROM facts WHERE ns=?1 AND nr=?2 AND no=?3",params![ns,nr,no],|r|r.get(0))?;
 for c in &f.categories{tx.execute("INSERT OR IGNORE INTO categories VALUES (?1,?2)",params![norm(c),id])?;}
 for s in &f.sources{tx.execute("INSERT OR IGNORE INTO sources VALUES (?1,?2)",params![id,s])?;}
 let cats:i64=tx.query_row("SELECT count(*) FROM categories WHERE fact=?1",[id],|r|r.get(0))?;
 let srcs:i64=tx.query_row("SELECT count(*) FROM sources WHERE fact=?1",[id],|r|r.get(0))?;
 if cats>64||srcs>64{return Err("merged metadata limit".into())}inserted+=n;}tx.commit()?;Ok(inserted)}
 pub fn len(&self)->Result<u64>{self.remaining.store(self.limits.max_steps,std::sync::atomic::Ordering::Relaxed);Ok(self.conn.query_row("SELECT count(*) FROM facts",[],|r|r.get(0))?)}
 pub fn lookup(&self,s:&str,r:&str)->Result<Retrieved>{self.select("SELECT id,s,r,o FROM facts WHERE ns=?1 AND nr=?2 ORDER BY id LIMIT ?3",&norm(s),&norm(r))}
 pub fn category(&self,c:&str)->Result<Retrieved>{self.select("SELECT f.id,f.s,f.r,f.o FROM categories c JOIN facts f ON c.fact=f.id WHERE c.category=?1 AND ?2='' ORDER BY c.fact LIMIT ?3",&norm(c),"")}
 fn select(&self,sql:&str,a:&str,b:&str)->Result<Retrieved>{
 self.remaining.store(self.limits.max_steps,std::sync::atomic::Ordering::Relaxed);let mut stmt=self.conn.prepare(sql)?;let mut rows=stmt.query(params![a,b,self.limits.rows+1])?;
 let mut out=Retrieved{facts:vec![],bytes:0,truncated:false};let mut steps=0u32;
 while let Some(row)=rows.next()?{if out.facts.len()>=self.limits.rows as usize{out.truncated=true;break;}
 let id:i64=row.get(0)?;let mut f=Fact{subject:row.get(1)?,relation:row.get(2)?,object:row.get(3)?,categories:BTreeSet::new(),sources:BTreeSet::new()};
 let mut bytes=f.subject.len()+f.relation.len()+f.object.len();steps+=1;
 for (sql,set) in [("SELECT category FROM categories WHERE fact=?1 LIMIT 65",&mut f.categories),("SELECT source FROM sources WHERE fact=?1 LIMIT 65",&mut f.sources)]{let mut q=self.conn.prepare(sql)?;let mut rs=q.query([id])?;while let Some(x)=rs.next()?{let v:String=x.get(0)?;steps+=1;bytes=bytes.saturating_add(v.len());if set.len()>=64||v.len()>4096||steps>self.limits.max_steps||out.bytes.saturating_add(bytes)>self.limits.bytes{out.truncated=true;return Ok(out)}set.insert(v);}}
 if steps>self.limits.max_steps||out.bytes.saturating_add(bytes)>self.limits.bytes{out.truncated=true;break;}out.bytes+=bytes;out.facts.push(f);
 }Ok(out)}
 pub fn has_category(&self,c:&str)->Result<bool>{self.remaining.store(self.limits.max_steps,std::sync::atomic::Ordering::Relaxed);Ok(self.conn.query_row("SELECT 1 FROM categories WHERE category=?1 LIMIT 1",[norm(c)],|r|r.get::<_,i64>(0)).optional()?.is_some())}
}
#[cfg(test)]mod tests{use super::*;use crate::store::parse_tsv;
#[test]fn selective_and_bounded(){let mut db=Indexed::create(Path::new(":memory:"),Limits{rows:1,..Limits::default()}).unwrap();for o in ["Paris","Lyon"]{db.insert(&parse_tsv(&format!("France\tcapital\t{o}\tgeo,politics\tsrc")).unwrap()).unwrap();}assert!(!db.insert(&parse_tsv("france\tcapital\tparis\tEurope\tsrc2").unwrap()).unwrap());assert_eq!(db.len().unwrap(),2);let r=db.lookup("FRANCE","capital").unwrap();assert_eq!(r.facts.len(),1);assert!(r.truncated);assert_eq!(r.facts[0].sources.len(),2);assert!(db.has_category("europe").unwrap());assert!(db.category("unknown").unwrap().facts.is_empty());}
#[test]fn byte_budget(){let mut db=Indexed::create(Path::new(":memory:"),Limits{bytes:1,..Limits::default()}).unwrap();db.insert(&parse_tsv("a\tb\tc\td\te").unwrap()).unwrap();assert!(db.lookup("a","b").unwrap().truncated);}}
