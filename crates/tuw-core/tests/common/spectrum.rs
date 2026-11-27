// SPDX-License-Identifier: GPL-3.0-or-later

//! Spectral analysis for the render tests: how far below the fundamental is everything that is
//! not a harmonic of the note?

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

/// Bins on each side of a harmonic that belong to it. The 4-term Blackman–Harris main lobe is
/// 4 bins wide on each side; 6 leaves room for scalloping when a harmonic falls between bins.
pub const GUARD_BINS: f64 = 6.0;

/// Harmonics closer than this many bins leave too few bins between them to examine.
pub const MIN_SPACING_BINS: f64 = 16.0;

/// The 4-term Blackman–Harris window of length `n` (periodic form, sidelobes at about −92 dB).
fn blackman_harris(n: usize) -> Vec<f64> {
    let (a0, a1, a2, a3) = (0.35875, 0.48829, 0.14128, 0.01168);
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            a0 - a1 * x.cos() + a2 * (2.0 * x).cos() - a3 * (3.0 * x).cos()
        })
        .collect()
}

/// The largest bin of `signal` that is not within [`GUARD_BINS`] of a multiple of `note`
/// (DC included), in dB relative to the fundamental's bin.
///
/// The signal is windowed with a 4-term Blackman–Harris window and analysed by an f64 FFT of
/// its whole length. Panics if harmonics are closer than [`MIN_SPACING_BINS`] or if no bin was
/// examined, so a test using it can never pass vacuously.
pub fn worst_non_harmonic_db(signal: &[f32], note: f64, sample_rate: f64) -> f64 {
    let n = signal.len();
    let window = blackman_harris(n);
    let mut buf: Vec<Complex<f64>> = signal
        .iter()
        .zip(&window)
        .map(|(&s, &w)| Complex::new(f64::from(s) * w, 0.0))
        .collect();
    FftPlanner::<f64>::new()
        .plan_fft_forward(n)
        .process(&mut buf);
    let magnitude: Vec<f64> = buf.iter().take(n / 2 + 1).map(|c| c.norm()).collect();

    let spacing = note * n as f64 / sample_rate;
    assert!(
        spacing >= MIN_SPACING_BINS,
        "harmonics only {spacing:.1} bins apart; need at least {MIN_SPACING_BINS}"
    );

    // The fundamental: the largest bin within the guard around it.
    let lo = (spacing - GUARD_BINS).ceil().max(0.0) as usize;
    let hi = ((spacing + GUARD_BINS).floor() as usize).min(magnitude.len() - 1);
    let fundamental = magnitude[lo..=hi].iter().fold(0.0f64, |m, &v| m.max(v));
    assert!(fundamental > 0.0, "no fundamental at bin {spacing:.1}");

    let mut worst = 0.0f64;
    let mut examined = 0usize;
    for (bin, &value) in magnitude.iter().enumerate() {
        let k = (bin as f64 / spacing).round();
        if (bin as f64 - k * spacing).abs() <= GUARD_BINS {
            continue;
        }
        examined += 1;
        worst = worst.max(value);
    }
    assert!(examined > 0, "no non-harmonic bin was examined");
    20.0 * (worst / fundamental).log10()
}
