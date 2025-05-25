use fxhash;

use crate::station::StationAverage;

pub struct Container {
    pub backing: Vec<Option<StationAverage>>,
    size: usize,
}

impl Container {
    pub fn new() -> Self {
        let backing_array = vec![None; 10_000];
        return Container {
            backing: backing_array,
            size: 10_000,
        };
    }

    #[inline(always)]
    pub fn insert(&mut self, element: StationAverage, key: &str) {
        let index = self.compute_index(key);
        self.backing[index] = Some(element);
    }

    pub fn sort(&mut self) {
        self.backing.sort();
    }

    #[inline(always)]
    pub fn get_mut(&mut self, key: &str) -> Option<&mut StationAverage> {
        let index = self.compute_index(key);
        self.backing.get_mut(index).unwrap().as_mut()
    }

    #[inline(always)]
    fn compute_index(&self, key: &str) -> usize {
        let hash = fxhash::hash(key);
        let index = hash % self.size;
        index
    }
}
