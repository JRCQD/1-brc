use std::{
    collections::HashMap, fs::File, sync::{Mutex, RwLock, Arc}
};
use memmap2::Mmap;

use crate::{queue::LockFreeQueue, station::StationAverage};

const CHUNK_SIZE: usize = 64 * 1024 * 1024;
type StationData = RwLock<HashMap<String, Mutex<StationAverage>>>;


pub struct MemMappedReader {
    mapper: Mmap,
    queue: Arc<LockFreeQueue<Vec<u8>>>
}

impl MemMappedReader {
    pub fn new(path: String, queue: Arc<LockFreeQueue<Vec<u8>>>) -> Self {
        let file = File::open(path).unwrap();
        let mmap = unsafe {
            Mmap::map(&file).unwrap()
        };
        MemMappedReader { mapper: mmap, queue }
    }

    pub fn parse(&self) {
        let mut offset = 0;
        let total_length = self.mapper.len();

        while offset < total_length {
            let end = offset + CHUNK_SIZE;
            let end = end.min(total_length);
            let mut chunk_end = end;

            while chunk_end < total_length && self.mapper[chunk_end] != b'\n' {
                chunk_end += 1;
            }

            let chunk = &self.mapper[offset..chunk_end];
            self.queue.enqueue(chunk.to_vec());
            offset = chunk_end + 1;
        }
    }
}

