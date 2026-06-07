# Treiber Stack
## Introduction
The Treiber stack is a lock-free stack data structure that uses atomic operations to implement concurrent push and pop operations.

## Algorithm
The Treiber stack uses a linked list of nodes, where each node contains a value and a pointer to the next node. Pushing a value onto the stack involves creating a new node and atomically updating the head pointer to point to the new node. Popping a value from the stack involves atomically updating the head pointer to point to the next node and returning the value of the old head node.

## Implementation
### `new`
Initializes `head` to `ptr::null_mut()` which is a mutable null pointer representing an empty stack.

### `push`
- creates a new box with the given value and set the next to a null pointer
- create a loop that retries the push operation if the head pointer has changed
- loads the current head pointer
- set the next pointer of 

### `pop`
- create a loop that retries the pop if the head pointer has changed
- loads the current head pointer
- if the head pointer is null, return `None`
- set the new head pointer to the next of the current head pointer
- return the value of the old head node by using `unsafe` to convert the raw pointer to a box and then extracting the value

### `is_empty`
- loads the current head pointer
- returns `true` if the head pointer is null, `false` otherwise