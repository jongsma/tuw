# Design notes

Ideas, research findings and DSP pitfalls collected while planning TUW. Nothing in here is
binding: settled choices are in [`DECISIONS.md`](DECISIONS.md), the file format is defined in
[`SPEC.md`](SPEC.md). When a note here turns into a decision or a spec change, move it there.

Each note is tagged:
**[concept]** core idea of the project · **[finding]** research result ·
**[proposal]** suggested approach, not decided · **[parked]** idea for a later phase

---

## 1. Core concepts

### 1.1 The 3D wavetable cube [concept]

A table is a grid of single-cycle frames along two independent morph axes, Y and Z.
The playhead is a 2D coordinate (y, z); playback interpolates bilinearly between the four
neighbouring frames. This is what TUW3 stores and what TUWS plays.

### 1.2 Wavetables as morphable filters [concept]

Besides driving an oscillator, a table can shape a **filter**: each frame is treated as a
spectral envelope (magnitude per harmonic/bin), applied via FIR or FFT-based filtering.
Scanning the morph position changes the filter shape, e.g. for vowel/formant morphing.
The same community-made table can serve as oscillator source *and* filter.

**Caveat [finding]:** linear interpolation between frames *crossfades* spectra. Morphing
from an "a" frame to an "o" frame fades one set of formant peaks out and the other in; at the
midpoint you hear both sets, not a formant that moves. For formants that *sweep*, the table
must contain the intermediate positions as frames (many small steps), or the filter needs a
different morph model (e.g. interpolating formant frequencies instead of magnitudes). This
needs a decision before the TUWS filter is designed.

#### Implementation approaches [proposal]

Three viable approaches, in order of spectral accuracy:

**1. FIR via IFFT (most accurate)**
Take a wavetable frame as the desired magnitude response, apply IFFT to derive time-domain
FIR coefficients, then convolve incoming audio with those coefficients. Morph by interpolating
between two frames *before* the IFFT step, so the coefficients themselves are blended rather
than the audio. Latency is proportional to FIR length; manage with overlap-add convolution.

**2. Direct frequency-domain multiply (most efficient)**
FFT a block of incoming audio, multiply each bin by the corresponding wavetable sample
(the frame treated as a per-bin gain), then IFFT back. This is the overlap-add method and
is the natural fit for `rustfft`. Morph by crossfading between two frames before the multiply.

**3. Peak-tracking filter bank (most analog character)**
Analyse the wavetable frame for spectral peaks and drive a bank of resonant filters (SVF or
biquad) from those peaks. Less spectrally precise but gives the character of analog filter
cores. Closest to how formant filters in classic vocoders work.

Approach 2 is the likely starting point for TUWS: it reuses the `rustfft` dependency already
planned for Phase 2, and its latency is set by one fixed internal block size (see the caveats
below).

**Caveats for approaches 1 and 2 [finding]:**

- *Circular convolution.* An FFT treats its block as one period of a repeating signal, so
  multiplying spectra bin by bin convolves *circularly*: a block of L samples filtered by an
  impulse response of M samples gives L + M − 1 output samples, and with an FFT of size L the
  last M − 1 wrap around onto the start of the block (audible as echoes, clicks or smearing at
  block edges). Overlap-add only works with zero padding: FFT size N ≥ L + M − 1, output the
  first L samples, and add the M − 1 tail samples to the start of the next block's output.
- *Approaches 1 and 2 belong together.* A frame used as magnitudes with zero phase gives an
  impulse response centred on time 0, half of it at negative time. It must first be made a
  real, causal filter: shifted by half its length (linear phase, latency of half the filter)
  or converted to minimum phase (low latency, some phase distortion), then windowed to M taps.
  That design step is approach 1; approach 2 is the fast way to apply the result.
- *Mapping a frame onto the bins.* A frame has `frame_size` samples; an audio FFT of size N has
  N/2 + 1 bins. A rule is needed: raw frame samples or the magnitudes of the frame's own
  harmonics as the curve, a linear or logarithmic frequency axis, and fixing the curve in Hz so
  the filter sounds the same at every sample rate and FFT size. Frame samples run from −1 to 1;
  a negative gain flips a bin's phase, so take the absolute value or map the range to 0–1.
- *Morphing over time.* If the morph position changes between blocks, the filter jumps at the
  block edge; crossfade between the old and new filter's output, or smooth the position.
- *Block size and latency.* The host chooses the size of each `process()` buffer and can change
  it on every call (small blocks live, large ones offline, short ones at loop ends; NIH-plug
  can split blocks for sample-accurate automation). The FFT block size therefore cannot follow
  the host: collect input in a FIFO, process fixed blocks of L samples, and output L samples
  late. Report that latency to the host (`context.set_latency_samples()` in NIH-plug); linear
  phase adds half the filter length. At 48 kHz, 256 samples is 5.3 ms and 1024 is 21 ms. The
  host compensates the latency of an effect on a recorded track, but not of a synth played live
  over MIDI, where it is felt as a delay between key and sound. Long filters at small L need
  partitioned convolution.

#### Filtering in the harmonic domain [proposal]

For TUWS's own oscillator there is a fourth option. Its input is a wavetable frame at a known
pitch, so its harmonics are known exactly: multiply each harmonic's amplitude by the filter
curve evaluated at that harmonic's frequency in Hz, and play the resulting frame. No FFT runs on
the live audio, so there is no circular convolution and **no latency**, and it fits naturally
with the bandlimiting the oscillator needs anyway. Limits: it only filters TUWS's own
oscillator, not external audio, and a fast-moving filter means rebuilding the frame often.

