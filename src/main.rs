use crossbeam::channel::bounded;
use worker_pool::Worker;
use std::time;
mod reader;
mod station;
mod worker_pool;

fn main() {
    let base = "/home/ryan/Documents/projects/one_brc";
    let data = format!("{}/{}", base, "measurements.txt");
    let output = format!("{}/{}", base, "output_fifth_pass.txt");
    let (send, rec) = bounded(100_000_000);
    let mut worker = Worker::new(rec, output);
    let handle = std::thread::spawn(move || {
        worker.listen();
    });
    let start = time::Instant::now();
    reader::parse_file(data, send);
    match handle.join() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{:?}", e)
        }
    }
    let end = start.elapsed();
    println!("Completed in {:?}", end);
}
