// SPDX-License-Identifier: GPL-3.0-or-later

//! The interpolation kernel: reading a stored level between its samples.
//!
//! A level holds one period, band-limited and at least 2× oversampled (`M ≥ 4 H`), so its
//! content sits in the lower half of its own band. Reading it at a fractional position is
//! convolution with a windowed sinc, `h(t) = sinc(t) × kaiser(t / 8, β)` for `|t| < 8`: the
//! sinc keeps the content, the Kaiser window (β = [`KERNEL_BETA`]) pushes the images that
//! interpolation leaves behind far down.

use super::{KERNEL_BETA, KERNEL_PHASES, KERNEL_TAPS};

/// Half the kernel width, in samples: taps reach from `floor(x) - 7` to `floor(x) + 8`.
const HALF: usize = KERNEL_TAPS / 2;

/// The modified Bessel function of the first kind, order 0, by its power series
/// `Σ ((x/2)^k / k!)²`. The terms shrink fast for the `x ≤ 10` the kernel needs.
fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..200 {
        term *= half / k as f64;
        let add = term * term;
        sum += add;
        if add < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// `h(t)`: windowed sinc, zero for `|t| ≥ HALF`.
fn h(t: f64, beta: f64) -> f64 {
    let half = HALF as f64;
    if t.abs() >= half {
        return 0.0;
    }
    let sinc = if t.abs() < 1e-12 {
        1.0
    } else {
        let x = std::f64::consts::PI * t;
        x.sin() / x
    };
    let u = t / half;
    let window = bessel_i0(beta * (1.0 - u * u).sqrt()) / bessel_i0(beta);
    sinc * window
}

/// The kernel, tabulated once at `KERNEL_PHASES + 1` fractional offsets.
///
/// Row `p` holds the taps for a position `p / KERNEL_PHASES` past a stored sample; tap `k`
/// weighs the sample `k - 7` places from it. Built when the voice is made; reading allocates
/// nothing.
pub(crate) struct Kernel {
    /// `(KERNEL_PHASES + 1) × KERNEL_TAPS`, row after row.
    rows: Vec<f64>,
}

impl Kernel {
    /// Tabulates the kernel. Allocates; call it off the audio thread.
    pub(crate) fn new() -> Kernel {
        let beta = f64::from(KERNEL_BETA);
        let mut rows = Vec::with_capacity((KERNEL_PHASES + 1) * KERNEL_TAPS);
        for p in 0..=KERNEL_PHASES {
            let frac = p as f64 / KERNEL_PHASES as f64;
            for k in 0..KERNEL_TAPS {
                // Tap k reads sample floor(x) - 7 + k, which is t = frac + 7 - k away.
                rows.push(h(frac + (HALF - 1) as f64 - k as f64, beta));
            }
        }
        Kernel { rows }
    }

    /// The level `samples` (one period) read at position `x` (in samples, `x ≥ 0`), indexing
    /// circularly. An empty level reads as 0.
    ///
    /// Real-time safe: no allocation, and no index can go out of bounds.
    pub(crate) fn read(&self, samples: &[f32], x: f64) -> f64 {
        let m = samples.len();
        if m == 0 || !x.is_finite() || x < 0.0 {
            return 0.0;
        }
        let floor = x.floor();
        let frac = x - floor;
        let i = (floor as usize) % m;

        // Linear interpolation between the two nearest tabulated rows.
        let pos = frac * KERNEL_PHASES as f64;
        let p = (pos as usize).min(KERNEL_PHASES - 1);
        let t = pos - p as f64;
        let (Some(row0), Some(row1)) = (
            self.rows.get(p * KERNEL_TAPS..(p + 1) * KERNEL_TAPS),
            self.rows.get((p + 1) * KERNEL_TAPS..(p + 2) * KERNEL_TAPS),
        ) else {
            return 0.0;
        };

        let tap = |k: usize, s: f32| {
            let (a, b) = (row0[k], row1[k]);
            (a + t * (b - a)) * f64::from(s)
        };
        // The whole window lies inside the level: read it as one slice.
        if i >= HALF - 1 {
            if let Some(window) = samples.get(i - (HALF - 1)..i + HALF + 1) {
                return window.iter().enumerate().map(|(k, &s)| tap(k, s)).sum();
            }
        }
        // Near the ends (or a level shorter than the kernel): wrap every index. `start` is
        // `i - 7` moved up by whole periods so it can't underflow, and `% m` keeps every
        // index inside the level.
        let start = i + m * KERNEL_TAPS - (HALF - 1);
        (0..KERNEL_TAPS)
            .map(|k| tap(k, samples[(start + k) % m]))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bessel_matches_known_values() {
        // I0(0) = 1, I0(1) = 1.2660658777520082, I0(10) = 2815.716628466254 (tables);
        // the series in f64 agrees to about 1e-15 relative.
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-15);
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_2).abs() < 1e-14);
        assert!((bessel_i0(10.0) / 2_815.716_628_466_254 - 1.0).abs() < 1e-13);
    }

    #[test]
    fn integer_positions_return_the_sample() {
        // At frac 0 the kernel is 1 at tap 7 and 0 at every other (sinc zeros), so reading at
        // an integer position returns the stored sample up to f64 round-off.
        let kernel = Kernel::new();
        let level: Vec<f32> = (0..20).map(|i| (i as f32 * 0.37).sin()).collect();
        for i in 0..40 {
            let got = kernel.read(&level, i as f64);
            assert!((got - f64::from(level[i % 20])).abs() < 1e-12, "at {i}");
        }
    }

    #[test]
    fn short_levels_wrap() {
        // A level shorter than the kernel still reads circularly without panicking.
        let kernel = Kernel::new();
        for len in [1usize, 3, 7, 15, 16] {
            let level = vec![0.5f32; len];
            for step in 0..64 {
                let x = step as f64 * len as f64 / 64.0;
                assert!(kernel.read(&level, x).is_finite());
            }
        }
    }
}
