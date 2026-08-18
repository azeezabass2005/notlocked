# Michael-Scott Queue
## Introduction
The Michael-Scott queue is a lock-free FIFO queue that uses atomic operations to implement concurrent enqueue and dequeue operations. It comes from the 1996 paper "Simple, Fast, and Practical Non-Blocking and Blocking Concurrent Queue Algorithms".

## Algorithm
The queue is a singly linked list of nodes, where each node contains a value and a pointer to the next node. The list always keeps a dummy node at the front, so `head` points to the dummy node and the first real value is in `head.next`. `tail` points to the last node, or one node behind it when another thread has linked a node but not yet advanced `tail`.

Enqueuing links a new node after the last node and then swings `tail` forward to it. Dequeuing reads the value out of `head.next`, swings `head` forward to that node, and frees the old dummy node. Because the tail can lag one node behind, both operations help move it forward when they find it pointing at a node that already has a successor.

## Implementation
### `new`
- allocates a dummy node with an uninitialized value and a null next pointer
- points both `head` and `tail` at that dummy node

### `enqueue`
- creates a new node with the given value and a null next pointer
- create a loop that retries until the node is linked
- loads `tail` and its next pointer, then re-reads `tail` to check it did not change
- if the next pointer is null, `tail` is the last node, so try to link the new node with a compare-and-swap
- if that succeeds, try to swing `tail` forward to the new node and return
- if the next pointer is not null, `tail` is lagging, so help it forward and retry

### `dequeue`
- create a loop that retries until a value is removed or the queue is found empty
- loads `head`, `tail`, and `head.next`, then re-reads `head` to check it did not change
- if `head` and `tail` are equal and the next pointer is null, the queue is empty, so return `None`
- if `head` and `tail` are equal but the next pointer is not null, `tail` is lagging, so help it forward and retry
- otherwise read the value out of `head.next` before moving `head`, because another dequeue could free that node afterwards
- swing `head` forward to `head.next` with a compare-and-swap
- if it succeeds, retire the old dummy node and return the value
- if it fails, forget the value that was read so it is not dropped, then retry

### `is_empty`
- loads `head` and its next pointer
- returns `true` if the next pointer is null, `false` otherwise

## Reclamation
Dequeued nodes are freed through the same epoch-based `Collector` the Treiber stack uses, rather than the modification counters the original paper attaches to each pointer. Pinning the epoch during an operation keeps a node alive while another thread still holds a pointer to it.
