use std::{mem::MaybeUninit, sync::atomic::AtomicPtr};

/// A single node in the Michael-Scott queue.
///
/// `next` is an `AtomicPtr` because enqueue updates it in place with a CAS.
/// `value` is a `MaybeUninit<T>` because the head is always a dummy node whose value slot never holds a live value.
pub struct Node<T> {
    pub value: MaybeUninit<T>,
    pub next: AtomicPtr<Node<T>>,
}
