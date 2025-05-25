#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
use crossbeam::channel::bounded;
use std::time;
use worker_pool::Worker;
mod container;
mod reader;
mod station;
mod worker_pool;

fn main() {
    let base = "/home/ryan/Documents/projects/one_brc";
    let data = format!("{}/{}", base, "measurements.txt");
    let output = format!("{}/{}", base, "output_sixth_pass.txt");
    let (send, rec) = bounded(200_000_000);
    let mut worker = Worker::new(rec, output);
    let handle = std::thread::spawn(move || {
        worker.listen();
    });
    let start = time::Instant::now();
    reader::parse_file(data, send);
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
