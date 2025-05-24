use queue::LockFreeQueue;
use reader::MemMappedReader;
use std::sync::Arc;

mod queue;
mod reader;
mod station;
mod worker_pool;


fn main() {
    let queue = LockFreeQueue::new();
    let q = Arc::new(queue);
    let reader = MemMappedReader::new(String::from(""), q);
    reader.parse();
}
