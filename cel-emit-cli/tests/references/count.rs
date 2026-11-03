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

/// Counts from zero.
pub fn start() {
    for count in [&CALLS, &BYTES] {
        count.store(0, Relaxed);
    }
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    ON.store(true, Relaxed);
}

/// Calls, bytes, and peak bytes live since `start`.
pub fn stop() -> (usize, usize, isize) {
    ON.store(false, Relaxed);
    (CALLS.load(Relaxed), BYTES.load(Relaxed), PEAK.load(Relaxed))
}

/// `f`, counted, its phase and counts on a line of stderr.
pub fn counted<T>(phase: &str, f: impl FnOnce() -> T) -> T {
    start();
    let out = framed(f);
    let (calls, bytes, peak) = stop();
    eprintln!("{phase} {calls} {bytes} {peak}");
    out
}

/// `f`'s value, called from a stack frame of its own, never as a tail call. On aarch64 a call
/// leaves the stack pointer where it was, so callgrind ends a function it entered only once its
/// caller's frame unwinds: this frame, right after the function returns.
#[inline(never)]
fn framed<T>(f: impl FnOnce() -> T) -> T {
    std::hint::black_box(f())
}
