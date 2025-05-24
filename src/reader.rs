use std::{
    fs::File,
    io::{BufRead, BufReader},
};
use crossbeam::channel::Sender;

pub fn parse_file(file: String, queue: Sender<String>) {
    println!("{}", file);
    let file = File::open(file).unwrap();
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.unwrap();
        match queue.send(line) {
            Ok(_) => {
            }
            Err(e) => {
                eprintln!("{:?}", e)
            }
        }
    }
}
