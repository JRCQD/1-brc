#[derive(Clone)]
pub struct StationAverage {
    pub name: String,
    min: i16,
    max: i16,
    count: u32,
    running_total: u32,
    mutliplier: f32,
    pub average: Option<f32>,
}

impl PartialEq for StationAverage {
    fn eq(&self, other: &StationAverage) -> bool {
        self.name == other.name
    }
}

impl PartialOrd for StationAverage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.name.cmp(&other.name))
    }
}

impl Ord for StationAverage {
    fn cmp(&self, other: &StationAverage) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl Eq for StationAverage {}

impl StationAverage {
    pub fn new(name: String, temp: i16) -> Self {
        StationAverage {
            name,
            min: temp,
            max: temp,
            count: 1,
            running_total: temp as u32,
            average: None,
            mutliplier: 10.0
        }
    }

    #[inline]
    pub fn update_values(&mut self, temp: i16) {
        if temp < self.min {
            self.min = temp
        } else if temp > self.max {
            self.max = temp
        }

        // we're going to do a lot of calls to update_values(), so instead of computing the average
        // each time. We'll just keep a running total and a count of all the temps we've seen. Then
        // at the end of the process, we'll compute the average once.
        self.running_total += temp as u32;
        self.count += 1;
    }

    #[inline]
    pub fn average(&self) -> f32 {
        let f = self.running_total as f32 / self.mutliplier;
        let ave = f / self.count as f32;
        return ave;
    }

    pub fn to_string(&self) -> String {
        format!(
            "{}={}/{}/{}",
            self.name,
            self.min,
            self.average(), 
            self.max
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_updating() {
        const EPS: f32 = 1e-6;
        let mut station_average = StationAverage::new("Test".to_string(), 100);
        assert_eq!(station_average.average(), 10.0);
        assert_eq!(station_average.max, 100);
        assert_eq!(station_average.min, 100);
        station_average.update_values(200);

        assert_eq!(station_average.min, 100);
        assert_eq!(station_average.max, 200);
        let expected_average = 15.0;
        // 20 + 10 / 2 = 15
        assert!((station_average.average() - expected_average).abs() <= EPS);

        station_average.update_values(50);
        assert_eq!(station_average.min, 50);
        assert_eq!(station_average.max, 200);
        // (20 + 10 + 5) / 3 = 35/3 = 11 + 2/3
        let expected_average = 11.666666667;
        assert!((station_average.average() - expected_average).abs() <= EPS);
    }
}
