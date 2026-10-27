use crate::station::StationAverage;
use affinity::{self, set_thread_affinity};
use memmap2::Mmap;
use std::{
    arch::x86_64::{
        __m128i, __m256i, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_movemask_epi8,
        _mm256_set1_epi8, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8,
    },
    fs::File,
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

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
        let hash = self.hash_name(key) as usize;
        let index = hash & (self.size - 1);
        index
    }

    #[inline(always)]
    fn hash_name(&self, key: &[u8]) -> u64 {
        const BLOCK_MIX: u64 = 0x9e3779b185ebca87;
        const FINAL_MIX_1: u64 = 0xff51afd7ed558ccd;
        const FINAL_MIX_2: u64 = 0xc4ceb9fe1a85ec53;

        let mut hash = (key.len() as u64).wrapping_mul(BLOCK_MIX);
        let mut chunks = key.chunks_exact(8);

        for chunk in chunks.by_ref() {
            let word = u64::from_le_bytes(chunk.try_into().unwrap());
            hash ^= word;
            hash = hash.rotate_left(27).wrapping_mul(BLOCK_MIX);
        }

        let remainder = chunks.remainder();
        if !remainder.is_empty() {
            let mut tail = 0u64;
            for (shift, byte) in remainder.iter().enumerate() {
                tail |= u64::from(*byte) << (shift * 8);
            }
            hash ^= tail;
            hash = hash.rotate_left(27).wrapping_mul(BLOCK_MIX);
        }

        hash ^= hash >> 33;
        hash = hash.wrapping_mul(FINAL_MIX_1);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(FINAL_MIX_2);
        hash ^ (hash >> 33)
    }
}

const READER_THREADS: usize = 8;
const CHUNK_SIZE: usize = 1 << 21;

struct WorkerProfile {
    worker_id: usize,
    core_id: usize,
    chunks: usize,
    bytes: usize,
    wall_time: Duration,
    cpu_time: Duration,
    minor_page_faults: i64,
    major_page_faults: i64,
    slowest_chunk: Duration,
}

#[derive(Clone, Copy)]
struct PageFaults {
    minor: i64,
    major: i64,
}

enum WorkerResult {
    Standard(Container),
    Profiled(Container, WorkerProfile),
}

pub fn read_with_mmap(file: String) {
    let read_start = Instant::now();
    let profiling = std::env::var_os("BRC_PROFILE").is_some();
    println!("{}", file);
    let file = File::open(file).unwrap();
    let advisory_result =
        unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL) };
    assert_eq!(advisory_result, 0, "failed to set sequential-read advisory");
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
            if profiling {
                let (container, profile) = profiled_worker(mmap, file_size, c, i, cores[i]);
                WorkerResult::Profiled(container, profile)
            } else {
                WorkerResult::Standard(worker(mmap, file_size, c, i))
            }
        });
        handles.push(h);
    }

    let mut profiles = Vec::with_capacity(READER_THREADS);
    for h in handles {
        match h.join().unwrap() {
            WorkerResult::Standard(container) => drop(container),
            WorkerResult::Profiled(container, profile) => {
                drop(container);
                profiles.push(profile);
            }
        }
    }

    if profiling {
        print_profile(&mut profiles, file_size, read_start.elapsed());
    }
}

fn worker(
    mmap: Arc<Mmap>,
    file_size: usize,
    counter: Arc<AtomicUsize>,
    _worker_id: usize,
) -> Container {
    let mut container = Container::new();
    // println!("worker: {} is pinned to {:?}", worker_id, get_thread_affinity().unwrap());
    loop {
        let start = counter.fetch_add(CHUNK_SIZE, Ordering::Relaxed);
        if start >= file_size {
            break;
        }
        let (start_bound, end_bound) = find_start_end_bounds(&mmap, start, CHUNK_SIZE);
        let chunk = &mmap[start_bound..end_bound];
        process_lines(chunk, &mut container);
    }
    container
}

fn profiled_worker(
    mmap: Arc<Mmap>,
    file_size: usize,
    counter: Arc<AtomicUsize>,
    worker_id: usize,
    core_id: usize,
) -> (Container, WorkerProfile) {
    let worker_start = Instant::now();
    let cpu_start = current_thread_cpu_time();
    let faults_start = current_thread_page_faults();
    let mut container = Container::new();
    let mut chunks = 0;
    let mut bytes = 0;
    let mut slowest_chunk = Duration::ZERO;

    loop {
        let start = counter.fetch_add(CHUNK_SIZE, Ordering::Relaxed);
        if start >= file_size {
            break;
        }

        let chunk_start = Instant::now();
        let (start_bound, end_bound) = find_start_end_bounds(&mmap, start, CHUNK_SIZE);
        let chunk = &mmap[start_bound..end_bound];
        process_lines(chunk, &mut container);

        chunks += 1;
        bytes += chunk.len();
        slowest_chunk = slowest_chunk.max(chunk_start.elapsed());
    }

    let faults_end = current_thread_page_faults();
    let profile = WorkerProfile {
        worker_id,
        core_id,
        chunks,
        bytes,
        wall_time: worker_start.elapsed(),
        cpu_time: current_thread_cpu_time().saturating_sub(cpu_start),
        minor_page_faults: faults_end.minor - faults_start.minor,
        major_page_faults: faults_end.major - faults_start.major,
        slowest_chunk,
    };
    (container, profile)
}

