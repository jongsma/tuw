// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview hand-off between the UI and audio threads: real-time safety and the rings.
//!
//! This test is its own binary because it installs a counting global allocator. The allocator
//! only counts on a thread while that thread's flag is set, and the tests set it only around
//! `Oscillator::render`, so the counts are deterministic even with tests running in parallel.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::f64::consts::PI;
use std::sync::{Arc, Weak};

use tuw_core::Wavetable;
use tuw_core::preview::{
    Oscillator, PLAY_FADE_SECONDS, PreparedTable, Preparer, PreviewControl, PreviewError,
    SWAP_FADE_SECONDS, preview,
};

/// Counts allocations, deallocations and reallocations on the threads that ask for it.
struct Counting;

thread_local! {
    // `const` initialisation: no lazy set-up and no destructor, so reading these from inside
    // the allocator never allocates itself.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static DEALLOCS: Cell<usize> = const { Cell::new(0) };
    static REALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn count(counter: &'static std::thread::LocalKey<Cell<usize>>) {
    // `try_with`: the allocator may run while the thread's locals are being torn down.
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = counter.try_with(|c| c.set(c.get() + 1));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(&ALLOCS);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(&ALLOCS);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(&DEALLOCS);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(&REALLOCS);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// What the allocator saw on this thread during one call.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    allocs: usize,
    deallocs: usize,
    reallocs: usize,
}

/// Renders one block of `out.len()` samples, counting allocator calls during the render only.
fn render_counted(osc: &mut Oscillator, out: &mut [f32]) -> Counts {
    ALLOCS.set(0);
    DEALLOCS.set(0);
    REALLOCS.set(0);
    COUNTING.set(true);
    osc.render(out);
    COUNTING.set(false);
    Counts {
        allocs: ALLOCS.get(),
        deallocs: DEALLOCS.get(),
        reallocs: REALLOCS.get(),
    }
}

/// Renders `blocks` blocks of `BLOCK` samples and asserts the allocator was never called.
fn render_blocks(osc: &mut Oscillator, blocks: usize, what: &str) {
    let mut out = [0.0f32; BLOCK];
    for block in 0..blocks {
        let counts = render_counted(osc, &mut out);
        assert_eq!(counts, Counts::default(), "{what}, block {block}");
    }
}

const FS: f32 = 48_000.0;
const BLOCK: usize = 256;

/// One period of a sine of `harmonic` cycles, `n` samples.
fn sine(n: usize, harmonic: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (2.0 * PI * (harmonic * i) as f64 / n as f64).sin() as f32)
        .collect()
}

/// A 256 × 4 × 1 table with a sine of `harmonic` cycles in every frame.
fn table(harmonic: usize) -> Wavetable {
    let frame = sine(256, harmonic);
    let mut t = Wavetable::silent(256, 4, 1).unwrap();
    for y in 0..4 {
        t = t.with_frame(y, 0, &frame).unwrap();
    }
    t
}

/// Sends `table`, rendering (and checking) blocks while the table ring is full.
fn send(control: &mut PreviewControl, osc: &mut Oscillator, table: Arc<PreparedTable>) {
    let mut table = table;
    for _ in 0..1000 {
        match control.set_table(table) {
            Ok(()) => return,
            Err(back) => {
                table = back;
                render_blocks(osc, 1, "while the table ring is full");
            }
        }
    }
    panic!("the table ring never had room");
}

/// Renders and collects until every table in `tables` is freed; returns how many came back.
fn collect_all(
    control: &mut PreviewControl,
    osc: &mut Oscillator,
    tables: &[Weak<PreparedTable>],
) -> usize {
    let mut collected = 0;
    for _ in 0..1000 {
        collected += control.collect();
        if tables.iter().all(|t| t.strong_count() == 0) {
            return collected;
        }
        render_blocks(osc, 1, "while collecting");
    }
    panic!("not every table came back");
}

