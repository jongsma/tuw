# TUW3 reader and writer (version 0)

- **Issue:** #13
- **Status:** Draft
- **Crates:** tuw3
- **Format change:** yes (`SPEC.md` gains the duplicate-key rule; version bump: no)

---

## Spec

### Problem

TUW3 version 0 is specified in `docs/SPEC.md` (decision D11), but nothing can read or write it
yet. TUWE needs to save and open tables, TUWS will later load them, and authors of other tools
need a reference implementation they can read next to the spec. Until `tuw3` can do this,
no other Phase 1 work can store a table.

### Behaviour

**Scope.** `tuw3` turns a table into TUW3 bytes and back. It does no file or other I/O, no DSP
and no editing; callers read and write files themselves (`std::fs::read` / `std::fs::write`).

**Table.** A `Table` holds the dimensions (`frame_size`, `y_size`, `z_size`), the samples in spec
order (Z-major) and the metadata. A `Table` is always valid:

- it can only be created by `Table::new(frame_size, y_size, z_size, samples, metadata)` or by
  `read`, and both apply the same checks: dimensions within the spec limits, a sample count of
  exactly `frame_size × y_size × z_size`, every sample finite, serialised metadata at most
  1 MiB;
- it gives read-only access: the dimensions, `frame(y, z)` (one frame, or `None` outside the
  grid), `samples()` (the whole block) and `metadata()`;
- to change a table, the caller takes it apart (`into_parts()`) and builds a new one.

Editing models belong to `tuw-core`, which converts to a `Table` to save or play.

**Metadata.** `Metadata` has one public `Option<String>` per known spec field (`name`,
`author`, `license`, `y_label`, `z_label`, `description`). Unknown keys from a file are kept
privately and written back unchanged in content; they are not exposed in the API.
`Metadata::default()` is empty. A known field must be a JSON string when present: `null`, a
number or any other type is a wrong type (absent and `null` are not the same).

**Reading.** `read(bytes: &[u8]) -> Result<Table, ReadError>` accepts exactly the files the spec
allows and rejects every case in the spec's *Reading a file* section. It checks the fixed header
and the exact file length before it allocates anything sized by the header, so a forged header
cannot cause a large allocation. Each failure is a distinct error variant with context (for
example the field, value and allowed range, or the `(y, z, x)` position of a non-finite sample).

**Writing.** `write(table: &Table) -> Vec<u8>` cannot fail. Its output is deterministic:

- metadata is compact JSON; known fields in spec order, fields that are `None` left out
  (`Some("")` is written as `""`), then unknown keys in alphabetical order (also inside nested
  objects);
- a table without any metadata is written with `meta_len = 0` (no `{}`);
- padding bytes are zero.

**Errors.** Three typed errors, each implementing `Display` and `std::error::Error`:

- `TableError`: what makes a table invalid, whether it comes from a file or from code
  (size out of range, too many samples, sample count mismatch, non-finite sample, metadata
  too long);
- `MetadataError`: not UTF-8, invalid JSON, not an object, known field of the wrong type,
  duplicate key;
- `ReadError`: file-level failures (too short, bad magic, unsupported version, unsupported
  flags, `meta_len` too long, truncated, trailing bytes, non-zero padding) and wrappers for
  `TableError` and `MetadataError`.

**Constants.** `MAGIC`, `VERSION`, `FIXED_HEADER_LEN` and the spec limits are public, so tools
can use the same numbers (for example to bound an input field in TUWE).

**Spec change.** `SPEC.md` *Metadata* gains: keys must be unique within every JSON object, at
every nesting level; a file with a duplicate key is invalid. The reason (one file meaning
different things in different tools, e.g. two `license` values) goes into `DECISIONS.md`.

**Round trips.**

- `read(write(t))` equals `t` for every valid table, bit for bit.
- `write(read(b))` equals `b` only for files already in this writer's canonical form. A valid
  file from another tool (e.g. pretty-printed JSON) keeps its content but is re-written in
  canonical form.

### Acceptance criteria

