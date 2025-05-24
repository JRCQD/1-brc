use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{BufWriter, Write},
};

use crossbeam::channel::Receiver;
use crate::station::StationAverage;

pub struct Worker {
    rec_chan: Receiver<String>,
    container: HashMap<String, StationAverage>,
    output: String,
}

impl Worker {
    pub fn new(chan: Receiver<String>, out: String) -> Self {
        Worker {
            rec_chan: chan,
            container: HashMap::new(),
            output: out,
        }
    }

    pub fn listen(&mut self) {
        while let Ok(line) = self.rec_chan.recv().map_err(|err| eprintln!("{:?}", err)) {
            let vals: Vec<&str> = line.split(';').collect();
            let station = vals.first().unwrap().to_string();
            let data: f32 = vals.last().unwrap().parse::<f32>().unwrap();
            if let Some(existing) = self.container.get_mut(&station) {
                existing.update_values(data);
            } else {
                let station_ave = StationAverage::new(station.clone(), data);
                self.container.insert(station, station_ave);
            }
        }
        self.write();
    }

    fn write(&self) {
        let mut vec = Vec::with_capacity(self.container.len());
        for (_, data) in self.container.iter() {
            vec.push(data);
        }
        vec.sort();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.output.clone())
            .unwrap();
        let mut writer = BufWriter::new(file);

        for data in vec {
            writeln!(writer, "{}", data.to_string()).unwrap();
        }
    }
}