#[test]
fn render_allocates_nothing() {
    // The UI prepares A, sends it and plays.
    let (mut control, mut osc) = preview(FS, 4).unwrap();
    let mut preparer = Preparer::new();
    let source_a = table(1);
    let a = preparer.prepare(&source_a).unwrap();
    let weak_a = Arc::downgrade(&a);
    control.set_table(a).unwrap();
    control.set_frequency(220.0).unwrap();
    control.play().unwrap();
    render_blocks(&mut osc, 4, "playing A");
    assert!(osc.voice().last_weights().w_first > 0.0, "A sounds");

    // The UI prepares B with one frame changed, so B shares the other frames with A.
    let source_b = source_a.with_frame(1, 0, &sine(256, 2)).unwrap();
    let b = preparer.prepare(&source_b).unwrap();
    assert_eq!(
        preparer.frames_built_last(),
        1,
        "B reuses A's unchanged frames"
    );
    {
        let a = weak_a.upgrade().unwrap();
        assert!(Arc::ptr_eq(a.frame(0, 0).unwrap(), b.frame(0, 0).unwrap()));
    }
    let weak_b = Arc::downgrade(&b);
    control.set_table(b).unwrap();
    // The UI drops every reference it holds to A and B and clears the preparer's cache: the
    // oscillator now owns the last references.
    drop(source_a);
    drop(source_b);
    preparer = Preparer::new();
    assert_eq!(weak_a.strong_count(), 1, "only the voice holds A");
    assert_eq!(weak_b.strong_count(), 1, "only the table ring holds B");

    // The audio thread renders through the whole swap, freeing nothing.
    let swap_blocks = (SWAP_FADE_SECONDS * FS) as usize / BLOCK + 2;
    render_blocks(&mut osc, swap_blocks, "swapping A to B");
    assert!(!osc.voice().is_swapping(), "the swap finished");
    assert_eq!(
        weak_a.strong_count(),
        1,
        "A is on the return ring, not freed"
    );

    // A comes back to the UI thread, which frees it.
    assert_eq!(collect_all(&mut control, &mut osc, &[weak_a]), 1);
    assert_eq!(weak_b.strong_count(), 1, "B plays on");
    drop(preparer);

    // The return ring full: no collecting while swapping more tables than it holds.
    let (mut control, mut osc) = preview(FS, 2).unwrap();
    let mut preparer = Preparer::new();
    control.set_frequency(220.0).unwrap();
    control.play().unwrap();
    let mut weak = Vec::new();
    let mut most_held = 0;
    for harmonic in 1..=6 {
        let t = preparer.prepare(&table(harmonic)).unwrap();
        weak.push(Arc::downgrade(&t));
        send(&mut control, &mut osc, t);
        render_blocks(&mut osc, swap_blocks, "swapping with no collect");
        most_held = most_held.max(osc.held());
    }
    drop(preparer);
    render_blocks(&mut osc, swap_blocks, "stuck with every slot full");
    assert_eq!(osc.held(), 2, "both holding slots are in use");
    assert!(most_held <= 2);
    // Five tables were replaced (the sixth waits or plays); none was freed by the render.
    assert!(weak.iter().all(|t| t.strong_count() == 1));
    let replaced = &weak[..5];
    assert_eq!(collect_all(&mut control, &mut osc, replaced), 5);
    assert_eq!(weak[5].strong_count(), 1, "the last table plays");

    // A replace while stopped retires the old table at once; it still comes back.
    let (mut control, mut osc) = preview(FS, 2).unwrap();
    let mut preparer = Preparer::new();
    let a = preparer.prepare(&table(1)).unwrap();
    let weak_a = Arc::downgrade(&a);
    control.set_table(a).unwrap();
    control.set_frequency(220.0).unwrap();
    render_blocks(&mut osc, 1, "stopped with A");
    let b = preparer.prepare(&table(3)).unwrap();
    let weak_b = Arc::downgrade(&b);
    control.set_table(b).unwrap();
    drop(preparer);
    render_blocks(&mut osc, 1, "replacing A while stopped");
    assert!(!osc.voice().is_swapping());
    assert_eq!(collect_all(&mut control, &mut osc, &[weak_a]), 1);

    // Dropping the oscillator (off the audio thread) frees the table it plays.
    drop(osc);
    assert_eq!(weak_b.strong_count(), 0);
}

