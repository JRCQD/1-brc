use crossbeam::channel::Sender;
use std::{
    fs::File,
    io::{BufRead, BufReader},
};

pub fn parse_file(file: String, queue: Sender<String>) {
    println!("{}", file);
    let file = File::open(file).unwrap();
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.unwrap();
        match queue.send(line) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("{:?}", e)
            }
        }
    }
}

pub fn parse_file_with_buf(file: String, queue: Sender<Vec<u8>>) {
    println!("{}", file);
    let file = File::open(file).unwrap();
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::with_capacity(1024);
    while reader.read_until(b'\n', &mut buffer).unwrap() > 0 {
        if let Some(&b'\n') = buffer.last() {
            buffer.pop();
        }
        queue.send(buffer.clone()).unwrap();
        buffer.clear();
    }
}
