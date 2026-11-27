//! Content-addressed knowledge package intake; integrity is not authenticity.
use crate::{indexed::{Indexed,Result},policy::Policy,consolidation::{digest,merge_batch},curate::curate};
use serde::Deserialize;
use std::{fs::File,io::Read,path::Path};
#[derive(Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Manifest{pub format:u32,pub name:String,pub version:String,pub license:String,pub sha256:String}
pub fn import(db:&mut Indexed,p:&Policy,m:&Manifest,path:&Path)->Result<(usize,usize)>{
 if m.format!=1||[&m.name,&m.version,&m.license].iter().any(|s|s.trim().is_empty()||s.len()>128){return Err("invalid manifest".into())}
 let mut raw=vec![];File::open(path)?.take(1048577).read_to_end(&mut raw)?;if raw.len()>1048576||digest(&raw)!=m.sha256{return Err("package integrity or size failure".into())}
 let text=std::str::from_utf8(&raw)?;let mut facts=vec![];for line in text.lines(){if line.trim().is_empty()||line.starts_with('#'){continue}if facts.len()>=128{return Err("package record limit".into())}facts.push(curate(line)?);}
 let r=merge_batch(db,p,&facts)?;Ok((r.inserted,r.merged))
}