#### Filter in the 3D cube [concept]

With a 3D wavetable serving both oscillator and filter roles, each layer gets its own 2D
morph coordinate navigating the same cube independently:

    Oscillator reads cube at (y_osc, z_osc)
    Filter     reads cube at (y_filt, z_filt)

Four independent morph coordinates navigating the same timbral space. The same
community-made wavetable can be used as both the sound source and the filter shape,
giving the instrument a strong sonic identity without extra file formats or design work.

#### Oscillator-drives-filter modulation [proposal]

Tying `y_osc` → `y_filt` (or any axis combination) means the filter timbre tracks the
oscillator timbre as it morphs. The filter character and the source character evolve together
rather than independently. Worth exposing as an explicit modulation routing option rather than
hardwiring it.

## 2. Wavetable DSP pitfalls

### 2.1 Phase alignment and DC across frames [finding] / [proposal]

If neighbouring frames have harmonics with different phases, morphing between them makes
partials partially cancel (audible dips/nulls mid-morph). Frames should also be DC-free.

*Proposal:* offer **phase canonicalization** in TUWE (at import, or as an editor action):
per frame, forward FFT → set every harmonic to a fixed phase, keep magnitudes → inverse FFT.
Absolute per-harmonic phase is essentially inaudible for a steady tone, so this changes how a
frame *looks* but not how it *sounds*, and it removes morph nulls completely. A single
waveform rotation (Bristow-Johnson's method) can only align the overall shape, not every
harmonic, which fails for drawn or algorithmic frames.

Good first test: morph between two canonicalized frames and assert there are no spectral nulls.

*Open:* whether "phase-canonical" and/or "DC-free" should become a normalisation convention
in `SPEC.md`, or stay an editor feature. Not decided.

### 2.2 Anti-aliasing: mip-maps in 3D [finding] / [proposal]

Playing a table across the keyboard needs bandlimited versions: roughly one mip level per
octave (~10–11 per frame), with crossfading between adjacent levels to avoid timbre jumps.
In the 3D cube, the pitch-based level choice must be identical for all four corners of the
bilinear interpolation.

*Proposal:* TUW3 stores only full-band frames; mip levels are computed by the player
(at load or lazily). Not decided; affects memory and load time.

### 2.3 Frame size [finding]

2048 samples per frame is the de facto standard: 1024 harmonics, full bandwidth down to about
20 Hz. AKWF frames are 600 samples (300 harmonics), losing top end below roughly 73 Hz. Fine
for analysis, worth knowing when playing them directly or resampling to 2048.

### 2.4 Morphing and phase [finding]

All frames share one phase accumulator, so morphing does not cause phase discontinuities.
The real issue is zipper noise from stepping the morph position: smooth the Y/Z parameters
(NIH-plug `Smoother`). Mixing interpolated reads per sample is preferable to rebuilding a
blended table.

Use one phase convention throughout: normalised phase (Δφ = f/fs, wrap at 1) or index phase
(Δφ = f/fs × N, wrap at N), never a mix.

### 2.5 Memory [finding]

100 tables × 256 frames × 2048 samples × 4 bytes ≈ 210 MB *before* mip levels. Large
libraries need lazy loading; the format should allow reading the header without the samples.

### 2.6 Integrated (DPW) tables [finding]

If differentiated/integrated tables are ever used: the differentiator must be scaled by 1/Δφ
(numerically fragile at low pitch), and the table must be DC-free before integrating.

## 3. Phase 2: algorithmic generation

### 3.1 Pipeline [concept]

1. Read single-cycle WAVs with `hound`
2. FFT analysis with `rustfft`
3. Extract a harmonic profile (magnitudes, maybe phases) per waveform
4. Generate new waveforms by additive synthesis or interpolation between profiles
5. Write the result as a TUW3 table

### 3.2 Source datasets [finding]

| Dataset | Notes |
|---|---|
| Adventure Kid Waveforms (AKWF), adventurekid.se | ~4,300 single-cycle WAVs, consistent format, broad timbral range, freely available. Best starting set. |
| KRC Mathwaves | 160,000+ mathematically generated tables, Serum/Vital/Surge compatible |
| `publicsamples/Single-Cycle-Waveforms` (GitHub) | Vintage hardware recordings and digitally generated cycles |

Check each dataset's licence before redistributing anything derived from it. Never commit the
datasets themselves (see `AGENTS.md`); reference them by source and licence.

### 3.3 Soliton-based tables [parked]

Evolve a starting waveform through a nonlinear PDE (KdV or nonlinear Schrödinger) with
periodic boundary conditions and save snapshots as successive frames. An arbitrary initial
condition breaks up into an amplitude-sorted train of solitons, so the morph axis follows an
ordered, physically meaningful spectral change instead of plain interpolation.

## 4. References

- R. Bristow-Johnson, *Wavetable Synthesis 101, A Fundamental Perspective*, AES 1996
  (copy on musicdsp.org). Core reading for sections 1–3 and 5: oscillator model, multi-table
  morphing (Prophet VS), wavetable ↔ additive equivalence via the DFT, phase alignment,
  pitch-synchronous extraction. Dated on anti-aliasing (no mip levels).
- EarLevel Engineering, wavetable oscillator article series: practical anti-aliasing.
- Franck & Välimäki (2012) and Välimäki & Huovilainen (2006): bandlimited oscillator research.
