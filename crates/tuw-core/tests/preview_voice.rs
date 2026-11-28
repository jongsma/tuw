// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview voice over time: smoothing, play/stop fades and table swaps.

use std::f64::consts::PI;
use std::sync::Arc;

use tuw_core::Wavetable;
use tuw_core::preview::{
    PLAY_FADE_SECONDS, PreparedTable, Preparer, SMOOTHING_SECONDS, SWAP_FADE_SECONDS, Voice,
};

/// The low note of the click tests: its steady largest step is about `2π f / fs`, small enough
/// that a click stands out clearly.
const LOW_NOTE: f32 = 110.0;

/// One period of a sine of amplitude `amplitude`, `n` samples.
fn sine(n: usize, amplitude: f64) -> Vec<f32> {
    (0..n)
        .map(|i| (amplitude * (2.0 * PI * i as f64 / n as f64).sin()) as f32)
        .collect()
}

/// A `y_size × 1` table with the same frame everywhere.
fn row(frame: &[f32], y_size: u32) -> Arc<PreparedTable> {
    let mut t = Wavetable::silent(frame.len() as u32, y_size, 1).unwrap();
    for y in 0..y_size {
        t = t.with_frame(y, 0, frame).unwrap();
    }
    Preparer::new().prepare(&t).unwrap()
}

fn samples(seconds: f32, fs: f32) -> usize {
    (f64::from(seconds) * f64::from(fs)).round() as usize
}

fn render(v: &mut Voice, n: usize) -> Vec<f32> {
    let mut out = vec![0.0; n];
    v.render(&mut out);
    out
}

fn one(v: &mut Voice) -> f32 {
    let mut s = [0.0f32];
    v.render(&mut s);
    s[0]
}

/// The largest step between neighbouring samples, `before` counting as the sample ahead of
/// `out[0]`.
fn largest_step(before: f32, out: &[f32]) -> f32 {
    let mut last = before;
    let mut largest = 0.0f32;
    for &s in out {
        largest = largest.max((s - last).abs());
        last = s;
    }
    largest
}

/// Renders single samples until one is above 0.99 in magnitude (a sine peak) and returns it,
/// so the change that follows happens where a jump would click hardest.
fn to_peak(v: &mut Voice) -> f32 {
    for _ in 0..10_000 {
        let s = one(v);
        if s.abs() > 0.99 {
            return s;
        }
    }
    panic!("no peak within 10 000 samples");
}

#[test]
fn playhead_and_note_ramp() {
    // 0.02 s × 51 200 Hz = 1024 samples: the ramp steps 3/1024 and 220/1024 are dyadic, so
    // every value on the ramp is exact in f32 and the bound below needs no room for rounding
    // beyond its 1e-6. (At 48 kHz the frequency steps would carry up to one ulp of 440,
    // 3e-5, of rounding.)
    let fs = 51_200.0f32;
    let len = samples(SMOOTHING_SECONDS, fs);
    assert_eq!(len, 1024);
    let table = row(&sine(256, 1.0), 4);
    let mut v = Voice::new(fs).unwrap();
    assert!(v.set_table(table).is_none());
    v.set_playhead(0.0, 0.0);
    v.set_frequency(220.0);
    v.play();
    render(&mut v, 2 * samples(PLAY_FADE_SECONDS, fs)); // sounding at full gain

    v.set_playhead(3.0, 0.0);
    v.set_frequency(440.0);
    let ramp = SMOOTHING_SECONDS * fs;
    let max_dy = 3.0 / ramp + 1e-6;
    let max_df = 220.0 / ramp + 1e-6;
    let (mut y, z) = v.playhead();
    let mut f = v.frequency();
    assert_eq!(y.to_bits(), 0f32.to_bits(), "the ramp starts where it was");
    assert_eq!(f.to_bits(), 220f32.to_bits());
    for i in 0..len {
        if i == len / 2 {
            // Re-sending the targets mid-ramp must not restart the ramps.
            v.set_playhead(3.0, 0.0);
            v.set_frequency(440.0);
        }
        if i == len - 1 {
            assert_ne!(v.playhead().0.to_bits(), 3f32.to_bits(), "arrived early");
            assert_ne!(v.frequency().to_bits(), 440f32.to_bits(), "arrived early");
        }
        one(&mut v);
        let (ny, nz) = v.playhead();
        let nf = v.frequency();
        assert!((ny - y).abs() <= max_dy, "sample {i}: y {y} -> {ny}");
        assert!((nf - f).abs() <= max_df, "sample {i}: f {f} -> {nf}");
        assert_eq!(nz.to_bits(), z.to_bits(), "z did not change");
        (y, f) = (ny, nf);
    }
    // Arrived on sample `len` exactly, and stays.
    for _ in 0..100 {
        assert_eq!(v.playhead().0.to_bits(), 3f32.to_bits());
        assert_eq!(v.frequency().to_bits(), 440f32.to_bits());
        one(&mut v);
    }
}

