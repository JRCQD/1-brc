use reader::read_with_mmap;
use std::time;
mod container;
mod reader;
mod station;

fn main() {
    let base = "/home/ryan/Documents/projects/one_brc";
    let data = format!("{}/{}", base, "measurements.txt");
    let start = time::Instant::now();
    read_with_mmap(data);
    let end = start.elapsed();
    println!("Completed in {:?}", end);
}
