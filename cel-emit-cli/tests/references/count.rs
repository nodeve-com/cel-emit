//! Counts the allocations between `start` and `stop`, and the most bytes live at once: each
//! measured program's global allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering::Relaxed};

static ON: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

pub struct Counting;

fn grow(by: usize) {
    CALLS.fetch_add(1, Relaxed);
    BYTES.fetch_add(by, Relaxed);
    let live = LIVE.fetch_add(by as isize, Relaxed) + by as isize;
    PEAK.fetch_max(live, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ON.load(Relaxed) {
            grow(layout.size());
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ON.load(Relaxed) {
            LIVE.fetch_sub(layout.size() as isize, Relaxed);
        }
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ON.load(Relaxed) {
            LIVE.fetch_sub(layout.size() as isize, Relaxed);
            grow(size);
        }
        System.realloc(ptr, layout, size)
    }
}

pub fn start() {
    ON.store(true, Relaxed);
}

/// Calls, bytes, and peak bytes live since `start`.
pub fn stop() -> (usize, usize, isize) {
    ON.store(false, Relaxed);
    (CALLS.load(Relaxed), BYTES.load(Relaxed), PEAK.load(Relaxed))
}
