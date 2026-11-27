// SPDX-License-Identifier: GPL-3.0-or-later

//! The preparer: band-limited levels, the source left alone, the size limit.

mod common;

use common::{assert_same, finite_f32, tuw3_table};
use proptest::prelude::*;
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;
use tuw_core::Wavetable;
use tuw_core::preview::{
    MAX_MEMORY_FACTOR, MAX_PREVIEW_SAMPLES, PreparedTable, Preparer, PreviewError, level_harmonics,
    level_length,
};

/// Magnitudes of the bins of `samples` (unnormalised FFT), computed in f64 so the test's own
/// FFT adds no round-off worth counting next to the preparer's f32 error.
fn spectrum(samples: &[f32]) -> Vec<f64> {
    let mut buf: Vec<Complex<f64>> = samples
        .iter()
        .map(|&s| Complex::new(f64::from(s), 0.0))
        .collect();
    FftPlanner::<f64>::new()
        .plan_fft_forward(buf.len())
        .process(&mut buf);
    buf.iter().map(|c| c.norm()).collect()
}

/// A one-frame table holding `samples`.
fn one_frame(samples: &[f32]) -> Wavetable {
    Wavetable::silent(samples.len() as u32, 1, 1)
        .unwrap()
        .with_frame(0, 0, samples)
        .unwrap()
}

fn prepare(table: &Wavetable) -> std::sync::Arc<PreparedTable> {
    Preparer::new().prepare(table).unwrap()
}

