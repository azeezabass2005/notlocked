use std::{
    mem::{self, MaybeUninit},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicPtr, Ordering},
    },
};

use crate::queues::node::Node;
use crate::reclamation::ebr::epoch::{Collector, Guard};

/// An unbounded lock-free FIFO queue (Michael & Scott, 1996).
///
/// See `docs/algorithms/michael-scott-queue.md` for a full walkthrough.
pub struct MsQueue<T> {
    head: AtomicPtr<Node<T>>,
    tail: AtomicPtr<Node<T>>,
    collector: Arc<Collector>,
}

/// Since `head` and `tail` are `AtomicPtr`, the compiler cannot automatically derive `Send` and `Sync` for `MsQueue<T>`.
///
/// So we implement them manually to promise that `MsQueue<T>` is `Send` and `Sync` when `T` is `Send`.
unsafe impl<T: Send> Send for MsQueue<T> {}
unsafe impl<T: Send> Sync for MsQueue<T> {}

/// Drops the queue, freeing every node still linked in the list.
///
/// The head node is the sentinel, its value slot holds no live value, so its allocation is freed without dropping a value.
/// Every node that follows it holds a value that was enqueued but never dequeued, so those values are explicitly dropped.
/// Nodes already handed to the `Collector` by `dequeue` are reclaimed by the epoch machinery, not here.
impl<T> Drop for MsQueue<T> {
    fn drop(&mut self) {
        let mut current = *self.head.get_mut();
        let mut is_sentinel = true;
        while !current.is_null() {
            let mut node = unsafe { Box::from_raw(current) };
            current = node.next.load(Ordering::Relaxed);
            if !is_sentinel {
                unsafe { node.value.assume_init_drop() };
            }
            is_sentinel = false;
        }
    }
}

impl<T> MsQueue<T> {
    pub fn new() -> Self {
        // See docs/algorithms/michael-scott-queue.md#new
        let sentinel = Box::into_raw(Box::new(Node {
            value: MaybeUninit::uninit(),
            next: AtomicPtr::new(ptr::null_mut()),
        }));
        Self {
            head: AtomicPtr::new(sentinel),
            tail: AtomicPtr::new(sentinel),
            collector: Collector::new(),
        }
    }

    pub fn enqueue(&self, value: T) {
        // See docs/algorithms/michael-scott-queue.md#enqueue
        let _guard = Guard::new(Arc::clone(&self.collector));
        let new_node = Box::into_raw(Box::new(Node {
            value: MaybeUninit::new(value),
            next: AtomicPtr::new(ptr::null_mut()),
        }));
        loop {
            let tail = self.tail.load(Ordering::Acquire);
            let next = unsafe { (*tail).next.load(Ordering::Acquire) };
            if tail != self.tail.load(Ordering::Acquire) {
                continue;
            }
            if next.is_null() {
                if unsafe {
                    (*tail)
                        .next
                        .compare_exchange(next, new_node, Ordering::Release, Ordering::Relaxed)
                        .is_ok()
                } {
                    let _ = self.tail.compare_exchange(
                        tail,
                        new_node,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    return;
                }
            } else {
                let _ =
                    self.tail
                        .compare_exchange(tail, next, Ordering::Release, Ordering::Relaxed);
            }
        }
    }

    pub fn dequeue(&self) -> Option<T> {
        // See docs/algorithms/michael-scott-queue.md#dequeue
        let guard = Guard::new(Arc::clone(&self.collector));
        loop {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);
            let next = unsafe { (*head).next.load(Ordering::Acquire) };
            if head != self.head.load(Ordering::Acquire) {
                continue;
            }
            if head == tail {
                if next.is_null() {
                    return None;
                }
                let _ =
                    self.tail
                        .compare_exchange(tail, next, Ordering::Release, Ordering::Relaxed);
            } else {
                let value = unsafe { ptr::read((*next).value.as_ptr()) };
                if self
                    .head
                    .compare_exchange(head, next, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
                {
                    guard.retire(head);
                    return Some(value);
                }
                // CAS failed: forget the read copy so the value is not dropped, then retry
                mem::forget(value);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        // See docs/algorithms/michael-scott-queue.md#is_empty
        let _guard = Guard::new(Arc::clone(&self.collector));
        let head = self.head.load(Ordering::Acquire);
        let next = unsafe { (*head).next.load(Ordering::Acquire) };
        next.is_null()
    }
}

impl<T> Default for MsQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::Arc,
        thread::{self, JoinHandle},
    };

    #[test]
    fn test_is_empty() {
        let queue: MsQueue<i32> = MsQueue::new();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_is_not_empty() {
        let queue: MsQueue<i32> = MsQueue::new();
        queue.enqueue(22);
        assert!(!queue.is_empty());
    }

    #[test]
    fn test_enqueue_dequeue() {
        let queue: MsQueue<i32> = MsQueue::new();
        queue.enqueue(22);
        assert_eq!(queue.dequeue(), Some(22));
    }

    #[test]
    fn test_dequeue_empty() {
        let queue: MsQueue<i32> = MsQueue::new();
        assert_eq!(queue.dequeue(), None);
        queue.enqueue(22);
        assert_eq!(queue.dequeue(), Some(22));
        assert_eq!(queue.dequeue(), None);
    }

    #[test]
    fn test_fifo() {
        let queue: MsQueue<i32> = MsQueue::new();
        queue.enqueue(22);
        queue.enqueue(6);
        queue.enqueue(7);
        queue.enqueue(14);
        assert_eq!(queue.dequeue(), Some(22));
        assert_eq!(queue.dequeue(), Some(6));
        assert_eq!(queue.dequeue(), Some(7));
        assert_eq!(queue.dequeue(), Some(14));
        assert_eq!(queue.dequeue(), None);
    }

    #[test]
    fn test_with_concurrent_enqueue() {
        let queue: Arc<MsQueue<i32>> = Arc::new(MsQueue::new());

        let mut all_handles: Vec<JoinHandle<()>> = Vec::new();
        for _ in 0..25 {
            let thread_queue = queue.clone();
            let handle = thread::spawn(move || {
                for i in 0..50 {
                    thread_queue.enqueue(i);
                }
            });
            all_handles.push(handle);
        }
        for h in all_handles {
            h.join().unwrap();
        }

        for _ in 0..1250 {
            assert!(queue.dequeue().is_some());
        }
        assert!(queue.dequeue().is_none());
    }

    #[test]
    fn test_with_concurrent_dequeue() {
        let queue: Arc<MsQueue<i32>> = Arc::new(MsQueue::new());
        for i in 0..10000 {
            queue.enqueue(i);
        }

        let mut all_handles: Vec<JoinHandle<()>> = Vec::new();
        for _ in 0..100 {
            let thread_queue = queue.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    thread_queue.dequeue();
                }
            });
            all_handles.push(handle);
        }
        for h in all_handles {
            h.join().unwrap();
        }

        assert!(queue.dequeue().is_none());
    }
}
