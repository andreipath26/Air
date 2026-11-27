//! Trainable RAM-node classifier baseline. Fixed binary features; no matrix multiplication.
use crate::indexed::Result;
use std::collections::BTreeMap;
pub struct Classifier{nodes:Vec<BTreeMap<(u8,String),u32>>,labels:Vec<String>}
impl Classifier{
 pub fn new(labels:Vec<String>)->Result<Self>{if labels.is_empty()||labels.len()>32||labels.iter().any(|s|s.is_empty()||s.len()>128)||labels.iter().collect::<std::collections::BTreeSet<_>>().len()!=labels.len(){return Err("labels".into())}Ok(Self{nodes:(0..16).map(|_|BTreeMap::new()).collect(),labels})}
 fn addresses(text:&str)->Result<[u8;16]>{if text.len()>8192{return Err("feature input".into())}let mut bits=[0u8;16];for word in text.split(|c:char|!c.is_alphanumeric()).filter(|w|!w.is_empty()){let mut h=14695981039346656037u64;for b in word.to_lowercase().bytes(){h=(h^u64::from(b)).wrapping_mul(1099511628211);}let n=(h%128)as usize;bits[n/8]|=1<<(n%8);}Ok(bits)}
 pub fn train(&mut self,label:&str,text:&str)->Result<()>{if !self.labels.iter().any(|s|s==label){return Err("unknown label".into())}for(n,a)in self.nodes.iter_mut().zip(Self::addresses(text)?){if a!=0{let v=n.entry((a,label.into())).or_default();*v=v.saturating_add(1);}}Ok(())}
 pub fn classify(&self,text:&str)->Result<Option<(String,u32)>>{let addresses=Self::addresses(text)?;let mut scores:Vec<_>=self.labels.iter().map(|l|{let s=self.nodes.iter().zip(addresses).filter(|(_,a)|*a!=0).filter(|(n,a)|n.contains_key(&(*a,l.clone()))).count()as u32;(l.clone(),s)}).collect();scores.sort_by(|a,b|b.1.cmp(&a.1).then(a.0.cmp(&b.0)));if scores[0].1==0||scores.get(1).is_some_and(|x|x.1==scores[0].1){Ok(None)}else{Ok(scores.into_iter().next())}}
}
#[test]fn learning_and_abstention(){let mut c=Classifier::new(vec!["automotive".into(),"math".into()]).unwrap();c.train("automotive","diesel engine").unwrap();c.train("math","integer arithmetic").unwrap();assert_eq!(c.classify("diesel engine").unwrap().unwrap().0,"automotive");assert!(c.classify("").unwrap().is_none());}
