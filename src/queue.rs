use std::{
    fmt::Debug,
    sync::atomic::{
        AtomicPtr,
        Ordering::{Acquire, Release},
    },
};

#[derive(Debug)]
struct LockFreeNode<T: Clone> {
    value: Option<T>,
    next: Option<Box<LockFreeNode<T>>>,
}

#[derive(Debug)]
pub struct LockFreeQueue<T: Clone> {
    head: AtomicPtr<LockFreeNode<T>>,
    tail: AtomicPtr<LockFreeNode<T>>,
}

impl<T: Clone + Debug> LockFreeQueue<T> {
    pub fn new() -> Self {
        let sentinel = Box::new(LockFreeNode {
            value: None,
            next: None,
        });

        let sentinel_ptr: *mut _ = Box::into_raw(sentinel);

        LockFreeQueue {
            head: AtomicPtr::new(sentinel_ptr),
            tail: AtomicPtr::new(sentinel_ptr),
        }
    }

    #[inline]
    pub fn enqueue(&self, value: T) {
        let value = Some(value);
        let new_node = Box::new(LockFreeNode { value, next: None });

        let new_node_ptr: *mut _ = Box::into_raw(new_node);

        loop {
            let tail_ptr = self.tail.load(Acquire);

            unsafe {
                if (*tail_ptr).next.is_none() {
                    (*tail_ptr).next = Some(Box::from_raw(new_node_ptr));
                    if self
                        .tail
                        .compare_exchange(tail_ptr, new_node_ptr, Release, Acquire)
                        .is_ok()
                    {
                        break;
                    }
                }
            }
        }
    }

    #[inline]
    pub fn dequeue(&self) -> Option<T> {
        loop {
            let head_ptr = self.head.load(Acquire);
    
            unsafe {
                let next = (*head_ptr).next.as_ref()?;
                // This is ugly as sin
                let next_ptr = next.as_ref() as *const _ as *mut _;
    
                if self
                    .head
                    .compare_exchange(head_ptr, next_ptr, Release, Acquire)
                    .is_ok()
                {
                    return next.value.clone();
                }
            }
        }
    }
    
}

impl<T: Clone + Debug> Default for LockFreeQueue<T> {
    fn default() -> Self {
        LockFreeQueue::new()
    }
}

unsafe impl<T: Clone + Debug> Send for LockFreeQueue<T> {}
