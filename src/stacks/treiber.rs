use crate::reclamation::ebr::epoch::{Collector, Guard};
#[warn(missing_docs)]
use std::{
    mem::ManuallyDrop,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicPtr, Ordering},
    },
};

use crate::stacks::node::Node;

/// A Treiber stack implementation using atomic pointers.
///
/// See `docs/algorithms/treiber-stack.md` for a full walkthrough.
pub struct TreiberStack<T> {
    head: AtomicPtr<Node<T>>,
    collector: Arc<Collector>,
}

/// Since `head` is an `AtomicPtr`, the compiler cannot automatically derive `Send` and `Sync` for `TreiberStack<T>`.
///
/// This is because `AtomicPtr` is not `Send` or `Sync` for `T` that is not `Send`.
/// So we must implement `Send` and `Sync` manually to promise the compiler that `TreiberStack<T>` is Send and Sync when `T` is `Send`.
unsafe impl<T: Send> Send for TreiberStack<T> {}
unsafe impl<T: Send> Sync for TreiberStack<T> {}

/// Drops the stack, popping all elements.
///
/// If a non-empty stack is dropped, all the memory used by the stack will be leaked.
/// So it's important to explicitly ensure that all the memories being used by the stack is freed to avoid this.
impl<T> Drop for TreiberStack<T> {
    fn drop(&mut self) {
        let mut current = *self.head.get_mut();
        while !current.is_null() {
            let mut node = unsafe { Box::from_raw(current) };
            current = node.next;
            unsafe { ManuallyDrop::drop(&mut node.value) };
        }
    }
}

impl<T> TreiberStack<T> {
    pub fn new() -> Self {
        Self {
            // See docs/algorithms/treiber-stack.md#new
            head: AtomicPtr::new(ptr::null_mut()),
            collector: Collector::new(),
        }
    }

    pub fn push(&self, value: T) {
        // See docs/algorithms/treiber-stack.md#push
        let mut new_box = Box::new(Node {
            value: ManuallyDrop::new(value),
            next: ptr::null_mut(),
        });
        loop {
            let current_head = self.head.load(Ordering::Relaxed);
            new_box.next = current_head;
            let new_node = Box::into_raw(new_box);
            if self
                .head
                .compare_exchange(current_head, new_node, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
            // CAS failed: recover the box and retry

            new_box = unsafe { Box::from_raw(new_node) };
        }
    }

    // Multithreading
    // Mutex - GuardLock
    //
    // Let my_pdf;

    pub fn pop(&self) -> Option<T> {
        // See docs/algorithms/treiber-stack.md#pop
        let guard = Guard::new(Arc::clone(&self.collector));
        loop {
            let current_head = self.head.load(Ordering::Acquire);
            if current_head.is_null() {
                return None;
            }
            let new_head = unsafe { (*current_head).next };
            if self
                .head
                .compare_exchange(current_head, new_head, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                let value = unsafe { ManuallyDrop::into_inner(ptr::read(&(*current_head).value)) };
                guard.retire(current_head);
                return Some(value);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        // See docs/algorithms/treiber-stack.md#is_empty
        let current_head = self.head.load(Ordering::Relaxed);
        current_head.is_null()
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
        let stack: TreiberStack<i32> = TreiberStack::new();
        assert!(stack.is_empty());
    }

    #[test]
    fn test_is_not_empty() {
        let stack: TreiberStack<i32> = TreiberStack::new();
        stack.push(22);
        assert!(!stack.is_empty());
    }

    #[test]
    fn test_push_pop() {
        let stack: TreiberStack<i32> = TreiberStack::new();
        stack.push(22);
        assert_eq!(stack.pop(), Some(22));
    }

    #[test]
    fn test_pop() {
        let stack: TreiberStack<i32> = TreiberStack::new();
        stack.push(22);
        assert_eq!(stack.pop(), Some(22));
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn test_lifo() {
        let stack: TreiberStack<i32> = TreiberStack::new();
        stack.push(22);
        stack.push(06);
        stack.push(07);
        stack.push(14);
        assert_eq!(stack.pop(), Some(14));
        assert_eq!(stack.pop(), Some(07));
        assert_eq!(stack.pop(), Some(06));
        assert_eq!(stack.pop(), Some(22));
    }

    #[test]
    fn test_with_concurrent_push() {
        let stack: Arc<TreiberStack<i32>> = Arc::new(TreiberStack::new());

        let mut all_handles: Vec<JoinHandle<()>> = Vec::new();

        for _ in 0..25 {
            let thread_stack = stack.clone();
            let handle = thread::spawn(move || {
                for i in 0..50 {
                    thread_stack.push(i);
                }
            });
            all_handles.push(handle);
        }

        for h in all_handles.into_iter() {
            h.join().unwrap();
        }
        for _ in 0..1250 {
            assert!(stack.pop().is_some());
        }
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_with_concurrent_pop() {
        let stack: Arc<TreiberStack<i32>> = Arc::new(TreiberStack::new());

        for i in 0..10000 {
            stack.push(i);
        }

        let mut all_handles: Vec<JoinHandle<()>> = Vec::new();

        for _ in 0..100 {
            let thread_stack = stack.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    thread_stack.pop();
                }
            });
            all_handles.push(handle);
        }

        for h in all_handles.into_iter() {
            h.join().unwrap();
        }
        assert!(stack.pop().is_none());
    }
}

// Title: How do we ace it? [June 12 2026 - Fola]
// Oh, they do call it democracy.
// Celebrating democracy in a nation without security is actually brazy.
// Some say the talents aren't world-class, that they are lazy.
// But that's not the case if we trace it.
// Future talents are held captive by some people that are crazy.
// Most talents are long gone cos the future of the country is hazy.
// The people can barely survive, let alone live greatly.
// And nothing guarantees that your next candidate is not going to lace it.
// So how do we bring back hope and make things rosy?
// How do you and I make this failing country ace it?
