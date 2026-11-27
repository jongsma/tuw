// SPDX-License-Identifier: GPL-3.0-or-later

//! Which band-limited level to play at which pitch.

use super::FADE_START;

/// Harmonics kept by each level of a frame of `frame_size` samples, richest first.
///
/// The first level keeps every harmonic below the frame's Nyquist, `(frame_size - 1) / 2`.
/// Each next level keeps about `1 / sqrt 2` of the previous (two levels per octave), but never
/// more than three quarters, and always at least one less. The list ends at one harmonic.
/// A frame too small to have a harmonic gives an empty list.
pub fn level_harmonics(frame_size: u32) -> Vec<u32> {
    let mut h = (frame_size.saturating_sub(1)) / 2;
    let mut levels = Vec::new();
    if h == 0 {
        return levels;
    }
    levels.push(h);
    while h > 1 {
        let by_octave = (f64::from(h) / std::f64::consts::SQRT_2).round() as u32;
        let by_ratio = h * 3 / 4;
        let next = by_octave.min(by_ratio);
        h = if next < h { next } else { h - 1 };
        levels.push(h);
    }
    levels
}

/// Number of samples stored for a level with `harmonics` harmonics: four per harmonic, at
/// least 16 so the interpolation kernel always has a full window.
pub fn level_length(harmonics: u32) -> usize {
    (harmonics as usize).saturating_mul(4).max(16)
}

/// Which two neighbouring levels to mix, and how much of each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelWeights {
    /// Index of the richest level whose top harmonic is below Nyquist.
    pub first: usize,
    /// Weight of level `first`; 0 when no level is usable.
    pub w_first: f32,
    /// Weight of level `first + 1` (0 when `first` is the last level).
    pub w_second: f32,
}

/// Weights of the levels for a note of `frequency` Hz at `sample_rate`.
///
/// A level is *on* to degree `clamp((ny - H * f) / ((1 - FADE_START) * ny), 0, 1)`, where `H`
/// is its top harmonic and `ny` the Nyquist frequency. The richest level that is on gets that
/// degree as weight and the next, thinner level gets the rest. When the fundamental is at or
/// above Nyquist no level is on and both weights are 0. Computed in `f64` so the test
/// `H * f < ny` is exact for the `f32` inputs.
pub fn level_weights(levels: &[u32], frequency: f32, sample_rate: f32) -> LevelWeights {
    let silent = LevelWeights {
        first: 0,
        w_first: 0.0,
        w_second: 0.0,
    };
    let ny = f64::from(sample_rate) / 2.0;
    let f = f64::from(frequency);
    if !(f > 0.0 && ny > 0.0 && f.is_finite() && ny.is_finite()) {
        return silent;
    }
    let fade = (1.0 - f64::from(FADE_START)) * ny;
    for (first, &h) in levels.iter().enumerate() {
        let on = ((ny - f64::from(h) * f) / fade).clamp(0.0, 1.0);
        if on > 0.0 {
            let w_second = if first + 1 < levels.len() {
                1.0 - on
            } else {
                0.0
            };
            return LevelWeights {
                first,
                w_first: on as f32,
                w_second: w_second as f32,
            };
        }
    }
    silent
}
