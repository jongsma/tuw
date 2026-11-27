// SPDX-License-Identifier: GPL-3.0-or-later

//! Rendering the preview: no aliasing, bilinear morph, one level choice, DC, edge notes.
//!
//! Every control is set before `play()`, so nothing in these tests depends on smoothing.

mod common;

use std::f64::consts::PI;
use std::sync::Arc;

use common::spectrum::worst_non_harmonic_db;
use tuw_core::Wavetable;
use tuw_core::preview::{FADE_START, PreparedTable, Preparer, Voice, level_weights};

/// Samples analysed per spectrum: 2¹⁵ keeps the debug-build suite fast and resolves harmonics
/// of MIDI 48 at 96 kHz about 45 bins apart.
const ANALYSIS: usize = 1 << 15;

/// Samples rendered and thrown away before analysing.
const WARM_UP: usize = 1024;

fn midi(note: f64) -> f64 {
    440.0 * 2f64.powf((note - 69.0) / 12.0)
}

/// The naive saw: sample `i` of `n` is `2i/n − 1`.
fn saw(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (2.0 * i as f64 / n as f64 - 1.0) as f32)
        .collect()
}

fn sine(n: usize, harmonic: f64, offset: f64) -> Vec<f32> {
    (0..n)
        .map(|i| (offset + (2.0 * PI * harmonic * i as f64 / n as f64).sin()) as f32)
        .collect()
}

/// A `y_size × z_size` table with `frames[z * y_size + y]` at (y, z).
fn table(frame_size: u32, y_size: u32, z_size: u32, frames: &[Vec<f32>]) -> Arc<PreparedTable> {
    let mut t = Wavetable::silent(frame_size, y_size, z_size).unwrap();
    for z in 0..z_size {
        for y in 0..y_size {
            t = t
                .with_frame(y, z, &frames[(z * y_size + y) as usize])
                .unwrap();
        }
    }
    Preparer::new().prepare(&t).unwrap()
}

/// A voice playing `table` at (`y`, `z`) and `hz`, everything set before `play`.
fn voice(table: &Arc<PreparedTable>, fs: f32, hz: f32, y: f32, z: f32) -> Voice {
    let mut v = Voice::new(fs).unwrap();
    assert!(v.set_table(Arc::clone(table)).is_none());
    v.set_playhead(y, z);
    v.set_frequency(hz);
    v.play();
    v
}

/// `n` samples after `warm_up` thrown away.
fn render(v: &mut Voice, warm_up: usize, n: usize) -> Vec<f32> {
    let mut skip = vec![0.0; warm_up];
    v.render(&mut skip);
    let mut out = vec![0.0; n];
    v.render(&mut out);
    out
}

fn assert_close(a: &[f32], b: &[f32], tolerance: f32, what: &str) {
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!(
            (x - y).abs() <= tolerance,
            "{what}: sample {i}: {x} vs {y} (tolerance {tolerance})"
        );
    }
}

#[test]
fn no_aliasing_rendered() {
    let n = 2048;
    let saw_table = table(n as u32, 1, 1, &[saw(n)]);
    let levels = saw_table.levels().to_vec();
    let mid = levels[levels.len() / 2];
    let mut failures = Vec::new();
    for fs in [44_100.0f64, 48_000.0, 96_000.0] {
        let ny = fs / 2.0;
        let mut notes: Vec<f64> = [48.0, 69.0, 96.0, 108.0, 120.0]
            .iter()
            .map(|&m| midi(m))
            .collect();
        // Inside the crossfade of the mid level: its weight there is exactly 0.5.
        notes.push((1.0 + f64::from(FADE_START)) / 2.0 * ny / f64::from(mid));
        for f in notes {
            let w = level_weights(&levels, f as f32, fs as f32);
            let mut v = voice(&saw_table, fs as f32, f as f32, 0.0, 0.0);
            let out = render(&mut v, WARM_UP, ANALYSIS);
            let db = worst_non_harmonic_db(&out, f64::from(f as f32), fs);
            println!("fs {fs}, f {f:.2} Hz, weights {w:?}: worst {db:.1} dB");
            if db > -80.0 {
                failures.push(format!("fs {fs}, f {f:.2} Hz: {db:.1} dB"));
            }
        }
    }
    assert!(failures.is_empty(), "above -80 dB: {failures:?}");
}

