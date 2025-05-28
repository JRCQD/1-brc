const MAX_ARRAY_SIZE: usize = 100;

#[derive(Clone, Debug, Copy)]
pub struct StationAverage {
    pub name: [u8; MAX_ARRAY_SIZE],
    min: i16,
    max: i16,
    count: u32,
    running_total: u32,
    mutliplier: f32,
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
    pub fn new(name: &[u8], temp: i16) -> Self {
        let mut arr = [0u8; MAX_ARRAY_SIZE];
        arr[..name.len()].copy_from_slice(&name[..name.len()]);
        StationAverage {
            name: arr,
            min: temp,
            max: temp,
            count: 1,
            running_total: temp as u32,
            mutliplier: 10.0,
        }
    }

    #[inline(always)]
    pub fn update_values(&mut self, temp: i16) {
        self.min = std::cmp::min(self.min, temp);
        self.max = std::cmp::max(self.max, temp);

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
            self.from_bytes(&mut String::new()),
            self.min,
            self.average(),
            self.max
        )
    }

    fn from_bytes(&self, str_buf: &mut String) -> String {
        // This strips out the null bytes that might be inserted into the string
        // compared to everything else, this will be very slow, but it's only called
        // at most 10,000 times.
        let name: Vec<u8> = self
            .name
            .iter()
            .filter(|c| *(*c) == '\0' as u8)
            .map(|c| *c)
            .collect();
        let x = std::str::from_utf8(&name[..MAX_ARRAY_SIZE]).unwrap();
        str_buf.push_str(x);
        str_buf.to_string()
    }
}

impl Default for StationAverage {
    fn default() -> Self {
        StationAverage {
            name: [0; MAX_ARRAY_SIZE],
            min: 0,
            max: 0,
            count: 0,
            running_total: 0,
            mutliplier: 100.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_updating() {
        const EPS: f32 = 1e-6;
        let mut station_average = StationAverage::new("Test".as_bytes(), 100);
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
