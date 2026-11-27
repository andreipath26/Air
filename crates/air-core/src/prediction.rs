//! Bounded predictive memory based on observed state transitions.
use crate::indexed::Result;
use rusqlite::{Connection,params};
use std::path::Path;
pub struct Predictor{db:Connection}
impl Predictor{
 pub fn open(path:&Path)->Result<Self>{let db=Connection::open(path)?;db.busy_timeout(std::time::Duration::from_secs(2))?;db.pragma_update(None,"cache_size",-1024)?;db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,1048576);db.execute_batch("CREATE TABLE IF NOT EXISTS transitions(state TEXT NOT NULL,next TEXT NOT NULL,n INTEGER NOT NULL,PRIMARY KEY(state,next));CREATE INDEX IF NOT EXISTS transition_rank ON transitions(state,n DESC,next);")?;Ok(Self{db})}
 pub fn observe(&self,state:&str,next:&str)->Result<()>{if [state,next].iter().any(|x|x.is_empty()||x.len()>256){return Err("state limit".into())}self.db.execute("INSERT INTO transitions VALUES (?1,?2,1) ON CONFLICT(state,next) DO UPDATE SET n=min(n+1,1000000000)",params![state,next])?;Ok(())}
 pub fn predict(&self,state:&str,k:u32)->Result<Vec<(String,i64)>>{if k>32||state.len()>256{return Err("prediction limit".into())}let mut s=self.db.prepare("SELECT next,n FROM transitions WHERE state=?1 ORDER BY n DESC,next LIMIT ?2")?;let v=s.query_map(params![state,k],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;Ok(v)}
}
#[test]fn learns(){let p=Predictor::open(Path::new(":memory:")).unwrap();p.observe("a","b").unwrap();p.observe("a","c").unwrap();p.observe("a","c").unwrap();assert_eq!(p.predict("a",1).unwrap()[0],("c".into(),2));}
