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
            let sep = bytes.iter().position(|&b| b == b';').unwrap();
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

    #[inline]
    fn parse_string_to_int(&self, val: &str) -> i16 {
        let mut is_negative = false;
        let mut integer_part = 0;
        for char in val.chars() {
            if char == '-' {
                is_negative = true;
                continue;
            }
            match char {
                '0'..='9' => {
                    let digit = (char as u8 - b'0') as i16;
                    integer_part = integer_part * 10 + digit;
                }
                '.' => {
                    continue;
                }
                _ => {}
            }
        }
        if is_negative {
            return -integer_part;
        }
        return integer_part;
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
