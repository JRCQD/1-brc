use fxhash;

use crate::station::StationAverage;

pub struct Container {
    pub backing: [Option<StationAverage>; 5_000],
    size: usize,
}

impl Container {
    pub fn new() -> Self {
        let backing_array = [None::<StationAverage>; 5_000];
        return Container {
            backing: backing_array,
            size: 5_000,
        };
    }

    #[inline]
    pub fn insert(&mut self, element: StationAverage, key: &str) {
        let index = self.compute_index(key);
        self.backing[index] = Some(element);
    }

    pub fn sort(&mut self) {
        self.backing.sort();
    }

    #[inline]
    pub fn get_mut(&mut self, key: &str) -> Option<&mut StationAverage> {
        let index = self.compute_index(key);
        self.backing.get_mut(index).unwrap().as_mut()
    }

    #[inline]
    fn compute_index(&self, key: &str) -> usize {
        let hash = fxhash::hash(key);
        let index = hash % self.size;
        index
    }
}
