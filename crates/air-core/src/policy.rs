//! Operator-reviewed aliases and claim assessments, kept separate from untrusted knowledge.
use crate::{indexed::Result,store::Fact};
use rusqlite::{params,Connection,OptionalExtension};
use serde::{Serialize,Deserialize};
use std::{path::Path,time::Duration};
fn norm(s:&str)->String{s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()}
fn field(s:&str,n:usize)->Result<()> {if s.trim().is_empty()||s.len()>n||s.chars().any(char::is_control){return Err("invalid policy field".into())}Ok(())}
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]#[serde(rename_all="snake_case")]pub enum Verdict{Accepted,Rejected,Unreviewed}
#[derive(Debug,Clone,Serialize,Deserialize)]pub struct Review{pub verdict:Verdict,pub reviewer:String,pub rationale:String}
pub struct Policy{db:Connection}
impl Policy{
 pub fn open(path:&Path)->Result<Self>{let db=Connection::open(path)?;db.busy_timeout(Duration::from_secs(2))?;db.pragma_update(None,"cache_size",-1024)?;db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,1048576);db.execute_batch("CREATE TABLE IF NOT EXISTS aliases(kind TEXT NOT NULL,alias TEXT NOT NULL,canonical TEXT NOT NULL,PRIMARY KEY(kind,alias));CREATE TABLE IF NOT EXISTS reviews(id INTEGER PRIMARY KEY,s TEXT NOT NULL,r TEXT NOT NULL,o TEXT NOT NULL,body TEXT NOT NULL);CREATE INDEX IF NOT EXISTS review_lookup ON reviews(s,r,o,id DESC);CREATE TABLE IF NOT EXISTS approved_skills(name TEXT NOT NULL,version INTEGER NOT NULL,PRIMARY KEY(name,version));")?;Ok(Self{db})}
 pub fn alias(&self,kind:&str,alias:&str,canonical:&str)->Result<()>{if !["subject","relation","category","object"].contains(&kind){return Err("alias kind".into())}field(alias,256)?;field(canonical,256)?;let a=norm(alias);let c=norm(canonical);if a==c{return Err("self alias".into())}self.db.execute("INSERT INTO aliases VALUES (?1,?2,?3) ON CONFLICT(kind,alias) DO UPDATE SET canonical=excluded.canonical",params![kind,a,c])?;Ok(())}
 /// Single-hop mapping: no recursive aliases or silent cycle traversal.
 pub fn canonical(&self,kind:&str,value:&str)->Result<String>{field(value,256)?;Ok(self.db.query_row("SELECT canonical FROM aliases WHERE kind=?1 AND alias=?2",params![kind,norm(value)],|r|r.get(0)).optional()?.unwrap_or_else(||value.to_owned()))}
 pub fn review(&self,s:&str,r:&str,o:&str,review:&Review)->Result<i64>{for x in [s,r,o]{field(x,4096)?}field(&review.reviewer,256)?;field(&review.rationale,4096)?;self.db.execute("INSERT INTO reviews(s,r,o,body) VALUES (?1,?2,?3,?4)",params![norm(s),norm(r),norm(o),serde_json::to_string(review)?])?;Ok(self.db.last_insert_rowid())}
 pub fn assessment(&self,f:&Fact)->Result<Option<Review>>{let raw:Option<String>=self.db.query_row("SELECT body FROM reviews WHERE s=?1 AND r=?2 AND o=?3 ORDER BY id DESC LIMIT 1",params![norm(&f.subject),norm(&f.relation),norm(&f.object)],|r|r.get(0)).optional()?;raw.map(|s|serde_json::from_str(&s).map_err(Into::into)).transpose()}
 pub fn approve_skill(&self,name:&str,version:i64)->Result<()>{field(name,128)?;if version<1{return Err("skill version".into())}self.db.execute("INSERT OR IGNORE INTO approved_skills VALUES (?1,?2)",params![name,version])?;Ok(())}
 pub fn skill_approved(&self,name:&str,version:i64)->Result<bool>{Ok(self.db.query_row("SELECT 1 FROM approved_skills WHERE name=?1 AND version=?2",params![name,version],|r|r.get::<_,i64>(0)).optional()?.is_some())}
}
#[test]fn aliases_and_revisions(){let p=Policy::open(Path::new(":memory:")).unwrap();p.alias("relation","capital city","capital").unwrap();assert_eq!(p.canonical("relation","Capital City").unwrap(),"capital");let f=crate::store::parse_tsv("France\tcapital\tParis\tgeo\tsrc").unwrap();assert!(p.assessment(&f).unwrap().is_none());for verdict in [Verdict::Accepted,Verdict::Rejected]{p.review("France","capital","Paris",&Review{verdict,reviewer:"operator".into(),rationale:"fixture".into()}).unwrap();}assert_eq!(p.assessment(&f).unwrap().unwrap().verdict,Verdict::Rejected);}
