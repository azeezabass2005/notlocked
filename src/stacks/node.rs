pub struct Node<T> {
    pub value: T,
    pub next: *mut Node<T>,
}