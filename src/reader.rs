use crate::{station::StationAverage};
use memmap2::Mmap;
use std::{
    arch::x86_64::{__m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8}, fs::File, sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    }, time
};
use affinity::{self, set_thread_affinity};

const ARRAY_SIZE: usize = 1 << 10;
#[repr(C)]
pub struct Container {
    pub backing: [StationAverage; ARRAY_SIZE],
    size: usize,
}

impl Container {
    pub fn new() -> Self {
        let backing_array = [StationAverage::default(); ARRAY_SIZE];
        return Container {
            backing: backing_array,
            size: ARRAY_SIZE,
        };
    }


    #[inline(always)]
    pub fn update(&mut self, key: &[u8], element: i16) {
        let mut index = self.compute_index(key);

        for _ in 0..self.size {
            let station = &mut self.backing[index];

            if station.is_empty() {
                *station = StationAverage::new(key, element);
                return;
            }

            if station.matches_name(key) {
                station.update_values(element);
                return;
            }

            index = (index + 1) & (self.size - 1);
        }

        panic!("station table is full");
    }

    pub fn get(&self, key: &[u8]) -> Option<&StationAverage> {
        let mut index = self.compute_index(key);

        for _ in 0..self.size {
            let station = &self.backing[index];

            if station.is_empty() {
                return None;
            }

            if station.matches_name(key) {
                return Some(station);
            }

            index = (index + 1) & (self.size - 1);
        }

        None
    }

    #[inline(always)]
    fn compute_index(&self, key: &[u8]) -> usize {
        let hash = self.fnv1a(key) as usize;
        let index = hash & (self.size - 1);
        index
    }


    #[inline(always)]
    fn fnv1a(&self, key: &[u8]) -> usize {
        let mut hash = 0xcbf29ce484222325;
        let fnv_prime = 0x100000001b3;
        for b in key {
            hash = hash ^ (*b as u64);
            hash = hash.wrapping_mul(fnv_prime);
        }
        hash as usize
    }
}


const READER_THREADS: usize = 8;
const WORKER_THREADS: usize = 2;
const CHUNK_SIZE: usize = 1 << 16;



pub fn read_with_mmap(file: String) {
    println!("{}", file);
    let file = File::open(file).unwrap();
    let mmap = unsafe { Mmap::map(&file).unwrap() };
    let mmap = Arc::new(mmap);
    let cores: Arc<Vec<usize>> = Arc::new((0..affinity::get_core_num()).collect());
    let file_size = mmap.len();
    let mut handles = vec![];
    let counter = Arc::new(AtomicUsize::new(0));
    for i in 0..READER_THREADS {
        let mmap = mmap.clone();
        let c = counter.clone();
        let cores = cores.clone();
        let h = std::thread::spawn(move || {
            set_thread_affinity([cores[i]]).unwrap();
            worker(mmap, file_size, c, i)
        });
        handles.push(h);
    }

    for h in handles {
        let _ = h.join().unwrap();
    }
}

fn worker(mmap: Arc<Mmap>, file_size: usize, counter: Arc<AtomicUsize>, _worker_id: usize) -> Container {
    let mut container = Container::new();
    // println!("worker: {} is pinned to {:?}", worker_id, get_thread_affinity().unwrap());
    loop {
        let start = counter.fetch_add(CHUNK_SIZE, Ordering::Relaxed);
        if start >= file_size {
            break;
        }
        let (start_bound, end_bound) = find_start_end_bounds(&mmap, start, CHUNK_SIZE);
        let chunk = &mmap[start_bound..end_bound];
        unsafe { process_lines(chunk, &mut container) };
    }
    container
}

