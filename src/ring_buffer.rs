use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

#[repr(align(64))]
struct Slot<T> {
    ready: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Slot<T> {
    fn new() -> Self {
        Slot {
            ready: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    fn get(&self) -> T {
        let val = unsafe { (*self.value.get()).assume_init_read() };
        self.ready.store(false, Ordering::Release);
        val
    }

    fn write(&self, value: T) {
        unsafe { (*self.value.get()).write(value) };
        self.ready.store(true, Ordering::Release);
    }
}

pub struct Producer<T> {
    inner: Arc<RingBuffer<T>>,
}

impl<T> Producer<T> {
    pub fn try_enqueue(&self, elem: T) -> Result<(), T> {
        let head = self.inner.head.load(Ordering::Acquire);
        let tail = self.inner.tail.load(Ordering::Relaxed);

        // always take head index mod N to make sure the index is still
        // in the range of the buffer.
        let index = head % self.inner.capacity;
        // println!("index: {}", index);
        // println!("head: {}, tail: {}", head, tail);
        if head.wrapping_sub(tail) >= self.inner.capacity {
            // println!("index: {} now wrapping", index);
            // println!("head: {}, tail: {}", head, tail);
            return Err(elem);
        };

        unsafe {
            // Using the index, get the &UnsafeCell<MaybeUninit<T>> from the buffer
            // .get(); then takes gets the MaybeUninit<T> from the UnsafeCell
            if !self
                .inner
                .buffer
                .get_unchecked(index)
                .ready
                .load(Ordering::Acquire)
            {
                self.inner.buffer.get_unchecked(index).write(elem);
                self.inner
                    .head
                    .store(head.wrapping_add(1), Ordering::Release);
            } else {
                return Err(elem);
            }
        }
        Ok(())
    }
}

impl<T> Drop for Producer<T> {
    fn drop(&mut self) {
        println!("dropping");
        self.inner.closed.store(true, Ordering::Release);
    }
}

pub struct Consumer<T> {
    inner: Arc<RingBuffer<T>>,
}

impl<T> Consumer<T> {
    pub fn try_dequeue(&self) -> Option<T> {
        loop {
            let tail = self.inner.tail.load(Ordering::Relaxed);
            let head = self.inner.head.load(Ordering::Acquire);

            if tail >= head {
                if self.inner.closed.load(Ordering::Acquire) {
                    return None;
                } else {
                    continue;
                }
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
                let index = tail % self.inner.capacity;
                unsafe {
                    let cell = self.inner.buffer.get_unchecked(index);
                    while !cell.ready.load(Ordering::Acquire) {
                        std::hint::spin_loop();
                    }
                    return Some(cell.get())
                }
            }
        }
    }
}

pub struct RingBuffer<T> {
    buffer: Box<[Slot<T>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    closed: AtomicBool,
    capacity: usize,
}

unsafe impl<T: Send> Sync for RingBuffer<T> {}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let mut vec = Vec::with_capacity(capacity);

        for _ in 0..capacity {
            vec.push(Slot::new());
        }
        let buffer = vec.into_boxed_slice();
        RingBuffer {
            buffer,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            capacity,
        }
    }
}

pub fn channel<T>(capacity: usize) -> (Producer<T>, Arc<Consumer<T>>) {
    let buf = Arc::new(RingBuffer::new(capacity));
    let buf_2 = buf.clone();
    (Producer { inner: buf }, Arc::new(Consumer { inner: buf_2 }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_enqueue_dequeue() {
        let (prod, con) = channel::<Vec<u8>>(1_000);
        prod.try_enqueue([1, 2, 3, 5].to_vec()).unwrap();
        prod.try_enqueue([1, 2, 3, 5].to_vec()).unwrap();
        assert_eq!(Some([1, 2, 3, 5].to_vec()), con.try_dequeue());
        assert_eq!(Some([1, 2, 3, 5].to_vec()), con.try_dequeue());
    }

    #[test]
    fn test_drop_work() {
        let (prod, con) = channel::<Vec<u8>>(1_000);
        prod.try_enqueue([1, 2, 3, 5].to_vec()).unwrap();
        prod.try_enqueue([1, 2, 3, 5].to_vec()).unwrap();
        // Without this, the test hangs, this is the correct behaviour as we want the consumers to
        // loop while a producer still exists
        drop(prod);
        assert_eq!(Some([1, 2, 3, 5].to_vec()), con.try_dequeue());
        assert_eq!(Some([1, 2, 3, 5].to_vec()), con.try_dequeue());
        assert_eq!(None, con.try_dequeue());
    }
}