#[test]
fn ramps_only_while_sounding() {
    let fs = 48_000.0f32;
    let table = row(&sine(256, 1.0), 4);
    let mut v = Voice::new(fs).unwrap();
    assert!(v.set_table(table).is_none());

    // Never played: both setters jump.
    v.set_playhead(2.5, 0.5);
    v.set_frequency(330.0);
    assert_eq!(v.playhead().0.to_bits(), 2.5f32.to_bits());
    assert_eq!(v.playhead().1.to_bits(), 0.5f32.to_bits());
    assert_eq!(v.frequency().to_bits(), 330f32.to_bits());

    // Sounding: a ramp, so the target is not reached at once.
    v.play();
    render(&mut v, 1024);
    v.set_frequency(440.0);
    assert!(
        (v.frequency() - 330.0).abs() < 1e-3,
        "jumped while sounding"
    );

    // During the stop fade the voice still sounds, so it still ramps.
    render(&mut v, samples(SMOOTHING_SECONDS, fs));
    v.stop();
    one(&mut v);
    v.set_playhead(1.0, 0.0);
    assert!(
        (v.playhead().0 - 2.5).abs() < 1e-3,
        "jumped during the stop fade"
    );

    // Faded out: both jump again, also mid-ramp.
    render(&mut v, samples(PLAY_FADE_SECONDS, fs));
    v.set_playhead(0.25, 0.0);
    v.set_frequency(550.0);
    assert_eq!(v.playhead().0.to_bits(), 0.25f32.to_bits());
    assert_eq!(v.frequency().to_bits(), 550f32.to_bits());
}

/// Plays A at y = 3 on `v` up to a sine peak; returns (steady largest step, the peak sample).
fn play_to_peak(v: &mut Voice, a: &Arc<PreparedTable>) -> (f32, f32) {
    assert!(v.set_table(Arc::clone(a)).is_none());
    v.set_playhead(3.0, 0.0);
    v.set_frequency(LOW_NOTE);
    v.play();
    render(v, 1024);
    let warm = one(v);
    let steady = render(v, 2048);
    (largest_step(warm, &steady), to_peak(v))
}

/// Plays A (a 4 × 1 grid of sines) at y = 3, swaps to B (a 2 × 1 grid of inverted sines, where
/// y clamps to 1) at a peak and returns (steady largest step of A, largest step from the swap
/// on). A twin voice made by `make` plays A on without the swap, so B's polarity can be checked
/// sample by sample.
fn swap_steps(make: impl Fn() -> Voice, fs: f32) -> (f32, f32) {
    let a = row(&sine(2048, 1.0), 4);
    let b = row(&sine(2048, -1.0), 2);
    let (mut v, mut twin) = (make(), make());
    let (steady_step, before) = play_to_peak(&mut v, &a);
    let (_, twin_before) = play_to_peak(&mut twin, &a);
    assert_eq!(
        before.to_bits(),
        twin_before.to_bits(),
        "the twin is in step"
    );
    assert!(v.set_table(Arc::clone(&b)).is_none());
    let fade = samples(SWAP_FADE_SECONDS, fs);
    let after = render(&mut v, fade + 2048);
    let unswapped = render(&mut twin, fade + 2048);
    // The swap did happen: A came back, and after the fade B sounds as −A. B's levels are the
    // exact negation of A's (the FFT, the scaling and the kernel sums are symmetric in sign),
    // and both grids put the playhead on one identical frame, so 1e-6 is only slack for f32
    // rounding.
    let retired = v.take_retired().expect("A is retired after the swap");
    assert!(Arc::ptr_eq(&retired, &a));
    let mut checked = 0;
    for (i, (&s, &r)) in after.iter().zip(&unswapped).enumerate().skip(fade) {
        assert!((s + r).abs() <= 1e-6, "sample {i}: {s} is not −{r}");
        if r.abs() > 0.5 {
            assert!(s.signum() == -r.signum(), "sample {i}: polarity");
            checked += 1;
        }
    }
    assert!(checked > 100, "too few loud samples to check polarity");
    (steady_step, largest_step(before, &after))
}

