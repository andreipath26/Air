use std::{hint::black_box, time::Instant};
fn main() { if std::env::args().nth(1).as_deref()==Some("retrieval") { if let Err(e)=retrieval(){eprintln!("{e}");std::process::exit(2);} return; } let n: u64 = match std::env::args().nth(1).unwrap_or_else(||"100000".into()).parse() { Ok(n) if n > 0 => n, _ => { eprintln!("Expected positive iteration count"); std::process::exit(2); } }; let start=Instant::now(); let mut x=0u64; for i in 0..n { x=black_box(x.wrapping_add(black_box(i))); } println!("{{\"kind\":\"harness_smoke\",\"iterations\":{},\"elapsed_ns\":{},\"checksum\":{}}}",n,start.elapsed().as_nanos(),x); }

fn retrieval()->air_core::indexed::Result<()>{
 use air_core::{indexed::{Indexed,Limits},store::{Store,parse_tsv}};use std::path::PathBuf;
 let args:Vec<_>=std::env::args().collect();let n:usize=args.get(2).ok_or("missing count")?.parse()?;if n==0||n>100000{return Err("count outside 1..100000".into())}
 let path=PathBuf::from(args.get(3).ok_or("missing new DB path")?);let _=std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
 let start=Instant::now();let mut db=Indexed::create(&path,Limits::default())?;for i in 0..n{db.insert(&parse_tsv(&format!("item{i}\\tvalue\\t{i}\\tbench\\tfixture").replace("\\t","\t"))?)?;}let disk_build=start.elapsed().as_nanos();drop(db);
 let start=Instant::now();let mut store=Store::default();for i in 0..n{store.insert(parse_tsv(&format!("item{i}\tvalue\t{i}\tbench\tfixture"))?)?;}let memory_build=start.elapsed().as_nanos();
 let db=Indexed::open(&path,Limits::default())?;let queries=1000;let start=Instant::now();for i in 0..queries{let k=format!("item{}",i%n);let r=db.lookup(&k,"value")?;if r.facts.len()!=1{return Err("disk lookup mismatch".into())}black_box(r);}let disk=start.elapsed().as_nanos();
 let start=Instant::now();for i in 0..queries{let k=format!("item{}",i%n);let r=store.lookup(&k,"value",1);if r.len()!=1{return Err("memory lookup mismatch".into())}black_box(r);}let memory=start.elapsed().as_nanos();
 println!("{{\"kind\":\"synthetic_retrieval\",\"records\":{n},\"queries\":{queries},\"disk_build_ns\":{disk_build},\"memory_build_ns\":{memory_build},\"disk_query_ns\":{disk},\"memory_query_ns\":{memory},\"db_bytes\":{}}}",std::fs::metadata(path)?.len());Ok(())
}
