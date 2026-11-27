//! Safe bounded SafeTensors metadata inspection and streaming checkpoint fingerprint.
use crate::indexed::Result;
use serde::Serialize;
use sha2::{Sha256,Digest};
use std::{io::{Read,Seek,SeekFrom},fs::File,path::Path};
#[derive(Debug,Serialize)]pub struct Tensor{pub name:String,pub dtype:String,pub shape:Vec<u64>,pub offset:[u64;2]}
#[derive(Debug,Serialize)]pub struct Inventory{pub bytes:u64,pub sha256:String,pub tensors:Vec<Tensor>}
pub fn inspect(path:&Path)->Result<Inventory>{
 let mut f=File::open(path)?;let bytes=f.metadata()?.len();let mut n=[0;8];f.read_exact(&mut n)?;let n=u64::from_le_bytes(n);
 if n==0||n>16*1024*1024||n>bytes.saturating_sub(8){return Err("invalid header size".into())}
 let mut header=vec![0;n as usize];f.read_exact(&mut header)?;let v:serde_json::Value=serde_json::from_slice(&header)?;let map=v.as_object().ok_or("invalid header")?;
 if map.len()>100000{return Err("tensor count limit".into())}let mut tensors=vec![];
 for(name,t)in map{if name=="__metadata__"{continue}let dtype=t["dtype"].as_str().ok_or("dtype")?.to_owned();let shape:Vec<u64>=serde_json::from_value(t["shape"].clone())?;let offset:[u64;2]=serde_json::from_value(t["data_offsets"].clone())?;
 let width=match dtype.as_str(){"BOOL"|"U8"|"I8"|"F8_E4M3"|"F8_E5M2"=>1,"I16"|"U16"|"F16"|"BF16"=>2,"I32"|"U32"|"F32"=>4,"I64"|"U64"|"F64"=>8,_=>return Err("unsupported dtype".into())};
 let count=shape.iter().try_fold(1u64,|a,b|a.checked_mul(*b)).ok_or("shape overflow")?;if name.len()>1024||shape.len()>16||offset[1]<offset[0]||offset[1]>bytes-8-n||count.checked_mul(width)!=Some(offset[1]-offset[0]){return Err("invalid tensor extent".into())}tensors.push(Tensor{name:name.clone(),dtype,shape,offset});}
 tensors.sort_by_key(|t|(t.offset[0],t.offset[1]));let mut end=0;for t in &tensors{if t.offset[0]!=end{return Err("gap or overlap".into())}end=t.offset[1];}if end!=bytes-8-n{return Err("unaccounted tensor data".into())}
 f.seek(SeekFrom::Start(0))?;let mut h=Sha256::new();let mut b=[0;65536];loop{let n=f.read(&mut b)?;if n==0{break}h.update(&b[..n]);}Ok(Inventory{bytes,sha256:format!("{:x}",h.finalize()),tensors})
}

#[derive(Debug,Serialize)]pub struct Statistics{pub name:String,pub dtype:String,pub elements:u64,pub finite:u64,pub zeros:u64,pub nonfinite:u64,pub min:Option<f64>,pub max:Option<f64>,pub mean:Option<f64>,pub rms:Option<f64>}
pub fn statistics(path:&Path,inventory:&Inventory)->Result<Vec<Statistics>>{
 let mut f=File::open(path)?;if f.metadata()?.len()!=inventory.bytes{return Err("checkpoint size changed".into())}let mut n=[0;8];f.read_exact(&mut n)?;let base=8+u64::from_le_bytes(n);let mut out=vec![];
 for t in &inventory.tensors{let width=match t.dtype.as_str(){"F32"=>4,"BF16"|"F16"=>2,_=>continue};let elements=(t.offset[1]-t.offset[0])/width;f.seek(SeekFrom::Start(base+t.offset[0]))?;let(mut left,mut finite,mut zeros,mut nonfinite)=(elements,0u64,0u64,0u64);let(mut min,mut max,mut sum,mut sq)=(f64::INFINITY,f64::NEG_INFINITY,0.0,0.0);let mut buf=[0u8;65536];
 while left>0{let count=left.min((buf.len() as u64)/width);let len=(count*width)as usize;f.read_exact(&mut buf[..len])?;for b in buf[..len].chunks_exact(width as usize){let x=match t.dtype.as_str(){"F32"=>f32::from_le_bytes(b.try_into().unwrap())as f64,"BF16"=>f32::from_bits((u16::from_le_bytes(b.try_into().unwrap())as u32)<<16)as f64,_=>{let v=u16::from_le_bytes(b.try_into().unwrap());let sign=if v&0x8000==0{1.0}else{-1.0};let e=(v>>10)&31;let m=v&1023;if e==31{if m==0{sign*f64::INFINITY}else{f64::NAN}}else if e==0{sign*(m as f64)*2f64.powi(-24)}else{sign*(1.0+m as f64/1024.0)*2f64.powi(e as i32-15)}}};if x.is_finite(){finite+=1;if x==0.0{zeros+=1;}min=min.min(x);max=max.max(x);sum+=x;sq+=x*x;}else{nonfinite+=1;}}left-=count;}
 out.push(Statistics{name:t.name.clone(),dtype:t.dtype.clone(),elements,finite,zeros,nonfinite,min:(finite>0).then_some(min),max:(finite>0).then_some(max),mean:(finite>0).then(||sum/finite as f64),rms:(finite>0).then(||(sq/finite as f64).sqrt())});}
 Ok(out)
}
