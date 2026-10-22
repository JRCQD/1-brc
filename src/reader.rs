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
        let index = self.compute_index(key);
        unsafe { self.backing.get_unchecked_mut(index).update_values(element); };
    }

    pub fn get(&self, key: &[u8]) -> StationAverage {
        let idx = self.compute_index(key);
        self.backing[idx]
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
            hash = hash * fnv_prime;
        }
        hash as usize
    }
}


const READER_THREADS: usize = 8;
const WORKER_THREADS: usize = 2;
const CHUNK_SIZE: usize = 1 << 16;
const ARR_LENGTH: usize = 1 << 16;



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
        let _containter = h.join().unwrap();
    }
}

fn worker(mmap: Arc<Mmap>, file_size: usize, counter: Arc<AtomicUsize>, _worker_id: usize) -> Container {
    let mut container = Container::new();
    let mut line_break_positions: [Option<usize>; ARR_LENGTH] = [None; ARR_LENGTH];
    // println!("worker: {} is pinned to {:?}", worker_id, get_thread_affinity().unwrap());
    loop {
        let start = counter.fetch_add(CHUNK_SIZE, Ordering::Relaxed);
        if start >= file_size {
            break;
        }
        let (start_bound, end_bound) = find_start_end_bounds(&mmap, start, CHUNK_SIZE);
        let chunk = &mmap[start_bound..end_bound];
        let line_ends = unsafe { find_line_breaks(chunk, &mut line_break_positions) };
        let mut current = 0;
        for pos in line_ends {
            if pos.is_none() {
                break
            }
            let pos = pos.unwrap();
            let line = &chunk[current..pos];
            current = pos + 1;
            let sep = get_sep(line);
            let name = &line[..sep];
            let value = &line[sep + 1..];
            let value = parse_string_to_int(value);
            container.update(name, value);
        }
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
    let is_neg = (bytes[0] == 0x30) as usize;
    let mut index = is_neg;
    let max_index = byte_len - 2;
    while index < max_index {
        int_part = int_part * 10 + (bytes[index] ^ 0x30) as i16;
        index += 1;
    }
    int_part = int_part * 10 + frac_part;
    if is_neg == 1 {
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
unsafe fn find_line_breaks<'a>(chunk: &[u8], new_lines: &'a mut [Option<usize>; ARR_LENGTH]) -> &'a mut [Option<usize>; ARR_LENGTH] {
    const REG_WIDTH: usize = 16;
    let mut i = 0;
    let needle = _mm_set1_epi8(b'\n' as i8);
    let mut counter = 0;
    while i + REG_WIDTH <= chunk.len() {
        let c = _mm_loadu_si128(chunk.as_ptr().add(i) as *const __m128i);
        let cmpr = _mm_cmpeq_epi8(c, needle);
        let mut mask = _mm_movemask_epi8(cmpr);
       // if mask != 0 {
       //     for j in 0..REG_WIDTH {
       //         if (mask & (1 << j)) != 0 {
       //             new_lines[counter] = Some(i + j);
       //             counter += 1;
       //         }
       //     }
       // }
       //
       // I asked ChatGPT for a branchless version for this, and it gave me the following.
       while mask != 0 {
           let tz = mask.trailing_zeros() as usize;
           new_lines[counter] = Some(i +tz);
           counter += 1;
           mask &= mask - 1;
       }
        i += REG_WIDTH;
    }
    while i < chunk.len() {
        if chunk[i] == b'\n' {
            new_lines[counter] = Some(i);
            counter += 1;
        }
        i += 1;
    }
    while counter < ARR_LENGTH {
        new_lines[counter] = None;
        counter += 1;
    }
    new_lines
}

#[cfg(test)]
mod tests {
    use super::find_start_end_bounds;

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
}
