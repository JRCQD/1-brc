use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};


struct Slot<T> {
    ready: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>
}

impl<T> Slot<T> {
    fn new(value: T) -> Self {
        unsafe { Slot { ready: AtomicBool::new(false), value: UnsafeCell::new(MaybeUninit::uninit().assume_init()) } }
    }
}

pub struct Producer<T, const N: usize> {
    inner: Arc<RingBuffer<T, N>>
}

impl<T, const N: usize> Producer<T, N> {
    pub fn try_enqueue(&self, elem: T) -> Result<(), T> {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);

        // always take head index mod N to make sure the index is still
        // in the range of the buffer.
        let index = head % N;
        //println!("index: {}", index);

        if head.wrapping_sub(tail) >= N {
            println!("index: {} now wrapping", index);
            return Err(elem);
        };

        unsafe {
            // Using the index, get the &UnsafeCell<MaybeUninit<T>> from the buffer
            // .get(); then takes gets the MaybeUninit<T> from the UnsafeCell
            let cell = self.inner.buffer.get_unchecked(index).get();
            // .write... writes the the cell
            (*cell).write(Some(elem));
        }

        self.inner.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }
}

impl<T, const N: usize> Drop for Producer<T, N> {
    fn drop(&mut self) {
        println!("dropping");
        self.inner.closed.store(true, Ordering::Release);
    }
}

pub struct Consumer<T, const N: usize> {
    inner: Arc<RingBuffer<T, N>>
}

impl<T, const N: usize> Consumer<T,N> {
    pub fn try_dequeue(&self) -> Option<T> {
        loop {
            let tail = self.inner.tail.load(Ordering::Relaxed);
            let head = self.inner.head.load(Ordering::Acquire);

            if tail == head && self.inner.closed.load(Ordering::Acquire) {
                return None;
            }

            let next_tail = tail.wrapping_add(1);
            if self
                .inner
                .tail
                .compare_exchange(tail, next_tail, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                // This is safe from races... I think. Because once we've CAS'd the new tail then
                // we've already reserved our space in the ring buffer. Which means we can safely
                // get the value without there being any racing between different threads.
                let index = tail % N;
                unsafe {
                    let cell = self.inner.buffer.get_unchecked(index).get();
                    let value = (*cell).assume_init_read().take();
                    return value;
                }
            }
        }
    }
}

pub struct RingBuffer<T, const N: usize> {
    buffer: [UnsafeCell<MaybeUninit<Option<T>>>; N],
    head: AtomicUsize,
    tail: AtomicUsize,
    closed: AtomicBool,
}

unsafe impl<T: Send, const N: usize> Sync for RingBuffer<T, N> {}

impl<T, const N: usize> RingBuffer<T, N> {
    pub fn new() -> Self {
        let buffer = unsafe {
            let mut result: [UnsafeCell<MaybeUninit<Option<T>>>; N] = MaybeUninit::uninit().assume_init();
            for i in 0..N {
                result[i] = UnsafeCell::new(MaybeUninit::uninit().assume_init())
            }
            result
        };
        RingBuffer {
            buffer,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        }
    }
}

pub fn channel<T, const N: usize>() -> (Producer<T, N>, Arc<Consumer<T, N>>) {
    let buf = Arc::new(RingBuffer::new());
    let buf_2 = buf.clone();
    (Producer { inner: buf }, Arc::new(Consumer {inner: buf_2}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_enqueue_dequeue() {
        let (prod, con) = channel::<Vec<u8>, 1>();
        prod.try_enqueue([1,2,3,5].to_vec()).unwrap();
        con.try_dequeue();
    }
}
