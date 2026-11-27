//! Bounded traversal of explicitly reviewed causal assertions, not causal discovery.
use crate::{indexed::{Indexed,Result},policy::{Policy,Verdict},store::Fact};
use std::collections::{VecDeque,BTreeSet};
#[derive(Debug)]pub struct Explanation{pub paths:Vec<Vec<Fact>>,pub incomplete:bool}
pub fn explain(db:&Indexed,p:&Policy,start:&str,target:&str,max_depth:usize)->Result<Explanation>{
 if max_depth==0||max_depth>4||start.len()>256||target.len()>256||start.trim().is_empty()||target.trim().is_empty(){return Err("causal traversal bounds".into())}
 let norm=|s:&str|s.trim().to_lowercase();let mut q=VecDeque::from([(start.to_owned(),Vec::<Fact>::new(),BTreeSet::from([norm(start)]))]);let mut out=Explanation{paths:vec![],incomplete:false};let(mut expansions,mut bytes)=(0,0usize);
 while let Some((node,path,seen))=q.pop_front(){if expansions>=64{out.incomplete=true;break}expansions+=1;let r=db.lookup(&node,"causes")?;out.incomplete|=r.truncated;
 for f in r.facts{if p.assessment(&f)?.map(|x|x.verdict)!=Some(Verdict::Accepted){continue}if seen.contains(&norm(&f.object)){continue}
 if path.len()>=max_depth{out.incomplete=true;continue}
 let size=f.subject.len()+f.relation.len()+f.object.len()+f.sources.iter().chain(f.categories.iter()).map(String::len).sum::<usize>();let prior=path.iter().map(|x|x.subject.len()+x.relation.len()+x.object.len()+x.sources.iter().chain(x.categories.iter()).map(String::len).sum::<usize>()).sum::<usize>();
 bytes=bytes.saturating_add(size+prior);if bytes>256*1024||q.len()>=64||out.paths.len()>=16{out.incomplete=true;return Ok(out)}
 let next=f.object.clone();let mut np=path.clone();np.push(f);if norm(&next)==norm(target){out.paths.push(np);}else{let mut ns=seen.clone();ns.insert(norm(&next));q.push_back((next,np,ns));}}}
 Ok(out)
}
#[test]fn reviewed_paths_and_cycle(){use crate::{indexed::Limits,policy::Review,store::parse_tsv};use std::path::Path;let mut db=Indexed::create(Path::new(":memory:"),Limits::default()).unwrap();let p=Policy::open(Path::new(":memory:")).unwrap();for l in ["a\tcauses\tb\tx\ts","b\tcauses\tc\tx\ts","b\tcauses\ta\tx\ts"]{let f=parse_tsv(l).unwrap();db.insert(&f).unwrap();p.review(&f.subject,&f.relation,&f.object,&Review{verdict:Verdict::Accepted,reviewer:"fixture".into(),rationale:"synthetic".into()}).unwrap();}assert_eq!(explain(&db,&p,"a","c",3).unwrap().paths[0].len(),2);assert!(explain(&db,&p,"a","a",3).unwrap().paths.is_empty());}
