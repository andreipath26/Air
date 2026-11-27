//! Individually compressed, indexed knowledge blocks. Random lookup does not decompress unrelated blocks.
use crate::{indexed::Result,consolidation::digest};
use rusqlite::{Connection,params,OptionalExtension};
use flate2::{write::ZlibEncoder,read::ZlibDecoder,Compression};
use std::{io::{Read,Write},path::Path};
const MAX:usize=256*1024;
pub struct Compact{db:Connection}
impl Compact{
 pub fn open(path:&Path)->Result<Self>{let db=Connection::open(path)?;db.pragma_update(None,"cache_size",-1024)?;db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,1048576);db.execute_batch("CREATE TABLE IF NOT EXISTS compact_blocks(key TEXT PRIMARY KEY,hash TEXT NOT NULL,raw_size INTEGER NOT NULL,body BLOB NOT NULL) WITHOUT ROWID;")?;Ok(Self{db})}
 pub fn put(&self,key:&str,raw:&[u8])->Result<usize>{if key.is_empty()||key.len()>256||raw.len()>MAX{return Err("block limits".into())}let mut e=ZlibEncoder::new(Vec::new(),Compression::fast());e.write_all(raw)?;let b=e.finish()?;let n=b.len();self.db.execute("INSERT INTO compact_blocks VALUES (?1,?2,?3,?4) ON CONFLICT(key) DO UPDATE SET hash=excluded.hash,raw_size=excluded.raw_size,body=excluded.body",params![key,digest(raw),raw.len() as i64,b])?;Ok(n)}
 pub fn get(&self,key:&str)->Result<Option<Vec<u8>>>{let row:Option<(String,i64,Vec<u8>)>=self.db.query_row("SELECT hash,raw_size,body FROM compact_blocks WHERE key=?1",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;let Some((hash,n,b))=row else{return Ok(None)};if n<0||n as usize>MAX||b.len()>MAX+4096{return Err("invalid compressed block".into())}let mut out=Vec::new();ZlibDecoder::new(&b[..]).take(MAX as u64+1).read_to_end(&mut out)?;if out.len()!=n as usize||digest(&out)!=hash{return Err("block integrity failure".into())}Ok(Some(out))}
}
#[test]fn selective(){let c=Compact::open(Path::new(":memory:")).unwrap();let x=vec![42;10000];assert!(c.put("a",&x).unwrap()<x.len());c.put("b",b"other").unwrap();assert_eq!(c.get("a").unwrap(),Some(x));assert_eq!(c.get("missing").unwrap(),None);}
