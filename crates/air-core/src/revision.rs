//! Append-only, content-addressed knowledge revisions with explicit rollback pointers.
use crate::{indexed::Result,consolidation::digest};
use rusqlite::{Connection,params,OptionalExtension,TransactionBehavior};
use std::path::Path;
pub struct Revisions{db:Connection}
impl Revisions{
 pub fn open(path:&Path)->Result<Self>{let db=Connection::open(path)?;db.busy_timeout(std::time::Duration::from_secs(2))?;db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,1048576);db.execute_batch("CREATE TABLE IF NOT EXISTS revision_objects(hash TEXT PRIMARY KEY,body BLOB NOT NULL);CREATE TABLE IF NOT EXISTS revision_log(id INTEGER PRIMARY KEY,name TEXT NOT NULL,hash TEXT NOT NULL);CREATE INDEX IF NOT EXISTS revision_by_name ON revision_log(name,id DESC);")?;Ok(Self{db})}
 pub fn commit(&mut self,name:&str,body:&[u8])->Result<i64>{if name.is_empty()||name.len()>256||body.len()>262144{return Err("revision bounds".into())}let hash=digest(body);let tx=self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;tx.execute("INSERT OR IGNORE INTO revision_objects VALUES (?1,?2)",params![hash,body])?;tx.execute("INSERT INTO revision_log(name,hash) VALUES (?1,?2)",params![name,hash])?;let id=tx.last_insert_rowid();tx.commit()?;Ok(id)}
 pub fn read(&self,name:&str,version:Option<i64>)->Result<Option<Vec<u8>>>{let raw:Option<(String,Vec<u8>)>=match version{Some(id)=>self.db.query_row("SELECT o.hash,o.body FROM revision_log l JOIN revision_objects o ON o.hash=l.hash WHERE l.name=?1 AND l.id=?2",params![name,id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?,None=>self.db.query_row("SELECT o.hash,o.body FROM revision_log l JOIN revision_objects o ON o.hash=l.hash WHERE l.name=?1 ORDER BY l.id DESC LIMIT 1",[name],|r|Ok((r.get(0)?,r.get(1)?))).optional()?};match raw{Some((hash,b)) if b.len()<=262144&&digest(&b)==hash=>Ok(Some(b)),Some(_)=>Err("revision integrity failure".into()),None=>Ok(None)}}
 pub fn rollback(&mut self,name:&str,version:i64)->Result<i64>{let bytes=self.read(name,Some(version))?.ok_or("unknown revision")?;self.commit(name,&bytes)}
}
#[test]fn rollback_preserves_history(){let mut r=Revisions::open(Path::new(":memory:")).unwrap();let a=r.commit("x",b"old").unwrap();let b=r.commit("x",b"new").unwrap();assert!(r.rollback("x",a).unwrap()>b);assert_eq!(r.read("x",None).unwrap().unwrap(),b"old");assert_eq!(r.read("x",Some(b)).unwrap().unwrap(),b"new");}
