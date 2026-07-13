use std::mem::ManuallyDrop;

pub struct Node<T> {
    pub value: ManuallyDrop<T>,
    pub next: *mut Node<T>,
}
