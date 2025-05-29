use crate::container::Container;
use crate::station::StationAverage;
use memmap2::Mmap;
use std::{
    arch::x86_64::{__m128i, __m256i, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_movemask_epi8, _mm256_set1_epi8, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8}, fs::File, sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    }, time
};

const NUM_THREADS: usize = 8;
const CHUNK_SIZE: usize = 4096;
const ARR_LENGTH: usize = 4096;

pub fn read_with_mmap(file: String) {
    println!("{}", file);
    let file = File::open(file).unwrap();
    let mmap = unsafe { Mmap::map(&file).unwrap() };
    let mmap = Arc::new(mmap);

    let file_size = mmap.len();

    let mut handles = vec![];
    let counter = Arc::new(AtomicUsize::new(CHUNK_SIZE));
    for _ in 0..NUM_THREADS {
        let mmap = mmap.clone();
        let c = counter.clone();
        let h = std::thread::spawn(move || worker(mmap, file_size, c));
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }
}

fn worker(mmap: Arc<Mmap>, file_size: usize, counter: Arc<AtomicUsize>) -> Container {
    let mut container = Container::new();
    let mut line_break_positions: [Option<usize>; ARR_LENGTH] = [None; ARR_LENGTH];
    loop {
        let c = counter.fetch_add(CHUNK_SIZE, Ordering::AcqRel);
        if c >= file_size {
            break;
        }
        let (start_bound, end_bound) = find_start_end_bounds(&mmap, &c, file_size);
        let chunk = &mmap[c - CHUNK_SIZE + start_bound..end_bound];
        let line_ends = unsafe { find_line_breaks(chunk, &mut line_break_positions) };
        // let lines: Vec<&[u8]> = chunk.split(|b| *b == b'\n').collect();
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
            if let Some(existing) = container.get_mut(name) {
                existing.update_values(value);
            } else {
                let station_ave = StationAverage::new(name, value);
                container.insert(station_ave, name);
            }
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
fn parse_string_to_int(bytes: &[u8]) -> i16 {
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

#[inline(always)]
fn find_start_end_bounds(mmap: &Mmap, c: &usize, file_size: usize) -> (usize, usize) {
    let mut additional_offset: usize = 0;
    // check if we're at the start of a line
    if c - CHUNK_SIZE > 0 && mmap[c - CHUNK_SIZE - 1] != b'\n' {
        // if the first character from where we start isn't a new line, then we're not
        // at the start of a line, and another thread has read this line. So we read to
        // the start of the next line.
        while mmap[c - CHUNK_SIZE + additional_offset] != b'\n' {
            additional_offset += 1
        }
        additional_offset += 1;
    }
    let mut ending_offset: usize = 0;
    while (c + CHUNK_SIZE + ending_offset) < file_size
        && mmap[c + CHUNK_SIZE + ending_offset] != b'\n'
    {
        ending_offset += 1;
    }

    let end_chunk = {
        if c + CHUNK_SIZE + ending_offset > file_size {
            file_size
        } else {
            c + CHUNK_SIZE + ending_offset
        }
    };
    return (additional_offset, end_chunk);
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
        let mask = _mm_movemask_epi8(cmpr);

        if mask != 0 {
            for j in 0..REG_WIDTH {
                if (mask & (1 << j)) != 0 {
                    new_lines[counter] = Some(i + j);
                    counter += 1;
                }
            }
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
