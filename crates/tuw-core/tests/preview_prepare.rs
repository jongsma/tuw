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
            // The levels are computed in f64 and stored as f32, so the stray content of a bin
            // that should be empty is the f32 rounding of each sample (relative 6e-8) spread
            // over the spectrum: of order 6e-8 x sqrt(2 H) x largest, below 3e-6 of the largest
            // bin even for flat spectra. 1e-4 is also what an all-f32 path (up to 16 radix
            // stages of 1.2e-7 each on lengths <= 2^16) would meet, with a margin. The absolute
            // 1e-6 covers all-zero and near-zero frames, where the relative bound is meaningless.
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
                // f32 input and output samples (relative error 6e-8 each) and f64 FFTs are far
                // below a relative error of
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
            // The waveform itself, not only the magnitudes: a time-reversed or shifted level
            // has the same spectrum. Every part is kept once H >= 7. Samples have magnitude
            // <= 1.75, so f32 rounding (6e-8 relative) is far below 1e-5.
            if h >= 7 {
                for (j, &s) in level.iter().enumerate() {
                    let expected: f64 = parts
                        .iter()
                        .map(|&(k, a)| {
                            a * (2.0 * std::f64::consts::PI * f64::from(k) * j as f64 / m).sin()
                        })
                        .sum();
                    assert!(
                        (f64::from(s) - expected).abs() <= 1e-5,
                        "n = {n}, level {l}: sample {j} = {s}, expected {expected}"
                    );
                }
            }
        }
    }
}

/// A table of `y` x `z` frames that all differ (each a different constant level).
fn varied(y_size: u32, z_size: u32) -> Wavetable {
    let mut table = Wavetable::silent(8, y_size, z_size).unwrap();
    for z in 0..z_size {
        for y in 0..y_size {
            let v = (z * y_size + y + 1) as f32 * 0.01;
            table = table
                .with_frame(y, z, &[v, -v, v, -v, v, v, -v, -v])
                .unwrap();
        }
    }
    table
}

/// AC7: a first prepare builds every distinct frame, and a shared buffer only once.
#[test]
fn first_prepare_builds_all() {
    let mut preparer = Preparer::new();
    preparer.prepare(&varied(4, 3)).unwrap();
    assert_eq!(preparer.frames_built_last(), 12);
    preparer
        .prepare(&Wavetable::silent(8, 4, 3).unwrap())
        .unwrap();
    assert_eq!(preparer.frames_built_last(), 1);
}

/// AC7: after one frame is replaced, only that frame is built; the rest are reused.
#[test]
fn one_changed_frame_builds_one() {
    let mut preparer = Preparer::new();
    let before = varied(4, 3);
    let first = preparer.prepare(&before).unwrap();
    let after = before
        .with_frame(2, 1, &[0.5, 0.1, -0.3, 0.2, 0.0, 0.4, -0.5, 0.3])
        .unwrap();
    let second = preparer.prepare(&after).unwrap();
    assert_eq!(preparer.frames_built_last(), 1);
    for z in 0..3 {
        for y in 0..4 {
            let (a, b) = (first.frame(y, z).unwrap(), second.frame(y, z).unwrap());
            assert_eq!(std::sync::Arc::ptr_eq(a, b), (y, z) != (2, 1), "({y}, {z})");
        }
    }
}

/// AC7: the cache holds only the latest table's frames.
#[test]
fn cache_holds_only_the_latest() {
    let mut preparer = Preparer::new();
    let a = varied(4, 3);
    preparer.prepare(&a).unwrap();
    preparer.prepare(&varied(2, 2)).unwrap();
    preparer.prepare(&a).unwrap();
    assert_eq!(preparer.frames_built_last(), 12);
}

/// A prepare that fails leaves the cache as it was.
#[test]
fn failed_prepare_keeps_the_cache() {
    let mut preparer = Preparer::new();
    let a = varied(4, 3);
    preparer.prepare(&a).unwrap();
    assert!(
        preparer
            .prepare(&Wavetable::silent(8, 2048, 1025).unwrap())
            .is_err()
    );
    preparer.prepare(&a).unwrap();
    assert_eq!(preparer.frames_built_last(), 0);
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

/// Every valid table gives finite levels, even with samples at ±f32::MAX.
#[test]
fn huge_samples_stay_finite() {
    let alternating: Vec<f32> = (0..2048)
        .map(|i| if i % 2 == 0 { f32::MAX } else { -f32::MAX })
        .collect();
    let mut spike = vec![f32::MAX; 2048];
    spike[1000] = -f32::MAX;
    for samples in [alternating, spike] {
        let prepared = prepare(&one_frame(&samples));
        let frame = prepared.frame(0, 0).unwrap();
        for l in 0..frame.level_count() {
            assert!(
                frame.level(l).iter().all(|s| s.is_finite()),
                "level {l} has a non-finite sample"
            );
        }
    }
}