#[test]
fn analysis_catches_aliasing() {
    // The positive control: a naive saw read with plain linear interpolation aliases badly,
    // and the same analysis must say so.
    let (n, fs, f) = (2048usize, 48_000.0f64, midi(108.0));
    let frame = saw(n);
    let step = f64::from(f as f32) / fs;
    let mut phase = 0.0f64;
    let out: Vec<f32> = (0..ANALYSIS)
        .map(|_| {
            let x = phase * n as f64;
            let i = x.floor() as usize % n;
            let t = x - x.floor();
            let s = f64::from(frame[i]) * (1.0 - t) + f64::from(frame[(i + 1) % n]) * t;
            phase += step;
            phase -= phase.floor();
            s as f32
        })
        .collect();
    let db = worst_non_harmonic_db(&out, f64::from(f as f32), fs);
    println!("naive saw, linear interpolation: worst {db:.1} dB");
    assert!(db > -80.0, "the analysis missed aliasing: {db:.1} dB");
}

/// Three distinct frames of 256 samples: different harmonics and offsets.
fn distinct_frames(count: usize) -> Vec<Vec<f32>> {
    (0..count)
        .map(|i| {
            let mut f = sine(256, (i + 1) as f64, 0.1 * i as f64);
            for (j, s) in f.iter_mut().enumerate() {
                *s += 0.3 * ((j * (i + 3)) % 17) as f32 / 17.0;
            }
            f
        })
        .collect()
}

#[test]
fn bilinear_on_a_frame() {
    let frames = distinct_frames(6);
    let grid = table(256, 3, 2, &frames);
    let alone = table(256, 1, 1, &frames[5..6]); // (2, 1) is index 1 × 3 + 2
    let (fs, f) = (48_000.0, 523.25);
    let a = render(&mut voice(&grid, fs, f, 2.0, 1.0), 0, 4096);
    let b = render(&mut voice(&alone, fs, f, 0.0, 0.0), 0, 4096);
    // The same levels read the same way; only zero-weight corners differ, which add nothing.
    // 1e-6 leaves room for f32 rounding of samples near 1.
    assert_close(&a, &b, 1e-6, "on frame (2, 1)");
}

#[test]
fn bilinear_between_frames() {
    let frames = distinct_frames(4);
    let grid = table(256, 2, 2, &frames);
    let (fs, f, y, z) = (48_000.0, 311.13, 0.25f32, 0.5f32);
    let out = render(&mut voice(&grid, fs, f, y, z), 0, 4096);
    let single: Vec<Vec<f32>> = frames
        .iter()
        .map(|fr| {
            let t = table(256, 1, 1, std::slice::from_ref(fr));
            render(&mut voice(&t, fs, f, 0.0, 0.0), 0, 4096)
        })
        .collect();
    let (a, b) = (f64::from(y), f64::from(z));
    let expected: Vec<f32> = (0..4096)
        .map(|i| {
            ((1.0 - a) * (1.0 - b) * f64::from(single[0][i])
                + a * (1.0 - b) * f64::from(single[1][i])
                + (1.0 - a) * b * f64::from(single[2][i])
                + a * b * f64::from(single[3][i])) as f32
        })
        .collect();
    // Each single render is rounded to f32 once and mixed afterwards; the voice mixes before
    // rounding. Samples stay below about 2, so the difference is a few f32 ulps, far below 1e-5.
    assert_close(&out, &expected, 1e-5, "y 0.25, z 0.5");
}

#[test]
fn one_by_one_and_fractional() {
    let frames = distinct_frames(3);
    let (fs, f) = (44_100.0, 440.0);
    let only = |i: usize| table(256, 1, 1, &frames[i..=i]);
    let reference: Vec<Vec<f32>> = (0..3)
        .map(|i| render(&mut voice(&only(i), fs, f, 0.0, 0.0), 0, 2048))
        .collect();

    // y_size 1, y = 0.7: z = 0.0 picks frame 0 of a 1 × 3 grid, z = 2.0 the last.
    let column = table(256, 1, 3, &frames);
    let out = render(&mut voice(&column, fs, f, 0.7, 2.0), 0, 2048);
    assert_close(&out, &reference[2], 1e-6, "y_size 1, y 0.7, z 2");

    // z_size 1, z = 0.7 (and y past the grid, clamped to the last frame).
    let row = table(256, 3, 1, &frames);
    let out = render(&mut voice(&row, fs, f, 5.0, 0.7), 0, 2048);
    assert_close(&out, &reference[2], 1e-6, "z_size 1, y 5, z 0.7");

    // Both 1, both fractional.
    let out = render(&mut voice(&only(1), fs, f, 0.7, 0.3), 0, 2048);
    assert!(out.iter().all(|s| s.is_finite()));
    assert_close(&out, &reference[1], 1e-6, "1 × 1, y 0.7, z 0.3");
}