#[test]
fn busy_when_full() {
    let mut preparer = Preparer::new();
    let tables: Vec<_> = (1..=3)
        .map(|h| preparer.prepare(&table(h)).unwrap())
        .collect();

    // Capacity 2, nothing rendered: a third control is refused, a third table given back.
    let (mut control, _osc) = preview(FS, 2).unwrap();
    control.set_frequency(220.0).unwrap();
    control.set_playhead(0.0, 0.0).unwrap();
    assert_eq!(control.play(), Err(PreviewError::Busy), "a third control");
    control.set_table(Arc::clone(&tables[0])).unwrap();
    control.set_table(Arc::clone(&tables[1])).unwrap();
    let back = control.set_table(Arc::clone(&tables[2])).unwrap_err();
    assert!(
        Arc::ptr_eq(&back, &tables[2]),
        "a third table is given back"
    );
    drop(back);

    // A stop still arrives while a table waits for a swap to finish.
    let (mut control, mut osc) = preview(FS, 2).unwrap();
    control.set_table(Arc::clone(&tables[0])).unwrap();
    control.set_frequency(220.0).unwrap();
    control.play().unwrap();
    let play_fade = (PLAY_FADE_SECONDS * FS) as usize;
    let swap = (SWAP_FADE_SECONDS * FS) as usize;
    let mut sounding = vec![0.0f32; 2 * play_fade];
    osc.render(&mut sounding);
    assert!(sounding.iter().any(|s| s.abs() > 0.5), "sounding");
    control.set_table(Arc::clone(&tables[1])).unwrap();
    osc.render(&mut [0.0f32]); // starts the swap to table 1
    assert!(osc.voice().is_swapping());
    control.set_table(Arc::clone(&tables[2])).unwrap(); // waits on the ring
    control.stop().unwrap();
    let mut tail = vec![1.0f32; play_fade + 20];
    assert!(1 + tail.len() < swap, "the stop fade ends before the swap");
    osc.render(&mut tail);
    assert!(osc.voice().is_swapping(), "table 2 still waits");
    assert_eq!(
        Arc::strong_count(&tables[2]),
        2,
        "table 2 is still on the ring"
    );
    // After the stop fade the output is exact zeros.
    assert!(tail[play_fade..].iter().all(|s| s.to_bits() == 0));
}

#[test]
fn controls_arrive() {
    let (mut control, mut osc) = preview(FS, 8).unwrap();
    let t = Preparer::new().prepare(&table(1)).unwrap();
    control.set_table(t).unwrap();
    control.set_frequency(440.0).unwrap();
    control.set_playhead(2.5, 0.0).unwrap();
    let mut out = vec![1.0f32; 4800];
    osc.render(&mut out);
    // Stopped: exact zeros, and the silent voice jumped to the note and playhead.
    assert!(out.iter().all(|s| s.to_bits() == 0));
    assert_eq!(osc.voice().frequency().to_bits(), 440f32.to_bits());
    assert_eq!(osc.voice().playhead().0.to_bits(), 2.5f32.to_bits());

    control.play().unwrap();
    osc.render(&mut out);
    // A full-scale sine at 440 Hz: 0.1 s holds 44 periods, so the peak is near 1. The 0.05
    // margin covers the play fade-in (the peak comes after it) and the band-limited read.
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!((peak - 1.0).abs() < 0.05, "peak {peak}");

    control.stop().unwrap();
    osc.render(&mut out);
    let play_fade = (PLAY_FADE_SECONDS * FS) as usize;
    assert!(out[play_fade..].iter().all(|s| s.to_bits() == 0));
}
