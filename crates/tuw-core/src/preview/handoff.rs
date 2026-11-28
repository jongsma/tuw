// SPDX-License-Identifier: GPL-3.0-or-later

//! Handing the preview from the UI thread to the audio thread without locks.
//!
//! [`preview`] makes a pair: [`PreviewControl`] for the UI thread and [`Oscillator`] for the
//! audio thread. Three single-producer single-consumer rings ([`rtrb`]) connect them:
//!
//! - **tables**, UI → audio: prepared tables to play. A table waiting for a swap to finish
//!   stays on this ring, so it never holds up a control change.
//! - **controls**, UI → audio: playhead, note, play and stop. Drained at every render.
//! - **returned tables**, audio → UI: tables the voice no longer needs.
//!   [`PreviewControl::collect`] drops them on the UI thread, so freeing a table (possibly
//!   many megabytes) never happens on the audio thread, even when the UI dropped its own
//!   references long before.

use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};

use super::{PreparedTable, PreviewError, Voice};

/// Largest ring capacity [`preview`] uses; a larger request is clamped to it. It bounds the
/// memory the rings and holding slots take, so no capacity can make [`preview`] fail.
const MAX_CAPACITY: usize = 1 << 16;

/// One control change, sent from [`PreviewControl`] to [`Oscillator`].
#[derive(Debug, Clone, Copy)]
enum Control {
    Playhead(f32, f32),
    Frequency(f32),
    Play,
    Stop,
}

/// Makes a preview for `sample_rate`: the UI half and the audio half.
///
/// `capacity` is how many entries each of the three rings holds, and how many holding slots
/// the [`Oscillator`] keeps for tables the return ring can't take yet. A capacity of 0 is
/// treated as 1, and one above 65 536 as 65 536. A few entries are plenty when the UI sends
/// at most one table per frame it draws and calls [`PreviewControl::collect`] as often.
///
/// Allocates the rings, the holding slots and the voice's kernel: call it off the audio thread,
/// then move the [`Oscillator`] to the audio thread.
///
/// # Errors
///
/// [`PreviewError::InvalidSampleRate`] when `sample_rate` is not finite or not positive.
pub fn preview(
    sample_rate: f32,
    capacity: usize,
) -> Result<(PreviewControl, Oscillator), PreviewError> {
    let voice = Voice::new(sample_rate)?;
    let capacity = capacity.clamp(1, MAX_CAPACITY);
    let (table_tx, table_rx) = RingBuffer::new(capacity);
    let (control_tx, control_rx) = RingBuffer::new(capacity);
    let (return_tx, return_rx) = RingBuffer::new(capacity);
    let mut holding = Vec::with_capacity(capacity);
    holding.resize_with(capacity, || None);
    Ok((
        PreviewControl {
            tables: table_tx,
            controls: control_tx,
            returned: return_rx,
        },
        Oscillator {
            voice,
            tables: table_rx,
            controls: control_rx,
            returned: return_tx,
            holding,
        },
    ))
}

/// The UI half of a preview: sends tables and control changes to the [`Oscillator`] and
/// disposes of the tables it returns.
///
/// Every method returns at once; none waits for the audio thread.
pub struct PreviewControl {
    tables: Producer<Arc<PreparedTable>>,
    controls: Producer<Control>,
    returned: Consumer<Arc<PreparedTable>>,
}

impl std::fmt::Debug for PreviewControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewControl")
            .field("free_table_slots", &self.tables.slots())
            .field("free_control_slots", &self.controls.slots())
            .field("returned_waiting", &self.returned.slots())
            .finish()
    }
}

impl PreviewControl {
    /// Sends `table` to be played next (see *Table swaps* on [`Voice`]). Tables are played
    /// in the order sent, each one once the previous swap has finished.
    ///
    /// # Errors
    ///
    /// Gives `table` back when the table ring is full: keep it and try again later (after a
    /// render), or drop it.
    pub fn set_table(&mut self, table: Arc<PreparedTable>) -> Result<(), Arc<PreparedTable>> {
        self.tables
            .push(table)
            .map_err(|rtrb::PushError::Full(t)| t)
    }

    /// Moves the playhead to (`y`, `z`), in frames (see [`Voice::set_playhead`]).
    ///
    /// # Errors
    ///
    /// [`PreviewError::Busy`] when the control ring is full; try again later.
    pub fn set_playhead(&mut self, y: f32, z: f32) -> Result<(), PreviewError> {
        self.send(Control::Playhead(y, z))
    }

    /// Sets the note to `hz` (see [`Voice::set_frequency`]).
    ///
    /// # Errors
    ///
    /// [`PreviewError::Busy`] when the control ring is full; try again later.
    pub fn set_frequency(&mut self, hz: f32) -> Result<(), PreviewError> {
        self.send(Control::Frequency(hz))
    }

    /// Starts sounding (see [`Voice::play`]).
    ///
    /// # Errors
    ///
    /// [`PreviewError::Busy`] when the control ring is full; try again later.
    pub fn play(&mut self) -> Result<(), PreviewError> {
        self.send(Control::Play)
    }

