use fxhash;

use crate::station::StationAverage;
const ARRAY_SIZE: usize = 1 << 10;
#[repr(C)]
pub struct Container {
    pub backing: [Option<StationAverage>; ARRAY_SIZE],
    size: usize,
}

impl Container {
    pub fn new() -> Self {
        let backing_array = [None::<StationAverage>; ARRAY_SIZE];
        return Container {
            backing: backing_array,
            size: ARRAY_SIZE,
        };
    }

    #[inline(always)]
    pub fn insert(&mut self, element: StationAverage, key: &[u8]) {
        let index = self.compute_index(key);
        self.backing[index] = Some(element);
    }

    #[inline(always)]
    pub fn get_mut(&mut self, key: &[u8]) -> Option<&mut StationAverage> {
        let index = self.compute_index(key);
        unsafe { self.backing.get_unchecked_mut(index).as_mut() }
    }

    #[inline(always)]
    fn compute_index(&self, key: &[u8]) -> usize {
        let hash = fxhash::hash(key);
        let index = hash & (self.size - 1);
        index
    }
}