/// A random frame of one of the tested sizes, scaled so its peak is 1 (or all zero).
fn scaled_frame() -> impl Strategy<Value = Vec<f32>> {
    prop_oneof![Just(8usize), Just(600), Just(601), Just(2048)]
        .prop_flat_map(|n| proptest::collection::vec(finite_f32(), n))
        .prop_map(|v| {
            let peak = v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            if peak > 0.0 {
                // Divide in f64: a subnormal peak would overflow 1 / peak in f32.
                v.iter()
                    .map(|&s| (f64::from(s) / f64::from(peak)) as f32)
                    .collect()
            } else {
                v
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// AC1: no level has DC or any harmonic above its stated maximum.
    #[test]
    fn levels_are_band_limited(samples in scaled_frame()) {
        let n = samples.len() as u32;
        let prepared = prepare(&one_frame(&samples));
        let frame = prepared.frame(0, 0).unwrap();
        let harmonics = level_harmonics(n);
        prop_assert_eq!(prepared.levels(), &harmonics[..]);
        prop_assert_eq!(frame.level_count(), harmonics.len());
        for (l, &h) in harmonics.iter().enumerate() {
            let level = frame.level(l);
            prop_assert_eq!(level.len(), level_length(h));
            let mags = spectrum(level);
            let largest = mags.iter().cloned().fold(0.0, f64::max);
            // The level is one f32 inverse FFT (length up to 2^16, so at most 16 radix stages),
            // fed by one f32 forward FFT of the frame. Each stage adds about f32::EPSILON
            // (1.2e-7) of the signal's norm, so the stray energy in a bin that should be empty
            // is of order 16 x 1.2e-7 x sqrt(2 H) x largest, about 1e-5 of the largest bin even
            // for flat spectra; 1e-4 leaves a margin. The absolute 1e-6 covers all-zero and
            // near-zero frames, where the relative bound is meaningless.
            let bound = 1e-4 * largest + 1e-6;
            prop_assert!(mags[0] <= bound, "level {}: DC {} > {}", l, mags[0], bound);
            for (k, &m) in mags.iter().enumerate().take(level.len() / 2 + 1).skip(h as usize + 1) {
                prop_assert!(m <= bound, "level {} (H = {}): bin {} = {} > {}", l, h, k, m, bound);
            }
        }
    }

    /// AC6 (part): preparing reads the table and changes nothing in it.
    #[test]
    fn preparing_changes_nothing(table in tuw3_table(3)) {
        let table = Wavetable::from(table);
        let before = table.to_tuw3();
        let _prepared = Preparer::new().prepare(&table).unwrap();
        assert_same(&before, &table.to_tuw3());
    }
}

/// AC1: a level keeps exactly the harmonics up to its maximum, at their amplitudes.
#[test]
fn levels_keep_the_harmonics() {
    let parts = [(1u32, 1.0f64), (3, 0.5), (7, 0.25)];
    for n in [64usize, 600, 601, 2048] {
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                parts
                    .iter()
                    .map(|&(k, a)| {
                        a * (2.0 * std::f64::consts::PI * f64::from(k) * i as f64 / n as f64).sin()
                    })
                    .sum::<f64>() as f32
            })
            .collect();
        let prepared = prepare(&one_frame(&samples));
        let frame = prepared.frame(0, 0).unwrap();
        for (l, &h) in prepared.levels().iter().enumerate() {
            let level = frame.level(l);
            let m = level.len() as f64;
            let mags = spectrum(level);
            for (k, &mag) in mags.iter().enumerate().take(h as usize + 1).skip(1) {
                // A sine of amplitude a shows as a bin of magnitude a M / 2.
                let amplitude = mag * 2.0 / m;
                let expected = parts
                    .iter()
                    .find(|&&(pk, _)| pk as usize == k)
                    .map_or(0.0, |&(_, a)| a);
                // f32 samples (relative error 6e-8) and f32 FFTs of length <= 2048 (11 stages
                // of round-off, about 1e-6 of the signal) are far below a relative error of
                // 1e-4 for the kept harmonics; absent harmonics are held to 1e-4 of the
                // fundamental (amplitude 1).
                let tolerance = if expected > 0.0 {
                    1e-4 * expected
                } else {
                    1e-4
                };
                assert!(
                    (amplitude - expected).abs() <= tolerance,
                    "n = {n}, level {l} (H = {h}): harmonic {k} = {amplitude}, expected {expected}"
                );
            }
        }
    }
}

/// AC11: the size limit, the memory cap, and the source left unchanged.
#[test]
fn size_limit() {
    let mut preparer = Preparer::new();
    preparer
        .prepare(&Wavetable::silent(8, 2, 1).unwrap())
        .unwrap();
    assert_eq!(preparer.frames_built_last(), 1);

    let too_big = Wavetable::silent(8, 2048, 1025).unwrap();
    let samples = 8u64 * 2048 * 1025;
    assert_eq!(
        preparer.prepare(&too_big).unwrap_err(),
        PreviewError::Unsupported {
            samples,
            limit: 1 << 24
        }
    );
    assert_eq!(MAX_PREVIEW_SAMPLES, 1 << 24);
    // An Err leaves the preparer as it was.
    assert_eq!(preparer.frames_built_last(), 1);

    let wide = one_frame(&vec![0.0; 65_536]);
    let prepared = preparer.prepare(&wide).unwrap();
    assert!(prepared.sample_count() <= MAX_MEMORY_FACTOR * 65_536);

    // Exactly 2^24 samples: accepted. All frames share one buffer, so one frame is built.
    let at_limit = Wavetable::silent(8, 2048, 1024).unwrap();
    let prepared = preparer.prepare(&at_limit).unwrap();
    assert_eq!(preparer.frames_built_last(), 1);
    assert_eq!(prepared.sample_count(), 48 * 2048 * 1024);
    // Unchanged: every sample is still +0.0 (a deep snapshot would cost 64 MiB per copy).
    assert!(
        at_limit
            .frames()
            .all(|(_, _, f)| f.samples().iter().all(|s| s.to_bits() == 0))
    );
}

/// Review focus 5: the smallest frame size.
#[test]
fn frame_size_8() {
    let samples: Vec<f32> = (0..8).map(|i| i as f32 * 0.25 - 1.0).collect();
    let prepared = prepare(&one_frame(&samples));
    assert_eq!(prepared.levels(), &[3, 2, 1]);
    assert_eq!(prepared.frame_size(), 8);
    assert_eq!((prepared.y_size(), prepared.z_size()), (1, 1));
    assert_eq!(prepared.sample_count(), 48);
    let frame = prepared.frame(0, 0).unwrap();
    assert_eq!(frame.level_count(), 3);
    assert!(prepared.frame(1, 0).is_none());
    assert!(prepared.frame(0, 1).is_none());
}
