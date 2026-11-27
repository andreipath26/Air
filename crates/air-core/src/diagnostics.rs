//! Runtime counters with no hidden logging of question content.
use serde::Serialize;
#[derive(Debug,Default,Serialize)]pub struct Metrics{pub requests:u64,pub errors:u64,pub model_calls:u64,pub completed:u64,pub elapsed_ns:u128}
impl Metrics{pub fn record(&mut self,result:&crate::engine::Response,elapsed:u128){self.requests=self.requests.saturating_add(1);self.model_calls=self.model_calls.saturating_add(u64::from(result.model_called));self.completed=self.completed.saturating_add(u64::from(result.complete));self.elapsed_ns=self.elapsed_ns.saturating_add(elapsed);}}