#[test]
fn no_click_on_swap() {
    let fs = 48_000.0f32;
    let (steady, swap) = swap_steps(|| Voice::new(fs).unwrap(), fs);
    // The steady largest step of a unit sine is about 2π f / fs.
    let expected = 2.0 * std::f32::consts::PI * LOW_NOTE / fs;
    assert!(
        (steady - expected).abs() < 0.05 * expected,
        "steady {steady}"
    );
    // A linear crossfade from s to −s over 480 samples adds at most 2 / 480 per sample, at
    // right angles to the sine's own slope: about 1.04 × steady. 1.5 × leaves room and still
    // catches any jump.
    let bound = 1.5 * steady;
    assert!(
        swap <= bound,
        "swap step {swap} > {bound} (steady {steady})"
    );

    // Control: the same swap without a fade clicks (a jump of about 2 at the peak).
    let (steady, swap) = swap_steps(
        || Voice::with_times(fs, SMOOTHING_SECONDS, 0.0, PLAY_FADE_SECONDS).unwrap(),
        fs,
    );
    assert!(
        swap > 1.5 * steady,
        "the control did not click: {swap} (steady {steady})"
    );
}

/// Plays a unit sine at the low note, starting near a peak, stops at a peak, and returns
/// (steady largest step, largest step around play, largest step around stop, samples after
/// the stop).
fn play_stop_steps(mut v: Voice, fs: f32) -> (f32, f32, f32, Vec<f32>) {
    v.set_table(row(&sine(2048, 1.0), 1));
    v.set_frequency(LOW_NOTE);
    // The phase runs while stopped: a quarter period of silence starts the note at a peak.
    let quarter = (f64::from(fs) / f64::from(LOW_NOTE) / 4.0).round() as usize;
    let silence = render(&mut v, quarter);
    assert!(silence.iter().all(|s| s.to_bits() == 0));
    v.play();
    let start = render(&mut v, 1024);
    let play_step = largest_step(0.0, &start);
    let steady = render(&mut v, 2048);
    let steady_step = largest_step(start[start.len() - 1], &steady);
    let before = to_peak(&mut v);
    v.stop();
    let after = render(&mut v, 2048);
    let stop_step = largest_step(before, &after);
    (steady_step, play_step, stop_step, after)
}

#[test]
fn no_click_on_play_stop() {
    let fs = 48_000.0f32;
    let (steady, play, stop, after) = play_stop_steps(Voice::new(fs).unwrap(), fs);
    // A linear gain ramp over 240 samples adds at most 1 / 240 per sample, at right angles to
    // the sine's own slope: about 1.04 × steady. 1.5 × leaves room and still catches a jump.
    let bound = 1.5 * steady;
    assert!(play <= bound, "play step {play} > {bound}");
    assert!(stop <= bound, "stop step {stop} > {bound}");
    let fade = samples(PLAY_FADE_SECONDS, fs);
    assert!(
        after[fade..].iter().all(|s| s.to_bits() == 0),
        "not exact zeros after the stop fade"
    );

    // Control: without the fade, play and stop at a peak jump by about 1.
    let (steady, play, stop, _) = play_stop_steps(
        Voice::with_times(fs, SMOOTHING_SECONDS, SWAP_FADE_SECONDS, 0.0).unwrap(),
        fs,
    );
    assert!(
        play > 1.5 * steady,
        "the play control did not click: {play}"
    );
    assert!(
        stop > 1.5 * steady,
        "the stop control did not click: {stop}"
    );
}

#[test]
fn swap_queue() {
    let fs = 48_000.0f32;
    let frame = sine(256, 1.0);
    let (a, b, c) = (row(&frame, 1), row(&frame, 2), row(&frame, 3));

    // Sounding: a crossfade.
    let mut v = Voice::new(fs).unwrap();
    assert!(v.set_table(Arc::clone(&a)).is_none());
    assert!(
        !v.is_swapping(),
        "installing into an empty voice is no swap"
    );
    v.set_frequency(440.0);
    v.play();
    render(&mut v, 1024);
    assert!(v.set_table(Arc::clone(&b)).is_none());
    assert!(v.is_swapping());
    let back = v
        .set_table(Arc::clone(&c))
        .expect("refused during the swap");
    assert!(Arc::ptr_eq(&back, &c));
    drop(back);
    assert!(
        v.take_retired().is_none(),
        "nothing retired before the fade ends"
    );
    let fade = samples(SWAP_FADE_SECONDS, fs);
    render(&mut v, fade - 1);
    assert!(v.is_swapping());
    assert!(v.take_retired().is_none(), "retired one sample early");
    one(&mut v);
    assert!(v.is_swapping(), "a retired table not yet taken");
    let back = v.set_table(Arc::clone(&c)).expect("refused while A waits");
    assert!(Arc::ptr_eq(&back, &c));
    drop(back);
    let retired = v.take_retired().expect("A once the swap is done");
    assert!(Arc::ptr_eq(&retired, &a));
    drop(retired);
    assert!(!v.is_swapping());
    assert!(v.take_retired().is_none());
    // The voice holds B only, and never cloned anything.
    assert_eq!(Arc::strong_count(&a), 1);
    assert_eq!(Arc::strong_count(&b), 2);
    assert_eq!(Arc::strong_count(&c), 1);

    // Silent: replaced at once, the old one retired.
    let mut v = Voice::new(fs).unwrap();
    assert!(v.set_table(Arc::clone(&a)).is_none());
    assert!(v.set_table(Arc::clone(&b)).is_none());
    assert!(v.is_swapping(), "A is retired and not yet taken");
    let retired = v.take_retired().expect("A at once");
    assert!(Arc::ptr_eq(&retired, &a));
    assert!(!v.is_swapping());
    assert!(v.set_table(Arc::clone(&c)).is_none());
    let retired = v.take_retired().expect("B at once");
    assert!(Arc::ptr_eq(&retired, &b));
}