fn current_thread_page_faults() -> PageFaults {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let result = unsafe { libc::getrusage(libc::RUSAGE_THREAD, usage.as_mut_ptr()) };
    assert_eq!(result, 0, "failed to read worker resource usage");
    let usage = unsafe { usage.assume_init() };
    PageFaults {
        minor: usage.ru_minflt,
        major: usage.ru_majflt,
    }
}

fn current_thread_cpu_time() -> Duration {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    assert_eq!(result, 0, "failed to read worker CPU time");
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

fn print_profile(profiles: &mut [WorkerProfile], file_size: usize, total_wall_time: Duration) {
    profiles.sort_by_key(|profile| profile.worker_id);
    let total_cpu_time: Duration = profiles.iter().map(|profile| profile.cpu_time).sum();
    let total_minor_faults: i64 = profiles
        .iter()
        .map(|profile| profile.minor_page_faults)
        .sum();
    let total_major_faults: i64 = profiles
        .iter()
        .map(|profile| profile.major_page_faults)
        .sum();

    eprintln!(
        "Profile: {:.2} GiB, wall {:?}, aggregate worker CPU {:?}, page faults {} minor / {} major",
        file_size as f64 / (1024.0 * 1024.0 * 1024.0),
        total_wall_time,
        total_cpu_time,
        total_minor_faults,
        total_major_faults,
    );
    for profile in profiles {
        let throughput = profile.bytes as f64 / profile.wall_time.as_secs_f64() / 1_000_000_000.0;
        let cpu_share = profile.cpu_time.as_secs_f64() / profile.wall_time.as_secs_f64() * 100.0;
        eprintln!(
            "  worker {} (CPU {}): {:6} chunks, {:5.2} GiB, wall {:?}, CPU {:?} ({:4.1}%), {} minor / {} major faults, {:.2} GB/s, slowest chunk {:?}",
            profile.worker_id,
            profile.core_id,
            profile.chunks,
            profile.bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            profile.wall_time,
            profile.cpu_time,
            cpu_share,
            profile.minor_page_faults,
            profile.major_page_faults,
            throughput,
            profile.slowest_chunk,
        );
    }
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
fn process_lines(chunk: &[u8], container: &mut Container) {
    if std::is_x86_feature_detected!("avx2") {
        unsafe { process_lines_avx2(chunk, container) };
    } else {
        unsafe { process_lines_sse2(chunk, container) };
    }
}

unsafe fn process_lines_sse2(chunk: &[u8], container: &mut Container) {
    const REG_WIDTH: usize = 16;
    let mut position = 0;
    let mut line_start = 0;
    let needle = _mm_set1_epi8(b'\n' as i8);
    while position + REG_WIDTH <= chunk.len() {
        let c = _mm_loadu_si128(chunk.as_ptr().add(position) as *const __m128i);
        let cmpr = _mm_cmpeq_epi8(c, needle);
        let mut mask = _mm_movemask_epi8(cmpr) as u32;
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

#[target_feature(enable = "avx2")]
unsafe fn process_lines_avx2(chunk: &[u8], container: &mut Container) {
    const REG_WIDTH: usize = 32;
    let mut position = 0;
    let mut line_start = 0;
    let needle = _mm256_set1_epi8(b'\n' as i8);
    while position + REG_WIDTH <= chunk.len() {
        let c = _mm256_loadu_si256(chunk.as_ptr().add(position) as *const __m256i);
        let cmpr = _mm256_cmpeq_epi8(c, needle);
        let mut mask = _mm256_movemask_epi8(cmpr) as u32;
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

        let (first_key, second_key) =
            collision.expect("pigeonhole principle guarantees a collision");
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
    fn finds_separator_for_each_temperature_shape() {
        assert_eq!(super::get_sep(b"X;1.2"), 1);
        assert_eq!(super::get_sep(b"X;12.3"), 1);
        assert_eq!(super::get_sep(b"X;-1.2"), 1);
        assert_eq!(super::get_sep(b"X;-12.3"), 1);
    }

    #[test]
    fn processes_lines_without_storing_newline_positions() {
        let mut container = Container::new();

        super::process_lines(b"a;1.0\nb;-2.0\na;3.0\n", &mut container);

        let first = container.get(b"a").unwrap();
        assert_eq!(first.count(), 2);
        assert_eq!(first.average(), 2.0);

        let second = container.get(b"b").unwrap();
        assert_eq!(second.count(), 1);
        assert_eq!(second.average(), -2.0);
    }

    #[test]
    fn sse2_fallback_processes_lines_correctly() {
        let mut container = Container::new();

        unsafe { super::process_lines_sse2(b"a;1.0\nb;-2.0\na;3.0\n", &mut container) };

        let first = container.get(b"a").unwrap();
        assert_eq!(first.count(), 2);
        assert_eq!(first.average(), 2.0);

        let second = container.get(b"b").unwrap();
        assert_eq!(second.count(), 1);
        assert_eq!(second.average(), -2.0);
    }

    #[test]
    fn word_hash_includes_middle_bytes_and_remainder() {
        let container = Container::new();

        assert_ne!(
            container.hash_name(b"abcdefghMIDDLE00qrstuvwx"),
            container.hash_name(b"abcdefghMIDDLE11qrstuvwx")
        );
        assert_ne!(
            container.hash_name(b"abcdefghZ"),
            container.hash_name(b"abcdefghY")
        );
    }
}