    /// Fades out to silence (see [`Voice::stop`]).
    ///
    /// # Errors
    ///
    /// [`PreviewError::Busy`] when the control ring is full; try again later.
    pub fn stop(&mut self) -> Result<(), PreviewError> {
        self.send(Control::Stop)
    }

    /// Drops every table the [`Oscillator`] has returned, here on the calling thread, and
    /// returns how many there were. Call it regularly (once per UI frame, say): tables the
    /// return ring can't take wait in the oscillator's holding slots, and while all of those
    /// are full it takes no new table.
    pub fn collect(&mut self) -> usize {
        let mut count = 0;
        while let Ok(table) = self.returned.pop() {
            drop(table);
            count += 1;
        }
        count
    }

    fn send(&mut self, control: Control) -> Result<(), PreviewError> {
        self.controls.push(control).map_err(|_| PreviewError::Busy)
    }
}

/// The audio half of a preview: a [`Voice`] fed through lock-free rings.
///
/// [`render`](Oscillator::render) is real-time safe: it allocates nothing, frees nothing,
/// takes no lock and cannot panic. It never drops a table: every table the voice retires goes
/// back to the [`PreviewControl`] through the return ring, or waits in a holding slot
/// (preallocated in [`preview`]) while that ring is full.
///
/// **Dropping the `Oscillator` frees whatever it still holds** (the voice's tables, tables in
/// the holding slots and rings, the kernel), so drop it off the audio thread: move it back to
/// the UI thread, or hand it to a thread that frees it.
pub struct Oscillator {
    voice: Voice,
    tables: Consumer<Arc<PreparedTable>>,
    controls: Consumer<Control>,
    returned: Producer<Arc<PreparedTable>>,
    /// Tables waiting for room on the return ring. Made with all entries `None` and never
    /// resized; an entry is only written while it is `None`, so writing drops nothing.
    holding: Vec<Option<Arc<PreparedTable>>>,
}

impl std::fmt::Debug for Oscillator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Oscillator")
            .field("voice", &self.voice)
            .field("tables_waiting", &self.tables.slots())
            .field("held", &self.held())
            .finish_non_exhaustive()
    }
}

impl Oscillator {
    /// Fills `out` with the next samples, after applying the control changes and (when no
    /// swap is running) the next table sent. Real-time safe.
    pub fn render(&mut self, out: &mut [f32]) {
        while let Ok(control) = self.controls.pop() {
            match control {
                Control::Playhead(y, z) => self.voice.set_playhead(y, z),
                Control::Frequency(hz) => self.voice.set_frequency(hz),
                Control::Play => self.voice.play(),
                Control::Stop => self.voice.stop(),
            }
        }
        self.flush_holding();
        self.dispose_retired();
        self.take_next_table();
        // A replace while silent retires the old table at once.
        self.dispose_retired();
        self.voice.render(out);
        // A swap fade that ended in this block retired the old table.
        self.dispose_retired();
    }

    /// The voice. For tests and meters: it shows the playhead, the note and swap state.
    pub fn voice(&self) -> &Voice {
        &self.voice
    }

    /// How many holding slots are in use. For tests: it shows tables waiting for room on the
    /// return ring.
    pub fn held(&self) -> usize {
        self.holding.iter().filter(|slot| slot.is_some()).count()
    }

    /// Moves held tables onto the return ring while it has room, oldest slot first.
    fn flush_holding(&mut self) {
        for slot in self.holding.iter_mut().filter(|slot| slot.is_some()) {
            // The ring slot is reserved before the table is taken out of the holding slot,
            // so a table is only moved once it has somewhere to go.
            let Ok(chunk) = self.returned.write_chunk_uninit(1) else {
                return;
            };
            chunk.fill_from_iter(slot.take());
        }
    }

    /// Moves the voice's retired table, if any, onto the return ring, or into a free holding
    /// slot when the ring is full. With neither, the voice keeps it (and, swapping, refuses
    /// new tables) until there is room.
    fn dispose_retired(&mut self) {
        if let Ok(chunk) = self.returned.write_chunk_uninit(1) {
            chunk.fill_from_iter(self.voice.take_retired());
        } else if let Some(slot) = self.holding.iter_mut().find(|slot| slot.is_none()) {
            *slot = self.voice.take_retired();
        }
    }

    /// Hands the next table sent to the voice, if the voice is not swapping and a holding
    /// slot is free (so whatever it gives back has somewhere to go).
    fn take_next_table(&mut self) {
        if self.voice.is_swapping() {
            return;
        }
        let Some(slot) = self.holding.iter_mut().find(|slot| slot.is_none()) else {
            return;
        };
        let Ok(table) = self.tables.pop() else {
            return;
        };
        // Not swapping, so the voice takes the table; should it ever give one back, the free
        // slot found above keeps it (and the oscillator returns it to the UI thread later).
        *slot = self.voice.set_table(table);
    }
}