#[test]
fn one_level_choice() {
    let frames: Vec<Vec<f32>> = (0..4).map(|i| sine(2048, (i + 1) as f64, 0.0)).collect();
    let grid = table(2048, 2, 2, &frames);
    let levels = grid.levels().to_vec();
    let fs = 48_000.0f32;
    let mid = levels[levels.len() / 2];
    let f = ((1.0 + f64::from(FADE_START)) / 2.0 * f64::from(fs) / 2.0 / f64::from(mid)) as f32;
    let expected = level_weights(&levels, f, fs);
    assert!(
        expected.w_first > 0.0 && expected.w_second > 0.0,
        "not a crossfade note"
    );
    let mut v = voice(&grid, fs, f, 0.4, 0.6);
    let mut one = [0.0f32];
    for _ in 0..1000 {
        v.render(&mut one);
        let w = v.last_weights();
        // Exactly the same computation, so compare bits.
        assert_eq!(w.first, expected.first);
        assert_eq!(w.w_first.to_bits(), expected.w_first.to_bits());
        assert_eq!(w.w_second.to_bits(), expected.w_second.to_bits());
    }
}

#[test]
fn dc_removed() {
    let n = 1 << 14;
    let t = table(2048, 1, 1, &[sine(2048, 1.0, 0.5)]);
    for fs in [44_100.0f32, 48_000.0] {
        // k / 2¹⁴ is exact in f32 and f64, so the phase makes exactly k whole cycles.
        let f = fs * 37.0 / n as f32;
        let out = render(&mut voice(&t, fs, f, 0.0, 0.0), 0, n);
        let mean = out.iter().map(|&s| f64::from(s)).sum::<f64>() / n as f64;
        // The offset 0.5 must be gone; what remains is kernel ripple and f32 rounding of a
        // unit sine, orders of magnitude below 1e-4.
        assert!(mean.abs() <= 1e-4, "fs {fs}: mean {mean}");
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.9, "the sine itself must stay: peak {peak}");
    }
}

#[test]
fn edge_notes() {
    let fs = 48_000.0f32;
    let ny = fs / 2.0;
    let saw_table = table(2048, 1, 1, &[saw(2048)]);

    for f in [ny, 1.5 * ny] {
        let out = render(&mut voice(&saw_table, fs, f, 0.0, 0.0), 0, 4096);
        assert!(out.iter().all(|s| s.to_bits() == 0), "{f} Hz is not silent");
    }

    for f in [0.5f32, midi(0.0) as f32] {
        let mut v = voice(&saw_table, fs, f, 0.0, 0.0);
        let out = render(&mut v, 0, 4096);
        assert!(out.iter().all(|s| s.is_finite()));
        assert!(out.iter().any(|&s| s.abs() > 0.1), "{f} Hz is silent");
        assert_eq!(v.last_weights().first, 0, "{f} Hz");
        assert_eq!(level_weights(saw_table.levels(), f, fs).first, 0);
    }

    // Frame size 8: levels [3, 2, 1], all at the minimum length.
    let tiny = table(8, 1, 1, &[vec![0.3, 0.9, -0.2, -0.7, 0.5, 0.1, -1.0, 0.4]]);
    for f in [20.0f32, 440.0, 5_000.0, 15_000.0] {
        let out = render(&mut voice(&tiny, fs, f, 0.0, 0.0), 0, 4096);
        assert!(out.iter().all(|s| s.is_finite()), "frame size 8 at {f} Hz");
        assert!(
            out.iter().any(|&s| s.abs() > 0.01),
            "frame size 8 at {f} Hz"
        );
    }

    for f in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -440.0] {
        let mut v = voice(&saw_table, fs, 440.0, 0.0, 0.0);
        render(&mut v, 100, 0);
        v.set_frequency(f);
        let out = render(&mut v, 0, 4096);
        assert!(out.iter().all(|s| s.to_bits() == 0), "{f} Hz is not silent");
        assert_eq!(v.last_weights().w_first.to_bits(), 0f32.to_bits());
        assert_eq!(v.last_weights().w_second.to_bits(), 0f32.to_bits());
    }

    // A non-finite coordinate keeps the previous one; the other coordinate still applies.
    let frames = distinct_frames(4);
    let grid = table(256, 2, 2, &frames);
    let reference = render(&mut voice(&grid, fs, 440.0, 1.0, 0.0), 0, 2048);
    let elsewhere = render(&mut voice(&grid, fs, 440.0, 0.0, 0.0), 0, 2048);
    assert!(
        reference
            .iter()
            .zip(&elsewhere)
            .any(|(a, b)| (a - b).abs() > 0.1),
        "the two playheads must sound different, or this test proves nothing"
    );
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut v = Voice::new(fs).unwrap();
        assert!(v.set_table(Arc::clone(&grid)).is_none());
        v.set_playhead(1.0, 1.0);
        v.set_playhead(bad, 0.0);
        v.set_playhead(0.0, bad); // y applies, z stays 0
        v.set_playhead(1.0, bad); // back to (1, 0)
        v.set_frequency(440.0);
        v.play();
        let out = render(&mut v, 0, 2048);
        // The same voice state as the reference, so the same arithmetic.
        assert_close(&out, &reference, 0.0, "non-finite playhead");
    }
}
