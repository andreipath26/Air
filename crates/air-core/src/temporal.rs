//! Versioned time intervals for explicitly reviewed temporal claims.
use crate::indexed::Result;
use rusqlite::{Connection,params};
use std::path::Path;
#[derive(Debug,Clone,PartialEq,Eq)]pub struct Event{pub id:i64,pub subject:String,pub relation:String,pub object:String,pub from:i64,pub until:Option<i64>,pub source:String}
pub struct Timeline{db:Connection}
impl Timeline{
 pub fn open(path:&Path)->Result<Self>{let db=Connection::open(path)?;db.busy_timeout(std::time::Duration::from_secs(2))?;db.execute_batch("CREATE TABLE IF NOT EXISTS temporal_claims(id INTEGER PRIMARY KEY,s TEXT NOT NULL,r TEXT NOT NULL,o TEXT NOT NULL,start INTEGER NOT NULL,stop INTEGER,source TEXT NOT NULL);CREATE INDEX IF NOT EXISTS temporal_lookup ON temporal_claims(s,r,start);")?;Ok(Self{db})}
 pub fn record(&self,s:&str,r:&str,o:&str,from:i64,until:Option<i64>,source:&str)->Result<i64>{if [s,r,o,source].iter().any(|x|x.trim().is_empty()||x.len()>1024)||until.is_some_and(|u|u<=from){return Err("invalid temporal claim".into())}self.db.execute("INSERT INTO temporal_claims(s,r,o,start,stop,source) VALUES (?1,?2,?3,?4,?5,?6)",params![s,r,o,from,until,source])?;Ok(self.db.last_insert_rowid())}
 pub fn at(&self,s:&str,r:&str,when:i64)->Result<(Vec<Event>,bool)>{let mut q=self.db.prepare("SELECT id,s,r,o,start,stop,source FROM temporal_claims WHERE s=?1 AND r=?2 AND start<=?3 AND (stop IS NULL OR stop>?3) ORDER BY start DESC,id DESC LIMIT 33")?;let mut v=q.query_map(params![s,r,when],|x|Ok(Event{id:x.get(0)?,subject:x.get(1)?,relation:x.get(2)?,object:x.get(3)?,from:x.get(4)?,until:x.get(5)?,source:x.get(6)?}))?.collect::<rusqlite::Result<Vec<_>>>()?;let truncated=v.len()>32;v.truncate(32);Ok((v,truncated))}
}
#[test]fn boundaries(){let t=Timeline::open(Path::new(":memory:")).unwrap();t.record("a","state","old",0,Some(10),"s").unwrap();t.record("a","state","new",10,None,"s").unwrap();assert_eq!(t.at("a","state",9).unwrap().0[0].object,"old");assert_eq!(t.at("a","state",10).unwrap().0[0].object,"new");}
