use air_core::store::{Store,parse_tsv,Fact};
use std::{fs::{File,OpenOptions},io::{self,BufRead,BufReader,BufWriter,Write},path::Path};
fn show(f:&Fact){println!("{} | {} | {} | sources: {:?}",f.subject,f.relation,f.object,f.sources);}
fn run()->Result<(),Box<dyn std::error::Error>>{
 let a:Vec<String>=std::env::args().collect();match a.get(1).map(String::as_str){
 Some("doctor") if a.len()==2=>println!("AIR structured knowledge prototype; arch={} os={}; neural inference not implemented",std::env::consts::ARCH,std::env::consts::OS),
 Some("import") if a.len()==4=>{let mut s=Store::default();let mut r=BufReader::new(File::open(&a[2])?);let mut line=String::new();let mut n=0;loop{line.clear();let read=std::io::Read::take(&mut r,262145).read_line(&mut line)?;if read==0{break;}if read>262144{return Err("line too large".into());}n+=1;let l=line.trim_end_matches(['\r','\n']);if l.is_empty()||l.starts_with('#'){continue;}s.insert(parse_tsv(l).map_err(|e|format!("line {n}: {e}"))?)?;}
 let path=Path::new(&a[3]);let mut w=BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);if let Err(e)=s.write(&mut w).and_then(|_|w.flush()){drop(w);let _=std::fs::remove_file(path);return Err(e.into());}println!("Imported {} unique records; source claims are not verified",s.len());}
 Some("lookup") if a.len()==5=>{let s=Store::read(BufReader::new(File::open(&a[2])?))?;let f=s.lookup(&a[3],&a[4],100);if f.is_empty(){println!("No matching claim");}else{if f.len()>1{println!("Multiple claims; not resolved");}for x in f{show(x);}}}
 Some("category") if a.len()==4=>{let s=Store::read(BufReader::new(File::open(&a[2])?))?;for f in s.category(&a[3],100){show(f);}}
 _=>return Err(io::Error::new(io::ErrorKind::InvalidInput,"Usage: air-cli doctor | import INPUT.tsv NEW.air | lookup STORE SUBJECT RELATION | category STORE CATEGORY").into())
 }Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("{e}");std::process::exit(2);}}
