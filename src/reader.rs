use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use crate::ring_buffer::Producer;

pub fn parse_file_with_buf(file: String, queue: Producer<Vec<u8>>) {
    println!("{}", file);
    let file = File::open(file).unwrap();
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::with_capacity(1024);
    while reader.read_until(b'\n', &mut buffer).unwrap() > 0 {
        if let Some(&b'\n') = buffer.last() {
            buffer.pop();
        }
        // println!("Before loop");
        loop {
            // println!("Loop start");
            match queue.try_enqueue(buffer.clone()) {
                Ok(_) => {
                    buffer.clear();
                    break;
                }
                Err(_e) => {
                    // println!("err");
                    continue;
                }
            }
        }
    }
}