/// Plays the low-note sine, sets an unusable note (0 Hz) at a peak, and after the fade the low
/// note again; returns (steady largest step, largest step from the mute on, the muted samples
/// after the fade).
fn unusable_note_steps(mut v: Voice, fs: f32) -> (f32, f32, Vec<f32>) {
    assert!(v.set_table(row(&sine(2048, 1.0), 1)).is_none());
    v.set_frequency(LOW_NOTE);
    v.play();
    render(&mut v, 1024);
    let warm = one(&mut v);
    let steady = render(&mut v, 2048);
    let steady_step = largest_step(warm, &steady);
    let before = to_peak(&mut v);
    v.set_frequency(0.0);
    let fade = samples(PLAY_FADE_SECONDS, fs);
    let muted = render(&mut v, fade + 1024);
    v.set_frequency(LOW_NOTE);
    let back = render(&mut v, 2048);
    assert!(
        back[1024..].iter().any(|s| s.abs() > 0.99),
        "the note did not come back"
    );
    let mut all = muted.clone();
    all.extend_from_slice(&back);
    (
        steady_step,
        largest_step(before, &all),
        muted[fade..].to_vec(),
    )
}

#[test]
fn no_click_on_unusable_note() {
    let fs = 48_000.0f32;
    let (steady, step, silent) = unusable_note_steps(Voice::new(fs).unwrap(), fs);
    // The mute and the return ramp the note gain like play/stop: about 1.04 × steady (see
    // `no_click_on_play_stop`); 1.5 × still catches a jump.
    let bound = 1.5 * steady;
    assert!(step <= bound, "step {step} > {bound} (steady {steady})");
    assert!(
        silent.iter().all(|s| s.to_bits() == 0),
        "not exact zeros once muted"
    );

    // Control: without the fade, the mute at a peak jumps by about 1.
    let (steady, step, _) = unusable_note_steps(
        Voice::with_times(fs, SMOOTHING_SECONDS, SWAP_FADE_SECONDS, 0.0).unwrap(),
        fs,
    );
    assert!(
        step > 1.5 * steady,
        "the control did not click: {step} (steady {steady})"
    );
}

#[test]
fn playhead_clamps_on_a_smaller_grid() {
    // 51 200 Hz makes the ramp 1024 samples, so every value 3 − k/1024 on it is exact in f32
    // (see `playhead_and_note_ramp`); 1e-6 is only slack for that bound.
    let fs = 51_200.0f32;
    let len = samples(SMOOTHING_SECONDS, fs);
    assert_eq!(len, 1024);
    let frame = sine(256, 1.0);
    let mut v = Voice::new(fs).unwrap();
    assert!(v.set_table(row(&frame, 256)).is_none());
    v.set_playhead(200.0, 0.0);
    v.set_frequency(220.0);
    v.play();
    render(&mut v, 2 * samples(PLAY_FADE_SECONDS, fs)); // sounding at full gain

    // The swap clamps the playhead into the new 4-frame grid at once.
    assert!(v.set_table(row(&frame, 4)).is_none());
    assert!(v.is_swapping());
    let (mut y, z) = v.playhead();
    assert_eq!(y.to_bits(), 3f32.to_bits(), "clamped into the new grid");
    assert_eq!(z.to_bits(), 0f32.to_bits());

    // From there, 3 → 2 is a ramp over the whole smoothing time, not a late step.
    v.set_playhead(2.0, 0.0);
    let max_dy = 1.0 / len as f32 + 1e-6;
    for i in 0..len {
        if i == len - 1 {
            assert_ne!(v.playhead().0.to_bits(), 2f32.to_bits(), "arrived early");
        }
        one(&mut v);
        let ny = v.playhead().0;
        assert!((ny - y).abs() <= max_dy, "sample {i}: y {y} -> {ny}");
        let expected = 3.0 - (i + 1) as f32 / len as f32;
        assert!(
            (ny - expected).abs() <= 1e-6,
            "sample {i}: y {ny}, not {expected}"
        );
        y = ny;
    }
    assert_eq!(v.playhead().0.to_bits(), 2f32.to_bits());
}
