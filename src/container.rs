use crate::station::StationAverage;
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