Tests are written before the code. Float comparisons in these tests use `f32::to_bits`: the
format does no arithmetic, so every bit must survive, and `==` would equate `-0.0` and `0.0`.

- [ ] **AC1 (layout):** `write` of a small hand-built table produces exactly the bytes assembled
      in the test from the `SPEC.md` layout table (header, metadata, padding, samples), and
      `read` of those bytes returns that table.
- [ ] **AC2 (alignment):** for every `meta_len` from 0 to 64, the sample block starts at a
      multiple of 16 and the padding is at most 15 zero bytes.
- [ ] **AC3 (frame order):** in a table whose samples encode their own `(y, z, x)`,
      `frame(y, z)` returns the right frame for every position, including the last frame on
      each axis; positions outside the grid return `None`.
- [ ] **AC4 (round trip):** `read(write(t))` returns `t` for: `y_size = 1`, `z_size = 1`, both 1,
      `frame_size` 8 and 600; samples including `-0.0`, subnormals and `±f32::MAX`; metadata that
      is empty, complete, contains Unicode and escapes, and contains nested unknown keys.
- [ ] **AC5 (determinism):** writing the same table twice gives identical bytes; empty metadata
      gives `meta_len = 0`; known fields appear in spec order before unknown keys, which are
      sorted.
- [ ] **AC6 (table validity):** `Table::new` rejects, each with its own `TableError`: every
      dimension just outside its limit (and accepts the limit itself), a total above 2²⁸
      samples, a wrong sample count, NaN, +Inf and −Inf (including at the last sample), and
      metadata that serialises to more than 1 MiB.
- [ ] **AC7 (rejection):** `read` returns the matching `ReadError` for each case in the spec's
      *Reading a file*: too short, bad magic, version ≠ 0, any flag bit set, sizes out of range,
      `meta_len` over 1 MiB, truncated, trailing bytes, non-zero padding, metadata that is not
      UTF-8 / not JSON / not an object / has a known field of the wrong type (including `null`),
      non-finite sample.
- [ ] **AC8 (forged header):** a 24-byte file whose header claims the maximum sizes is rejected
      as truncated, without allocating memory sized by the header.
- [ ] **AC9 (duplicate keys):** metadata with a duplicate key, at the top level or inside a
      nested unknown value, is rejected; `SPEC.md` and `DECISIONS.md` state the rule.
- [ ] **AC10 (unknown keys):** unknown keys of any JSON type survive `read` → `write` → `read`
      with the same content.
- [ ] **AC11 (robustness):** property tests (`proptest`): random valid tables always round-trip,
      and `read` on random bytes never panics.
- [ ] **AC12 (crate rules):** `tuw3` has no I/O, DSP, GUI or plugin code; its only new runtime
      dependency is `serde_json` (with `serde`); `proptest` is a dev-dependency only;
      `cargo deny check` passes; every public item has a doc comment.

### Out of scope

- File I/O, streaming readers/writers (`io::Read`/`io::Write`), memory-mapping and lazy
  per-slice loading. The bytes API is designed so these can be added later without breaking it.
- Public access to unknown metadata keys (can be added later without breaking the API).
- Preserving the original order or formatting of a foreign file's JSON.
- Serum-style WAV import/export (#9), normalisation (#7), mip levels (#8).
- Fuzzing with `cargo-fuzz` (needs nightly Rust).
- Using `tuw3` from `tuw-core` or TUWE.

### Open questions

None. Settled while brainstorming (Oct 2026):

- API: bytes in, bytes out; no streams. Typical tables are a few MiB; streaming would only save
  the temporary raw copy, and the memory-saving path for very large tables (mmap, lazy loading)
  works on `&[u8]`.
- JSON via `serde_json` rather than a hand-written parser (correctness and a nesting limit on
  untrusted input).
- Metadata: typed known fields, unknown keys preserved but private; `serde_json` stays out of
  the public API.
- Duplicate keys are rejected (spec change, see above).
- `Table` enforces its own validity; `write` is infallible.

---

## Plan

To be written with the `writing-plans` skill once this spec is approved.

---

## Tasks

To be written with the plan.

---

## Notes

Things learned during implementation; spec changes made along the way.
