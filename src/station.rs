pub struct Station {
    pub name: String,
    pub temp: f32,
}

impl Station {
    pub fn new(line: &str) -> Self {
        let line_atoms: Vec<&str> = line.split(';').collect();
        let name = line_atoms.first().unwrap().to_string();
        let temp: f32 = line_atoms.last().unwrap().to_string().parse().unwrap();
        Station { name, temp }
    }
}

pub struct StationAverage {
    pub name: String,
    min: f32,
    max: f32,
    count: u32,
    running_total: f32,
    pub average: Option<f32>
}

impl StationAverage {
    pub fn new(name: String, temp: f32) -> Self {
        StationAverage {
            name,
            min: temp,
            max: temp,
            count: 1,
            running_total: temp,
            average: None
        }
    }

    #[inline]
    pub fn update_values(&mut self, temp: f32) {
        if temp < self.min {
            self.min = temp
        } else if temp > self.max {
            self.max = temp
        }

        // we're going to do a lot of calls to update_values(), so instead of computing the average
        // each time. We'll just keep a running total and a count of all the temps we've seen. Then
        // at the end of the process, we'll compute the average once.
        self.running_total += temp;
        self.count += 1;
    }

    #[inline]
    pub fn average(&mut self) -> f32 {
        let ave = self.running_total / self.count as f32;
        self.average = Some(ave);
        return ave
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_updating() {
        const EPS: f32 = 1e-6;
        let mut station_average = StationAverage::new("Test".to_string(), 10.0);
        assert_eq!(station_average.average(), 10.0);
        assert_eq!(station_average.max, 10.0);
        assert_eq!(station_average.min, 10.0);
        station_average.update_values(20.0);

        assert_eq!(station_average.min, 10.0);
        assert_eq!(station_average.max, 20.0);
        let expected_average = 15.0;
        // 20 + 10 / 2 = 15
        assert!((station_average.average() - expected_average).abs() <= EPS);

        station_average.update_values(5.0);
        assert_eq!(station_average.min, 5.0);
        assert_eq!(station_average.max, 20.0);
        // (20 + 10 + 5) / 3 = 35/3 = 11 + 2/3
        let expected_average = 11.666666667;
        assert!((station_average.average() - expected_average).abs() <= EPS);
    }
}
