//! Predictive SQLite page warming. Predictions cannot influence answer correctness.
use crate::{indexed::{Indexed,Result},prediction::Predictor};
use serde::Serialize;
#[derive(Default,Debug,Serialize)]pub struct Warmed{pub categories:usize,pub facts:usize,pub payload_bytes:usize}
pub fn warm(db:&Indexed,p:&Predictor,state:&str,max_categories:u32,max_bytes:usize)->Result<Warmed>{
 if max_categories>4||max_bytes>1024*1024||max_bytes==0{return Err("prefetch budget".into())}
 let mut r=Warmed::default();for(c,count)in p.predict(state,max_categories)?{if count<2||!db.has_category(&c)?{continue}let data=db.category(&c)?;if data.bytes>max_bytes.saturating_sub(r.payload_bytes){break}r.categories+=1;r.facts+=data.facts.len();r.payload_bytes+=data.bytes;}
 Ok(r)
}
#[test]fn bounded_prediction(){use crate::{indexed::Limits,store::parse_tsv};use std::path::Path;let mut db=Indexed::create(Path::new(":memory:"),Limits::default()).unwrap();db.insert(&parse_tsv("France\tcapital\tParis\tgeo\tfixture").unwrap()).unwrap();let p=Predictor::open(Path::new(":memory:")).unwrap();p.observe("auto","geo").unwrap();p.observe("auto","geo").unwrap();assert_eq!(warm(&db,&p,"auto",2,65536).unwrap().facts,1);assert!(warm(&db,&p,"auto",5,65536).is_err());}
