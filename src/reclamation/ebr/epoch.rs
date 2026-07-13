// See docs/concepts/epoch-based-reclamation.md

use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

// See docs/concepts/epoch-based-reclamation.md#globalepoch
struct GlobalEpoch {
    epoch: AtomicU64,
}

impl GlobalEpoch {
    fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
        }
    }

    fn get(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    fn try_increment(&self, current: u64) {
        let _ =
            self.epoch
                .compare_exchange(current, current + 1, Ordering::Release, Ordering::Acquire);
    }
}

// See docs/concepts/epoch-based-reclamation.md#retired
struct Retired {
    epoch: u64,
    ptr: *mut u8,
    drop_fn: unsafe fn(*mut u8),
}

unsafe impl Send for Retired {}

unsafe fn drop_data<T>(ptr: *mut u8) {
    unsafe { drop(Box::from_raw(ptr as *mut T)) }
}

// See docs/concepts/epoch-based-reclamation.md#threadlocalretired
struct ThreadLocalRetired {
    retired: Vec<Retired>,
    collector: Option<Weak<Collector>>,
}

impl ThreadLocalRetired {
    fn new() -> Self {
        Self {
            retired: Vec::new(),
            collector: None,
        }
    }

    fn init(&mut self, collector: &Arc<Collector>) {
        if self.collector.is_none() {
            self.collector = Some(Arc::downgrade(collector));
        }
    }
}

impl Drop for ThreadLocalRetired {
    fn drop(&mut self) {
        let Some(weak) = self.collector.take() else {
            return;
        };

        match weak.upgrade() {
            None => {
                for entry in self.retired.drain(..) {
                    unsafe { (entry.drop_fn)(entry.ptr) };
                }
            }
            Some(collector) => {
                let global = collector.global_epoch();
                self.retired.retain(|entry| {
                    if global >= entry.epoch + 2 {
                        unsafe { (entry.drop_fn)(entry.ptr) };
                        false
                    } else {
                        true
                    }
                });
                if !self.retired.is_empty() {
                    collector.orphans.lock().unwrap().append(&mut self.retired);
                }
            }
        }
    }
}

thread_local! {
    static RETIRED: RefCell<ThreadLocalRetired> = RefCell::new(ThreadLocalRetired::new());
}

// See docs/concepts/epoch-based-reclamation.md#localepoch
struct LocalEpoch {
    epoch: AtomicU64,
    is_pinned: AtomicBool,
}

impl LocalEpoch {
    fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            is_pinned: AtomicBool::new(false),
        }
    }

    fn sync(&self, global: u64) {
        self.epoch.store(global, Ordering::Release);
    }

    fn pin(&self) {
        self.is_pinned.store(true, Ordering::Release);
    }

    fn unpin(&self) {
        self.is_pinned.store(false, Ordering::Release);
    }

    fn get(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    fn is_pinned(&self) -> bool {
        self.is_pinned.load(Ordering::Acquire)
    }
}

// See docs/concepts/epoch-based-reclamation.md#collector
pub struct Collector {
    global: GlobalEpoch,
    registry: Mutex<Vec<Arc<LocalEpoch>>>,
    orphans: Mutex<Vec<Retired>>,
}

impl Collector {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            global: GlobalEpoch::new(),
            registry: Mutex::new(Vec::new()),
            orphans: Mutex::new(Vec::new()),
        })
    }

    fn global_epoch(&self) -> u64 {
        self.global.get()
    }

    fn register(&self) -> Arc<LocalEpoch> {
        let local = Arc::new(LocalEpoch::new());
        self.registry.lock().unwrap().push(Arc::clone(&local));
        local
    }

    fn unregister(&self, local: &Arc<LocalEpoch>) {
        self.registry
            .lock()
            .unwrap()
            .retain(|l| !Arc::ptr_eq(l, local));
    }

    fn try_advance(&self) {
        let global = self.global_epoch();
        let registry = self.registry.lock().unwrap();
        for entry in registry.iter() {
            if entry.is_pinned() && entry.get() != global {
                return;
            }
        }
        self.global.try_increment(global);
    }
}

// See docs/concepts/epoch-based-reclamation.md#guard
pub struct Guard {
    collector: Arc<Collector>,
    local: Arc<LocalEpoch>,
}

impl Guard {
    pub fn new(collector: Arc<Collector>) -> Self {
        RETIRED.with(|r| r.borrow_mut().init(&collector));

        let local = collector.register();
        let global = collector.global_epoch();
        local.sync(global);
        local.pin();
        collector.try_advance();
        collect(&collector);
        Self { collector, local }
    }

    pub fn retire<T>(&self, ptr: *mut T) {
        RETIRED.with(|r| {
            r.borrow_mut().retired.push(Retired {
                epoch: self.collector.global_epoch(),
                ptr: ptr as *mut u8,
                drop_fn: drop_data::<T>,
            });
        });
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.local.unpin();
        self.collector.try_advance();
        collect(&self.collector);
        self.collector.unregister(&self.local)
    }
}

// See docs/concepts/epoch-based-reclamation.md#collect
fn collect(collector: &Arc<Collector>) {
    let global = collector.global_epoch();

    RETIRED.with(|r| {
        let mut state = r.borrow_mut();
        state.retired.retain(|entry| {
            if global >= entry.epoch + 2 {
                unsafe { (entry.drop_fn)(entry.ptr) };
                false
            } else {
                true
            }
        });
    });

    let mut orphans = collector.orphans.lock().unwrap();
    orphans.retain(|entry| {
        if global >= entry.epoch + 2 {
            unsafe { (entry.drop_fn)(entry.ptr) };
            false
        } else {
            true
        }
    });
}
