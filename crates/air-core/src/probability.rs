//! Explicit finite Bayesian update under caller-supplied conditional-independence assumptions.
use crate::indexed::Result;
use serde::{Deserialize,Serialize};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Hypothesis{pub name:String,pub prior:f64,pub likelihoods:Vec<f64>}
#[derive(Debug,Serialize)]pub struct Posterior{pub name:String,pub probability:f64}
pub fn infer(h:&[Hypothesis])->Result<Vec<Posterior>>{
 if h.is_empty()||h.len()>32{return Err("hypothesis count".into())}
 let count=h[0].likelihoods.len();if count>64{return Err("evidence count".into())}
 let valid=|x:f64|x.is_finite()&&(0.0..=1.0).contains(&x);
 let mut names=std::collections::BTreeSet::new();
 if h.iter().any(|x|x.name.trim().is_empty()||x.name.len()>128||!names.insert(&x.name)||!valid(x.prior)||x.likelihoods.len()!=count||x.likelihoods.iter().any(|p|!valid(*p))){return Err("invalid probability model".into())}
 let sum=h.iter().map(|x|x.prior).sum::<f64>();if (sum-1.0).abs()>1e-9{return Err("priors must sum to one".into())}
 let logs:Vec<f64>=h.iter().map(|x|x.prior.ln()+x.likelihoods.iter().map(|p|p.ln()).sum::<f64>()).collect();
 let max=logs.iter().copied().fold(f64::NEG_INFINITY,f64::max);if !max.is_finite(){return Err("zero evidence probability".into())}
 let weights:Vec<_>=logs.iter().map(|x|(x-max).exp()).collect();let z=weights.iter().sum::<f64>();
 let mut out:Vec<_>=h.iter().zip(weights).map(|(x,w)|Posterior{name:x.name.clone(),probability:w/z}).collect();out.sort_by(|a,b|b.probability.total_cmp(&a.probability));Ok(out)
}
#[test]fn bayes(){let x=[Hypothesis{name:"a".into(),prior:0.2,likelihoods:vec![0.8]},Hypothesis{name:"b".into(),prior:0.8,likelihoods:vec![0.1]}];let r=infer(&x).unwrap();assert_eq!(r[0].name,"a");assert!((r[0].probability-2.0/3.0).abs()<1e-9);}
