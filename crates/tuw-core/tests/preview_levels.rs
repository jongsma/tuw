// SPDX-License-Identifier: GPL-3.0-or-later

//! Level math of the preview oscillator: harmonic counts, memory and Nyquist weights.

use tuw_core::preview::{
    FADE_START, MAX_MEMORY_FACTOR, level_harmonics, level_length, level_weights,
};

const RATES: [f32; 3] = [44_100.0, 48_000.0, 96_000.0];

/// The degree to which a level is on, computed independently of the crate (f64).
fn on(h: u32, f: f64, fs: f64) -> f64 {
    let ny = fs / 2.0;
    ((ny - f64::from(h) * f) / ((1.0 - f64::from(FADE_START)) * ny)).clamp(0.0, 1.0)
}

/// Full weight vector, one entry per level index.
fn weight_vector(levels: &[u32], f: f32, fs: f32) -> Vec<f64> {
    let w = level_weights(levels, f, fs);
    let mut v = vec![0.0; levels.len()];
    if w.w_first.to_bits() != 0 || w.w_second.to_bits() != 0 {
        v[w.first] = f64::from(w.w_first);
        if w.first + 1 < levels.len() {
            v[w.first + 1] = f64::from(w.w_second);
        }
    }
    v
}

/// Checks `first`, `w_first` and `w_second` against the independent `on()` (tolerance 1e-6:
/// the weights are f32, whose rounding error is about 6e-8).
fn check_pinned(levels: &[u32], f: f32, fs: f32) {
    let w = level_weights(levels, f, fs);
    let (f64f, fs64) = (f64::from(f), f64::from(fs));
    match levels.iter().position(|&h| on(h, f64f, fs64) > 0.0) {
        None => assert!(
            w.w_first.to_bits() == 0 && w.w_second.to_bits() == 0,
            "f = {f}"
        ),
        Some(first) => {
            let o = on(levels[first], f64f, fs64);
            assert_eq!(w.first, first, "first at f = {f}, fs = {fs}");
            assert!(
                (f64::from(w.w_first) - o).abs() < 1e-6,
                "w_first at f = {f}"
            );
            let expect = if first + 1 < levels.len() {
                1.0 - o
            } else {
                0.0
            };
            assert!(
                (f64::from(w.w_second) - expect).abs() < 1e-6,
                "w_second at f = {f}"
            );
        }
    }
}

#[test]
fn weights_for_a_known_note() {
    // N = 600: levels 299, 211, 149, 105, 74, 52, ... At 440 Hz and 48 kHz, ny = 24 000:
    // 74 * 440 = 32 560 > ny (off), 52 * 440 = 22 880 < ny, so first = 5 (H = 52),
    // on = (24 000 - 22 880) / 4 800 = 0.23333 and w_second = 0.76667.
    let levels = level_harmonics(600);
    assert_eq!(&levels[..6], &[299, 211, 149, 105, 74, 52]);
    let w = level_weights(&levels, 440.0, 48_000.0);
    assert_eq!(w.first, 5);
    assert!((f64::from(w.w_first) - 1120.0 / 4800.0).abs() < 1e-6);
    assert!((f64::from(w.w_second) - 3680.0 / 4800.0).abs() < 1e-6);
}

#[test]
fn harmonics_per_frame_size() {
    assert_eq!(level_harmonics(8), vec![3, 2, 1]);
    for n in [600u32, 601, 2048, 65_536] {
        let l = level_harmonics(n);
        assert_eq!(l[0], (n - 1) / 2);
        assert_eq!(*l.last().unwrap(), 1);
        for pair in l.windows(2) {
            assert!(pair[1] < pair[0], "strictly decreasing for {n}");
            let ratio = f64::from(pair[1]) / f64::from(pair[0]);
            assert!(ratio <= 0.75, "ratio {ratio} for {n}");
            if pair[0] >= 40 {
                // two levels per octave: 0.707 minus rounding of at most half a harmonic
                assert!(ratio >= 0.69, "ratio {ratio} at {} for {n}", pair[0]);
            }
        }
    }
}

#[test]
fn memory_stays_under_the_cap() {
    for n in 8u32..=65_536 {
        let total: usize = level_harmonics(n).iter().map(|&h| level_length(h)).sum();
        assert!(total <= MAX_MEMORY_FACTOR * n as usize, "N = {n}: {total}");
    }
}

#[test]
fn no_level_above_nyquist() {
    // Deterministic pseudo-random numbers (LCG), no extra dependency.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 40) as f64 / (1u64 << 24) as f64
    };
    for fs in RATES {
        let ny = f64::from(fs) / 2.0;
        for n in [8u32, 600, 2048] {
            let levels = level_harmonics(n);
            let mut freqs: Vec<f32> = (0..=127)
                .map(|m| (440.0 * 2f64.powf((f64::from(m) - 69.0) / 12.0)) as f32)
                .collect();
            freqs.extend((0..1000).map(|_| (next() * 2.0 * f64::from(fs)).max(1e-3) as f32));
            freqs.push(ny as f32);
            for &h in &levels {
                freqs.push((ny / f64::from(h)) as f32);
                freqs.push((f64::from(FADE_START) * ny / f64::from(h)) as f32);
            }
            for f in freqs {
                check_pinned(&levels, f, fs);
                let w = level_weights(&levels, f, fs);
                let v = weight_vector(&levels, f, fs);
                for (i, &weight) in v.iter().enumerate() {
                    if weight > 0.0 {
                        assert!(
                            f64::from(levels[i]) * f64::from(f) < ny,
                            "level {i} (H = {}) on at f = {f}, fs = {fs}",
                            levels[i]
                        );
                    }
                }
                if f64::from(f) >= ny {
                    assert!(w.w_first.to_bits() == 0 && w.w_second.to_bits() == 0);
                }
            }
        }
    }
}

#[test]
fn weights_are_continuous() {
    for fs in RATES {
        let ny = f64::from(fs) / 2.0;
        for n in [8u32, 600, 2048] {
            let levels = level_harmonics(n);
            let last = *levels.last().unwrap();
            let steps = 100_000;
            let ratio = (1.2 * ny).powf(1.0 / steps as f64);
            let mut f = 1.0f64;
            let mut prev = weight_vector(&levels, f as f32, fs);
            for _ in 0..steps {
                f *= ratio;
                check_pinned(&levels, f as f32, fs);
                let cur = weight_vector(&levels, f as f32, fs);
                for (a, b) in prev.iter().zip(&cur) {
                    assert!((a - b).abs() <= 0.01, "jump at f = {f}: {a} -> {b}");
                }
                let sum: f64 = cur.iter().sum();
                let fade_on = f64::from(last) * f >= f64::from(FADE_START) * ny;
                if !fade_on {
                    // f32 weights: rounding error well under 1e-6
                    assert!((sum - 1.0).abs() < 1e-6, "sum {sum} at {f}");
                } else if f64::from(last) * f < ny {
                    // the last level fades out alone, or still crossfades into it
                    let w = level_weights(&levels, f as f32, fs);
                    let expect = if w.first + 1 == levels.len() {
                        on(last, f64::from(f as f32), f64::from(fs))
                    } else {
                        1.0
                    };
                    assert!((sum - expect).abs() < 1e-6, "sum {sum} vs {expect} at {f}");
                }
                prev = cur;
            }
        }
    }
}
