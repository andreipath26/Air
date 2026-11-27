//! Conventional lexical baseline: multinomial naive Bayes with Laplace smoothing.
use crate::indexed::Result;
use std::collections::{BTreeMap,BTreeSet};
pub struct Lexical{labels:Vec<String>,documents:Vec<u64>,tokens:Vec<u64>,counts:Vec<BTreeMap<String,u64>>,vocabulary:BTreeSet<String>}
fn terms(text:&str)->Result<Vec<String>>{if text.len()>8192{return Err("text limit".into())}Ok(text.split(|c:char|!c.is_alphanumeric()).filter(|w|!w.is_empty()).map(str::to_lowercase).collect())}
impl Lexical{
 pub fn new(labels:Vec<String>)->Result<Self>{crate::weightless::Classifier::new(labels.clone())?;let n=labels.len();Ok(Self{labels,documents:vec![0;n],tokens:vec![0;n],counts:(0..n).map(|_|BTreeMap::new()).collect(),vocabulary:BTreeSet::new()})}
 pub fn train(&mut self,label:&str,text:&str)->Result<()>{let i=self.labels.iter().position(|x|x==label).ok_or("unknown label")?;let words=terms(text)?;if self.documents.iter().sum::<u64>()>=4096||self.vocabulary.len()+words.len()>65536{return Err("lexical capacity".into())}self.documents[i]+=1;for w in words{self.vocabulary.insert(w.clone());*self.counts[i].entry(w).or_default()+=1;self.tokens[i]+=1;}Ok(())}
 pub fn classify(&self,text:&str)->Result<Option<String>>{let words:Vec<_>=terms(text)?.into_iter().filter(|w|self.vocabulary.contains(w)).collect();if words.is_empty(){return Ok(None)}let mut scores=vec![];let vocab=self.vocabulary.len() as f64;for(i,l)in self.labels.iter().enumerate(){if self.documents[i]==0{continue}let mut score=(self.documents[i] as f64).ln();for w in &words{score+=((1+self.counts[i].get(w).copied().unwrap_or(0))as f64/(self.tokens[i]as f64+vocab)).ln();}scores.push((l,score));}scores.sort_by(|a,b|b.1.total_cmp(&a.1));if scores.is_empty()||scores.get(1).is_some_and(|b|(scores[0].1-b.1).abs()<1e-12){return Ok(None)}Ok(Some(scores[0].0.clone()))}
}
#[test]fn lexical_baseline(){let mut l=Lexical::new(vec!["math".into(),"engine".into()]).unwrap();l.train("math","integer multiplication").unwrap();l.train("engine","diesel injection").unwrap();assert_eq!(l.classify("diesel fuel").unwrap().as_deref(),Some("engine"));assert!(l.classify("unseen").unwrap().is_none());}
