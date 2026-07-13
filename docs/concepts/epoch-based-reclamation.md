# Epoch-Based Reclamation

## Introduction
Epoch-based reclamation (EBR) is a deferred memory reclamation scheme for lock-free data structures. Instead of freeing a removed node immediately — which would be unsafe if another thread still holds a pointer to it — EBR stamps each removed node with the current epoch and only frees it once the epoch has advanced far enough to guarantee no thread can still be reading it.

## Algorithm
A global epoch counter starts at zero and only moves forward. Each thread that wants to read shared data creates a `Guard`, which pins the thread to the current global epoch. A node removed from the data structure is *retired* — tagged with the global epoch at the time of removal — and placed in a per-thread retired list. A retired node is safe to free once the global epoch has advanced by at least two past the node's retirement epoch, because by then every thread that could have held a pointer to it has moved on.

The epoch advances when all currently pinned threads are observing the current global epoch. At that point any thread may increment it via a compare-and-swap, so only one thread wins the race and the rest move on safely.

## Implementation

### `GlobalEpoch`
Wraps an `AtomicU64` that only ever increases. `get` reads it with `Acquire`. `try_increment` uses a compare-and-swap so that if two threads both attempt to advance at the same time only one succeeds.

### `LocalEpoch`
Each `Guard` owns a `LocalEpoch` that records which epoch that thread is currently observing and whether it is actively pinned. Other threads read both fields during `try_advance` to decide whether it is safe to increment the global epoch.

### `Retired`
A type-erased record of a pointer that has been removed from the data structure but not yet freed. Stores the retirement epoch, the raw pointer, and a monomorphised drop function (`drop_data`) so the correct destructor is called regardless of the original type.

### `ThreadLocalRetired`
The per-thread list of retired nodes. Holds a `Weak<Collector>` reference so that when a thread exits its destructor can attempt one last collection pass and transfer any remaining nodes to the collector's orphan list rather than leaking them.

### `Collector`
The single shared coordinator. Owns the `GlobalEpoch`, a registry of every active `LocalEpoch`, and an orphan list for nodes left behind by exited threads. `register` adds a new `LocalEpoch` to the registry; `unregister` removes it. `try_advance` scans the registry and increments the global epoch only if every pinned thread is already at the current epoch.

### `Guard`
The public handle used by data structures. Creating a `Guard` registers a `LocalEpoch`, syncs it to the current global epoch, and pins the thread. Dropping a `Guard` unpins, attempts to advance the epoch, runs a collection pass, and unregisters. `retire` stamps a raw pointer with the **current global epoch** (not the guard's creation epoch) and appends it to the thread-local retired list.

### `collect`
Walks the calling thread's retired list and the collector's orphan list, freeing every entry whose retirement epoch satisfies `global_epoch >= retirement_epoch + 2`. The reason for the orphans is that when a thread exits there is no guarantee that its `ThreadLocalRetired` list will be freed, so it is transferred to the collector's orphan list rather than being leaked.
