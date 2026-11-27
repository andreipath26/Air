//! Weightless binary associative baseline: deterministic hashed token sets, Jaccard ranking.
use crate::indexed::Result;
#[derive(Clone,Debug,PartialEq,Eq)]pub struct Signature([u64;16]);
impl Signature{
 pub fn from_text(s:&str)->Result<Self>{if s.len()>8192{return Err("signature input limit".into())}let mut bits=[0u64;16];for word in s.split(|c:char|!c.is_alphanumeric()).filter(|x|!x.is_empty()){let mut h=14695981039346656037u64;for b in word.to_lowercase().bytes(){h=(h^u64::from(b)).wrapping_mul(1099511628211);}let i=(h%1024)as usize;bits[i/64]|=1u64<<(i%64);}Ok(Self(bits))}
 pub fn similarity(&self,b:&Self)->f64{let(mut i,mut u)=(0,0);for(a,b)in self.0.iter().zip(b.0){i+=(a&b).count_ones();u+=(a|b).count_ones();}if u==0{0.0}else{f64::from(i)/f64::from(u)}}
}
pub struct Associative{items:Vec<(String,Signature)>}
impl Default for Associative{fn default()->Self{Self{items:vec![]}}}
impl Associative{pub fn insert(&mut self,label:&str,text:&str)->Result<()>{if self.items.len()>=4096||label.is_empty()||label.len()>256{return Err("associative capacity".into())}self.items.push((label.into(),Signature::from_text(text)?));Ok(())}
pub fn search(&self,text:&str,k:usize)->Result<Vec<(&str,f64)>>{let q=Signature::from_text(text)?;let mut v:Vec<_>=self.items.iter().map(|(l,s)|(l.as_str(),q.similarity(s))).filter(|(_,s)|*s>0.0).collect();v.sort_by(|a,b|b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));v.truncate(k.min(32));Ok(v)}}
#[test]fn retrieval(){let mut a=Associative::default();a.insert("auto","diesel engine combustion").unwrap();a.insert("math","integer equation").unwrap();assert_eq!(a.search("engine diesel",1).unwrap()[0].0,"auto");assert!(a.search("",1).unwrap().is_empty());}
