//! Operator-trained persistent routing examples. Approximate categories are suggestions, not answers.
use crate::{indexed::Result,weightless::Classifier,router::{Route,SemanticRouter}};
use rusqlite::{Connection,params};
use std::path::Path;
pub struct Training{db:Connection}
impl Training{
 pub fn open(path:&Path)->Result<Self>{let db=Connection::open(path)?;db.busy_timeout(std::time::Duration::from_secs(2))?;db.execute_batch("CREATE TABLE IF NOT EXISTS route_examples(category TEXT NOT NULL,phrase TEXT NOT NULL,PRIMARY KEY(category,phrase));")?;Ok(Self{db})}
 pub fn teach(&self,category:&str,phrase:&str)->Result<()>{
 if category.trim().is_empty()||category.len()>128||phrase.trim().is_empty()||phrase.len()>1024{return Err("training bounds".into())}
 let n:i64=self.db.query_row("SELECT count(*) FROM route_examples",[],|r|r.get(0))?;if n>=4096{return Err("training capacity".into())}
 self.db.execute("INSERT OR IGNORE INTO route_examples VALUES (?1,?2)",params![category,phrase])?;Ok(())
 }
 pub fn classifier(&self)->Result<Option<Classifier>>{
 let mut q=self.db.prepare("SELECT DISTINCT category FROM route_examples ORDER BY category LIMIT 33")?;let labels=q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
 if labels.is_empty(){return Ok(None)}let mut c=Classifier::new(labels)?;
 let mut q=self.db.prepare("SELECT category,phrase FROM route_examples ORDER BY category,phrase LIMIT 4097")?;let mut count=0;
 for row in q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?{let (label,phrase)=row?;count+=1;if count>4096{return Err("training capacity".into())}c.train(&label,&phrase)?;}Ok(Some(c))
 }
}
pub struct Learned{pub classifier:Option<Classifier>}
impl SemanticRouter for Learned{fn route(&self,q:&str)->Result<Route>{match &self.classifier{Some(c)=>match c.classify(q)?{Some((category,score)) if score>=2=>Ok(Route::Category{category}),_=>Ok(Route::Unknown)},None=>Ok(Route::Unknown)}}}
#[test]fn persistent_training(){let t=Training::open(Path::new(":memory:")).unwrap();t.teach("automotive","diesel engine smoke combustion injection").unwrap();t.teach("math","integer arithmetic equation algebra").unwrap();let l=Learned{classifier:t.classifier().unwrap()};assert_eq!(l.route("").unwrap(),Route::Unknown);assert_eq!(l.route("diesel engine smoke combustion injection").unwrap(),Route::Category{category:"automotive".into()});}
