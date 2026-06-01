# notlocked

A Rust library implementing fundamental lock-free data structures.

The goal of this project is to move away from "black box" implementations. `notlocked` focuses on clear, auditable code where the memory ordering and atomic logic are as easy to read as the data they protect.

### Core Principles

* **Transparency:** Source code is documented to explain the "why" behind every atomic operation and memory barrier.
* **Auditability:** Designed for easy integration with formal verification tools like Loom and Kani.
* **Zero Magic:** No hidden abstractions—just raw atomics and the Rust memory model.

### Planned Structures

1. **Treiber Stack:** CAS-based LIFO stack.
2. **Michael-Scott Queue:** Unbounded FIFO queue.
3. **Bounded MPMC:** Fixed-capacity ring buffer.
4. **Elimination Stack:** High-contention LIFO stack.
