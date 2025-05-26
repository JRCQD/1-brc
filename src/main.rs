// #[global_allocator]
// static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
use ring_buffer::channel;
use std::time;
use worker_pool::{Worker, CHANNEL_SIZE};
mod container;
mod reader;
mod ring_buffer;
mod station;
mod worker_pool;

fn main() {
    let base = "/home/ryan/Documents/projects/one_brc";
    let data = format!("{}/{}", base, "measurements.txt");
    let output = format!("{}/{}", base, "output_ring_buffer.txt");
    let (send, rec) = channel::<Vec<u8>, CHANNEL_SIZE>();
    // let (send, rec) = bounded(100_000_000);
    let mut worker = Worker::new(rec, output);
    let handle = std::thread::spawn(move || {
        worker.listen();
    });
    let start = time::Instant::now();
    reader::parse_file_with_buf(data, send);
    match handle.join() {
        Ok(_) => {
            println!("Closed")
        }
        Err(e) => {
            eprintln!("{:?}", e)
        }
    }
    let end = start.elapsed();
    println!("Completed in {:?}", end);
}
