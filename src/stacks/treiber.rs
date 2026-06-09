use std::{ptr, sync::atomic::{AtomicPtr, Ordering}};

use crate::stacks::node::Node;

/// A Treiber stack implementation using atomic pointers.
/// 
/// See `docs/algorithms/treiber-stack.md` for a full walkthrough.
pub struct TreiberStack<T> {
    head: AtomicPtr<Node<T>>,
}

impl<T> TreiberStack<T> {
    pub fn new() -> Self {
        Self {
            // See docs/algorithms/treiber-stack.md#new
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }
   
    pub fn push(&self, value: T) {
        // See docs/algorithms/treiber-stack.md#push
        let mut new_box = Box::new(Node {
            value,
            next: ptr::null_mut(),
        });
        loop {
            
            let current_head = self.head.load(Ordering::Relaxed);
            new_box.next = current_head;
            let new_node = Box::into_raw(new_box);
            if self.head.compare_exchange(current_head, new_node, Ordering::Release, Ordering::Relaxed).is_ok() {
                return;
            }

            new_box = unsafe { Box::from_raw(new_node) };
        }
    }

    pub fn pop(&self) -> Option<T> {
        // See docs/algorithms/treiber-stack.md#pop
        loop {
            let current_head = self.head.load(Ordering::Acquire);
            if current_head.is_null() {
                return None;
            }
            let new_head = unsafe { (*current_head).next };
            if self.head.compare_exchange(current_head, new_head, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                let node =  unsafe { Box::from_raw(current_head) };
                return Some(node.value);
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
    use std::{sync::Arc, thread::{self, JoinHandle}};
    use super::*;

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
        
        for _ in 0..100 {
            let thread_stack = stack.clone();
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    thread_stack.push(i);
                }
            });
          all_handles.push(handle);  
        }

        for h in all_handles.into_iter() {
            h.join().unwrap();
        }
        for _ in 0..10000 {
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