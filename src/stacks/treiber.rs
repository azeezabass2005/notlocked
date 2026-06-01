use std::sync::atomic::AtomicPtr;

use crate::stacks::node::Node;

pub struct TreiberStack<T> {
    head: AtomicPtr<Node<T>>,
}

impl<T> TreiberStack<T> {
    pub fn new() -> Self {
        todo!();
    }
   
    pub fn push(&self, value: T) {
        todo!();
    }

    pub fn pop(&self) -> Option<T> {
        todo!();
    }

    pub fn is_empty(&self) -> bool {
        todo!();
    }
}

#[test]
fn test_treiber_stack() {
    
}