#[inline(always)]
fn get_sep(bytes: &[u8]) -> usize {
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
pub fn parse_string_to_int(bytes: &[u8]) -> i16 {
    let byte_len = bytes.len();
    let frac_part = (bytes[byte_len - 1] ^ 0x30) as i16;
    let mut int_part = 0;
    let is_negative = bytes[0] == b'-';
    let mut index = is_negative as usize;
    let max_index = byte_len - 2;
    while index < max_index {
        int_part = int_part * 10 + (bytes[index] ^ 0x30) as i16;
        index += 1;
    }
    int_part = int_part * 10 + frac_part;
    if is_negative {
        -int_part
    } else {
        int_part
    }
}

#[inline(always)]
fn find_start_end_bounds(bytes: &[u8], start: usize, chunk_size: usize) -> (usize, usize) {
    let file_size = bytes.len();
    let mut start_bound = start;

    if start_bound > 0 && bytes[start_bound - 1] != b'\n' {
        while start_bound < file_size && bytes[start_bound] != b'\n' {
            start_bound += 1;
        }
        if start_bound < file_size {
            start_bound += 1;
        }
    }

    let mut end_bound = start.saturating_add(chunk_size).min(file_size);
    while end_bound < file_size && bytes[end_bound] != b'\n' {
        end_bound += 1;
    }
    if end_bound < file_size {
        end_bound += 1;
    }

    (start_bound, end_bound)
}

#[inline(always)]
unsafe fn process_lines(chunk: &[u8], container: &mut Container) {
    const REG_WIDTH: usize = 16;
    let mut position = 0;
    let mut line_start = 0;
    let needle = _mm_set1_epi8(b'\n' as i8);
    while position + REG_WIDTH <= chunk.len() {
        let c = _mm_loadu_si128(chunk.as_ptr().add(position) as *const __m128i);
        let cmpr = _mm_cmpeq_epi8(c, needle);
        let mut mask = _mm_movemask_epi8(cmpr);
        while mask != 0 {
            let line_end = position + mask.trailing_zeros() as usize;
            process_line(&chunk[line_start..line_end], container);
            line_start = line_end + 1;
            mask &= mask - 1;
        }
        position += REG_WIDTH;
    }
    while position < chunk.len() {
        if chunk[position] == b'\n' {
            process_line(&chunk[line_start..position], container);
            line_start = position + 1;
        }
        position += 1;
    }
}

#[inline(always)]
fn process_line(line: &[u8], container: &mut Container) {
    let separator = get_sep(line);
    let name = &line[..separator];
    let value = parse_string_to_int(&line[separator + 1..]);
    container.update(name, value);
}

#[cfg(test)]
mod tests {
    use super::{find_start_end_bounds, Container, ARRAY_SIZE};

    #[test]
    fn chunk_ranges_assign_each_line_once() {
        let bytes = b"a;1.0\nbb;2.0\nccc;3.0\n";
        let chunk_size = 8;
        let mut reconstructed = Vec::new();

        for start in (0..bytes.len()).step_by(chunk_size) {
            let (start_bound, end_bound) = find_start_end_bounds(bytes, start, chunk_size);
            reconstructed.extend_from_slice(&bytes[start_bound..end_bound]);
        }

        assert_eq!(reconstructed, bytes);
    }

    #[test]
    fn linear_probing_keeps_colliding_stations_separate() {
        let mut container = Container::new();
        let mut first_key_for_index: Vec<Option<Vec<u8>>> = vec![None; ARRAY_SIZE];
        let mut collision = None;

        for number in 0..=ARRAY_SIZE {
            let key = format!("station-{number}").into_bytes();
            let index = container.compute_index(&key);

            if let Some(first_key) = &first_key_for_index[index] {
                collision = Some((first_key.clone(), key));
                break;
            }

            first_key_for_index[index] = Some(key);
        }

        let (first_key, second_key) = collision.expect("pigeonhole principle guarantees a collision");
        container.update(&first_key, 100);
        container.update(&second_key, 200);
        container.update(&first_key, 300);

        assert_eq!(container.get(&first_key).unwrap().count(), 2);
        assert_eq!(container.get(&second_key).unwrap().count(), 1);
    }

    #[test]
    fn parses_negative_temperatures_as_signed_tenths() {
        assert_eq!(super::parse_string_to_int(b"0.0"), 0);
        assert_eq!(super::parse_string_to_int(b"12.3"), 123);
        assert_eq!(super::parse_string_to_int(b"-12.3"), -123);
        assert_eq!(super::parse_string_to_int(b"-99.9"), -999);
    }

    #[test]
    fn processes_lines_without_storing_newline_positions() {
        let mut container = Container::new();

        unsafe { super::process_lines(b"a;1.0\nb;-2.0\na;3.0\n", &mut container) };

        let first = container.get(b"a").unwrap();
        assert_eq!(first.count(), 2);
        assert_eq!(first.average(), 2.0);

        let second = container.get(b"b").unwrap();
        assert_eq!(second.count(), 1);
        assert_eq!(second.average(), -2.0);
    }
}
