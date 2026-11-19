//! A counting allocator for `overlay-bench`, behind the `bench-alloc` feature.
//!
//! The hook DLL has its own Rust allocator, so a counter in the benchmark process does not see the hook's allocations.
//! With this feature the DLL counts its own allocations and exports the counter; the benchmark worker reads it around
//! each operation. A release hook never enables this feature.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

/// Every allocation call of the hook DLL since the last reset.
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
/// The bytes requested by those allocations since the last reset.
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

/// Counts allocations and forwards them to the system allocator.
struct CountingAllocator;

// SAFETY: every function forwards the exact layout and pointer it was given to `System`, which upholds the allocator
// contract; the counter is a relaxed atomic and does not change the returned pointer.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: the caller passes a valid layout, as the `GlobalAlloc` contract requires.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout came from `alloc` or `realloc` above.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(
            new_size.saturating_sub(layout.size()) as u64,
            Ordering::Relaxed,
        );
        // SAFETY: the caller passes a pointer and layout from this allocator and a valid new size.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// The number of allocations since the last reset, for the benchmark worker. Exported under a stable C name.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_allocations() -> u64 {
    ALLOCATIONS.load(Ordering::Relaxed)
}

/// The bytes requested since the last reset, for the benchmark worker.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_alloc_bytes() -> u64 {
    ALLOC_BYTES.load(Ordering::Relaxed)
}

/// Resets the allocation counters, for the benchmark worker. Exported under a stable C name.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_reset() {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
}
