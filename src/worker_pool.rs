use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    sync::Arc
};

pub const CHANNEL_SIZE: usize = 1;
use crate::{container::Container, station::StationAverage, ring_buffer::Consumer};

pub struct Worker {
    rec_chan: Arc<Consumer<Vec<u8>, CHANNEL_SIZE>>,
    container: Container,
    output: String,
}

impl Worker {
    pub fn new(chan: Arc<Consumer<Vec<u8>, CHANNEL_SIZE>>, out: String) -> Self {
        Worker {
            rec_chan: chan,
            container: Container::new(),
            output: out,
        }
    }

    pub fn listen(&mut self) {
        let mut counter = 0;
        while let Some(bytes) = self.rec_chan.try_dequeue() {
            // counter += 1;
            // println!("dequeuing {:?}", counter);
            // let start = Instant::now();
            let sep = self.get_sep(&bytes);
            let name = &bytes[..sep];
            let value = &bytes[sep + 1..];
            let value = self.parse_string_to_int(value);
            if let Some(existing) = self.container.get_mut(name) {
                existing.update_values(value);
            } else {
                let station_ave = StationAverage::new(name, value);
                self.container.insert(station_ave, name);
            }
            // counter += 1;
            // if counter % 100 == 0 {
            //     let end = start.elapsed();
            //     self.timeings.push(end.as_nanos());
            // }
        }
        self.write();
    }

    #[inline(always)]
    fn get_sep(&self, bytes: &[u8]) -> usize {
        let size = bytes.len();
        if bytes[size - 4] == b';' {
            size - 4
        } else if bytes[size - 5] == b';' {
            size - 5
        } else {
            size - 6
        }
    }

    #[inline(always)]
    fn parse_string_to_int(&self, bytes: &[u8]) -> i16 {
        let byte_len = bytes.len();
        let frac_part = (bytes[byte_len - 1] - b'0') as i16;
        let mut int_part = 0;
        let is_neg = (bytes[0] == b'-') as usize;
        let mut index = is_neg;
        let max_index = byte_len - 2;
        while index < max_index {
            int_part = int_part * 10 + (bytes[index] - b'0') as i16;
            index += 1;
        }
        int_part = int_part * 10 + frac_part;
        if is_neg == 1 {
            -int_part
        } else {
            int_part
        }
    }

    fn write(&mut self) {
        self.container.sort();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.output.clone())
            .unwrap();
        let mut writer = BufWriter::new(file);
        for data in &self.container.backing {
            match data.as_ref() {
                Some(d) => {
                    writeln!(writer, "{}", d.to_string()).unwrap();
                }
                None => {}
            }
        }
        // let file = OpenOptions::new()
        // .create(true)
        // .append(true)
        // .open("no_strings_loop_time.txt")
        // .unwrap();
        // let mut writer = BufWriter::new(file);
        // for data in &self.timeings {
        //     writeln!(writer, "{},", data).unwrap();
        // }
    }
}
