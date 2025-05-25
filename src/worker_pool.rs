use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
};

use crate::{container::Container, station::StationAverage};
use crossbeam::channel::Receiver;

pub struct Worker {
    rec_chan: Receiver<String>,
    container: Container,
    output: String,
}

impl Worker {
    pub fn new(chan: Receiver<String>, out: String) -> Self {
        Worker {
            rec_chan: chan,
            container: Container::new(),
            output: out,
        }
    }

    pub fn listen(&mut self) {
        while let Ok(line) = self.rec_chan.recv().map_err(|err| eprintln!("{:?}", err)) {
            let bytes = line.as_bytes();
            let sep = self.get_sep(bytes);
            let name = line[..sep].to_string();
            let value = &line[sep + 1..];
            let value = self.parse_string_to_int(value);
            if let Some(existing) = self.container.get_mut(&name) {
                existing.update_values(value);
            } else {
                let station_ave = StationAverage::new(name.clone(), value);
                self.container.insert(station_ave, &name);
            }
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
    fn parse_string_to_int(&self, val: &str) -> i16 {
        let bytes = val.as_bytes();
        let byte_len = bytes.len();
        let frac_part = (bytes[byte_len - 1] -b'0') as i16;
        let mut int_part = 0;
        let is_neg = (bytes[0] == b'-') as usize;
        let mut index = is_neg;
        let max_index = byte_len - 2;
        while index < max_index {
            int_part = int_part * 10 + (bytes[index] - b'0') as i16;
            index += 1;
        };
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
    }
}
