// SPDX-License-Identifier: GPL-3.0-or-later

//! The undo/redo document: its starting state, ids and selection (AC10, AC11, AC12).

mod common;

use std::collections::HashSet;

use common::tuw3_table;
use proptest::prelude::*;
use tuw_core::{Axis, Document, DocumentError, EditError, Selection, Wavetable};

fn table(y: u32, z: u32) -> Wavetable {
    Wavetable::silent(8, y, z).unwrap()
}

#[test]
fn ptr_eq_means_same_buffers() {
    let t = table(2, 2);
    assert!(Wavetable::ptr_eq(&t, &t.clone()));
    assert!(!Wavetable::ptr_eq(
        &t,
        &t.with_frame(0, 0, &[0.5; 8]).unwrap()
    ));
    assert!(!Wavetable::ptr_eq(&table(2, 2), &table(2, 2)));
    let same_metadata = t.with_metadata(t.metadata().clone()).unwrap();
    assert!(!Wavetable::ptr_eq(&t, &same_metadata));
}

#[test]
fn a_new_document() {
    let t = table(3, 2);
    let doc = Document::new(t.clone(), 10);
    assert!(Wavetable::ptr_eq(doc.current(), &t));
    assert!(Wavetable::ptr_eq(doc.finished(), &t));
    assert_eq!(doc.selection(), Selection::all(&t));
    assert!(!doc.is_step_open());
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.redo_label(), None);
    assert!(!doc.is_modified());
    assert_eq!(doc.history_limit(), 10);
}

#[test]
fn change_ids_are_unique_across_documents() {
    let t = table(2, 2);
    let a = Document::new(t.clone(), 10);
    let b = Document::new(t, 10);
    assert_ne!(a.change_id(), b.change_id());
}

#[test]
fn set_selection_fits() {
    let t = table(3, 2);
    let mut doc = Document::new(t.clone(), 10);
    doc.set_selection(Selection::all(&table(5, 5)));
    assert_eq!(doc.selection(), Selection::all(&t));
    let one = Selection::frame(&t, 1, 1).unwrap();
    doc.set_selection(one);
    assert_eq!(doc.selection(), one);
}

fn bits(s: &[f32]) -> Vec<u32> {
    s.iter().map(|x| x.to_bits()).collect()
}

/// The samples of frame (0, 0) of `t`, each multiplied by `g`.
fn gain(t: &Wavetable, g: f32) -> Vec<f32> {
    t.frame(0, 0)
        .unwrap()
        .samples()
        .iter()
        .map(|s| s * g)
        .collect()
}

fn ramp_table() -> Wavetable {
    let samples: Vec<f32> = (1..=8).map(|i| i as f32 / 10.0).collect();
    table(2, 2).with_frame(0, 0, &samples).unwrap()
}

#[test]
fn updates_start_from_the_start() {
    let start = ramp_table();
    let mut doc = Document::new(start.clone(), 10);
    doc.begin_step("Gain").unwrap();
    assert!(doc.is_step_open());
    for g in [1.3_f32, 1.5, 1.2] {
        doc.update_step(|s| {
            assert!(Wavetable::ptr_eq(s, &start));
            s.with_frame(0, 0, &gain(s, g))
        })
        .unwrap();
        assert_eq!(
            bits(doc.current().frame(0, 0).unwrap().samples()),
            bits(&gain(&start, g))
        );
    }
}

#[test]
fn a_gesture_is_one_step() {
    let mut doc = Document::new(ramp_table(), 10);
    doc.begin_step("Gain").unwrap();
    for i in 0..50 {
        doc.update_step(|s| s.with_frame(0, 0, &gain(s, 1.0 + i as f32 / 100.0)))
            .unwrap();
    }
    let last = doc.current().clone();
    doc.finish_step().unwrap();
    assert_eq!(doc.undo_label(), Some("Gain"));
    assert!(!doc.is_step_open());
    assert!(Wavetable::ptr_eq(doc.finished(), &last));
    assert!(Wavetable::ptr_eq(doc.current(), &last));
}

#[test]
fn cancel_restores_the_start() {
    let start = ramp_table();
    let mut doc = Document::new(start.clone(), 10);
    let id = doc.change_id();
    doc.begin_step("Gain").unwrap();
    doc.update_step(|s| s.with_frame(0, 0, &gain(s, 2.0)))
        .unwrap();
    assert!(!Wavetable::ptr_eq(doc.current(), &start));
    doc.cancel_step().unwrap();
    assert!(!doc.is_step_open());
    assert!(Wavetable::ptr_eq(doc.current(), &start));
    assert!(Wavetable::ptr_eq(doc.finished(), &start));
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.change_id(), id);
    assert!(!doc.is_modified());
}

#[test]
fn empty_and_no_op_steps_record_nothing() {
    let start = ramp_table();
    let mut doc = Document::new(start.clone(), 10);
    let id = doc.change_id();

    doc.begin_step("Nothing").unwrap();
    doc.finish_step().unwrap();
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.change_id(), id);
    assert!(Wavetable::ptr_eq(doc.finished(), &start));
    assert!(!doc.is_modified());

    doc.begin_step("Same").unwrap();
    doc.update_step(|s| Ok(s.clone())).unwrap();
    doc.finish_step().unwrap();
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.change_id(), id);
    assert!(Wavetable::ptr_eq(doc.finished(), &start));
    assert!(!doc.is_modified());
}

#[test]
fn failed_update_keeps_the_step_open() {
    let start = ramp_table();
    let mut doc = Document::new(start.clone(), 10);
    doc.begin_step("Gain").unwrap();

    let bad = |s: &Wavetable| s.with_frame(0, 0, &[f32::NAN; 8]);
    assert!(matches!(
        doc.update_step(bad),
        Err(DocumentError::Edit(EditError::NonFiniteSample { .. }))
    ));
    assert!(Wavetable::ptr_eq(doc.current(), &start));
    assert!(doc.is_step_open());

    doc.update_step(|s| s.with_frame(0, 0, &gain(s, 2.0)))
        .unwrap();
    let good = doc.current().clone();
    assert!(matches!(
        doc.update_step(bad),
        Err(DocumentError::Edit(EditError::NonFiniteSample { .. }))
    ));
    assert!(Wavetable::ptr_eq(doc.current(), &good));
    assert!(doc.is_step_open());

    doc.update_step(|s| {
        assert!(Wavetable::ptr_eq(s, &start));
        s.with_frame(0, 0, &gain(s, 3.0))
    })
    .unwrap();
    doc.finish_step().unwrap();
    assert_eq!(doc.undo_label(), Some("Gain"));
    assert_eq!(
        bits(doc.finished().frame(0, 0).unwrap().samples()),
        bits(&gain(&start, 3.0))
    );
}

#[test]
fn failed_apply_changes_nothing() {
    let start = ramp_table();
    let mut doc = Document::new(start.clone(), 10);
    let id = doc.change_id();
    let result = doc.apply("Bad", |s| s.with_frame(0, 0, &[f32::NAN; 8]));
    assert!(matches!(
        result,
        Err(DocumentError::Edit(EditError::NonFiniteSample { .. }))
    ));
    assert!(!doc.is_step_open());
    assert!(Wavetable::ptr_eq(doc.current(), &start));
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.change_id(), id);
}

#[test]
fn apply_records_one_step() {
    let mut doc = Document::new(ramp_table(), 10);
    doc.apply("Gain", |s| s.with_frame(0, 0, &gain(s, 2.0)))
        .unwrap();
    assert_eq!(doc.undo_label(), Some("Gain"));
    assert!(doc.is_modified());
    assert!(!doc.is_step_open());
}

#[test]
fn open_step_guards() {
    let mut doc = Document::new(ramp_table(), 10);
    assert_eq!(
        doc.update_step(|s| Ok(s.clone())),
        Err(DocumentError::NoOpenStep)
    );
    assert_eq!(doc.finish_step(), Err(DocumentError::NoOpenStep));
    assert_eq!(doc.cancel_step(), Err(DocumentError::NoOpenStep));

    doc.begin_step("Gain").unwrap();
    doc.update_step(|s| s.with_frame(0, 0, &gain(s, 2.0)))
        .unwrap();
    let before = doc.current().clone();
    assert_eq!(doc.begin_step("Other"), Err(DocumentError::StepOpen));
    assert_eq!(
        doc.apply("Other", |s| Ok(s.clone())),
        Err(DocumentError::StepOpen)
    );
    assert!(doc.is_step_open());
    assert!(Wavetable::ptr_eq(doc.current(), &before));
}

/// One operation of the selection property test.
#[derive(Debug, Clone)]
enum SelOp {
    /// A whole step that inserts (`true`) or removes (`false`) one row or column.
    Apply(bool, bool, u32),
    Begin,
    /// An update inside an open step that inserts or removes.
    Update(bool, bool, u32),
    Finish,
    Cancel,
    Undo,
    Redo,
    Select(u32, u32, u32, u32),
}

fn sel_op() -> impl Strategy<Value = SelOp> {
    prop_oneof![
        (any::<bool>(), any::<bool>(), any::<u32>()).prop_map(|(a, b, c)| SelOp::Apply(a, b, c)),
        Just(SelOp::Begin),
        (any::<bool>(), any::<bool>(), any::<u32>()).prop_map(|(a, b, c)| SelOp::Update(a, b, c)),
        Just(SelOp::Finish),
        Just(SelOp::Cancel),
        Just(SelOp::Undo),
        Just(SelOp::Redo),
        (any::<u32>(), any::<u32>(), any::<u32>(), any::<u32>())
            .prop_map(|(a, b, c, d)| SelOp::Select(a, b, c, d)),
    ]
}

/// Inserts or removes one row or column of `t`; `at` is reduced to a valid position.
fn grid_edit(t: &Wavetable, insert: bool, y: bool, at: u32) -> Result<Wavetable, EditError> {
    let axis = if y { Axis::Y } else { Axis::Z };
    let size = if y { t.y_size() } else { t.z_size() };
    if insert {
        t.insert(axis, at % (size + 1), 1)
    } else if size == 1 {
        Ok(t.clone())
    } else {
        t.remove(axis, at % size, 1)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// AC10: whatever happens, the selection lies inside the current grid.
    #[test]
    fn selection_always_fits(
        start in tuw3_table(4),
        (a, b, c, d) in (any::<u32>(), any::<u32>(), any::<u32>(), any::<u32>()),
        ops in proptest::collection::vec(sel_op(), 1..=20),
    ) {
        let t = Wavetable::from(start);
        let mut doc = Document::new(t.clone(), 50);
        let first = Selection::rect(&t, (a % t.y_size(), b % t.z_size()), (c % t.y_size(), d % t.z_size())).unwrap();
        doc.set_selection(first);
        for op in ops {
            // Refusals (a step open or not) are fine here; only the selection is checked.
            let _ = match op {
                SelOp::Apply(i, y, at) => doc.apply("edit", |t| grid_edit(t, i, y, at)),
                SelOp::Begin => doc.begin_step("edit"),
                SelOp::Update(i, y, at) => doc.update_step(|t| grid_edit(t, i, y, at)),
                SelOp::Finish => doc.finish_step(),
                SelOp::Cancel => doc.cancel_step(),
                SelOp::Undo => doc.undo(),
                SelOp::Redo => doc.redo(),
                SelOp::Select(a, b, c, d) => {
                    let cur = doc.current();
                    let (ys, zs) = (cur.y_size(), cur.z_size());
                    if let Ok(s) = Selection::rect(cur, (a % ys, b % zs), (c % ys, d % zs)) {
                        doc.set_selection(s);
                    }
                    Ok(())
                }
            };
            let sel = doc.selection();
            prop_assert!(*sel.y_range().end() < doc.current().y_size());
            prop_assert!(*sel.z_range().end() < doc.current().z_size());
        }
    }
}

#[test]
fn selection_stays_when_frames_are_replaced() {
    let t = table(3, 3);
    let mut doc = Document::new(t.clone(), 10);
    let one = Selection::frame(&t, 1, 1).unwrap();
    doc.set_selection(one);
    doc.apply("Fill", |s| s.with_frame(0, 0, &[0.5; 8]))
        .unwrap();
    assert_eq!(doc.selection(), one);
    doc.undo().unwrap();
    assert_eq!(doc.selection(), one);
}

#[test]
fn cancel_after_removing_rows_keeps_the_selection() {
    let t = table(4, 1);
    let mut doc = Document::new(t.clone(), 10);
    doc.begin_step("Remove").unwrap();
    doc.update_step(|s| s.remove(Axis::Y, 1, 2)).unwrap();
    assert_eq!(doc.selection(), Selection::all(doc.current()));
    doc.cancel_step().unwrap();
    // The restored grid is bigger, so the shrunken selection stays as it is.
    assert_eq!(doc.selection(), Selection::all(&table(2, 1)));
}

#[test]
fn selection_is_not_history() {
    let t = table(3, 3);
    let mut doc = Document::new(t.clone(), 10);
    // Selecting on a fresh document does not make it modified.
    doc.set_selection(Selection::frame(&t, 0, 0).unwrap());
    assert!(!doc.is_modified());
    doc.set_selection(Selection::all(&t));
    doc.apply("Fill", |s| s.with_frame(0, 0, &[0.5; 8]))
        .unwrap();
    let id = doc.change_id();
    let finished = doc.finished().clone();
    let modified = doc.is_modified();
    let before = Selection::all(doc.current());

    let one = Selection::frame(doc.current(), 1, 2).unwrap();
    doc.set_selection(one);
    assert_eq!(doc.undo_label(), Some("Fill"));
    assert_eq!(doc.change_id(), id);
    assert!(Wavetable::ptr_eq(doc.finished(), &finished));
    assert_eq!(doc.is_modified(), modified);

    // Undo does not bring back the earlier selection.
    assert_ne!(before, one);
    doc.undo().unwrap();
    assert_eq!(doc.selection(), one);
}

#[test]
fn finished_version_and_change_id() {
    let start = ramp_table();
    let mut doc = Document::new(start.clone(), 10);
    let mut ids = HashSet::new();
    let first = doc.change_id();
    assert!(ids.insert(first));

    doc.begin_step("Gain").unwrap();
    for g in [1.5_f32, 2.0] {
        doc.update_step(|s| s.with_frame(0, 0, &gain(s, g)))
            .unwrap();
        assert!(Wavetable::ptr_eq(doc.finished(), &start));
        assert_eq!(doc.change_id(), first);
    }
    let result = doc.current().clone();
    doc.finish_step().unwrap();
    assert!(ids.insert(doc.change_id()));
    assert!(Wavetable::ptr_eq(doc.finished(), &result));

    doc.undo().unwrap();
    assert!(ids.insert(doc.change_id()));
    assert!(Wavetable::ptr_eq(doc.finished(), &start));

    doc.redo().unwrap();
    assert!(ids.insert(doc.change_id()));
    assert!(Wavetable::ptr_eq(doc.finished(), &result));
}

fn gain_step(doc: &mut Document) {
    doc.apply("Gain", |s| s.with_frame(0, 0, &gain(s, 2.0)))
        .unwrap();
}

#[test]
fn modified_follows_the_saved_state() {
    let mut doc = Document::new(ramp_table(), 10);
    assert!(!doc.is_modified());
    gain_step(&mut doc);
    assert!(doc.is_modified());
    doc.undo().unwrap();
    assert!(!doc.is_modified());
    doc.redo().unwrap();
    assert!(doc.is_modified());
    doc.mark_saved();
    assert!(!doc.is_modified());
    doc.undo().unwrap();
    assert!(doc.is_modified());
    doc.redo().unwrap();
    assert!(!doc.is_modified());

    // An open step does not count: `is_modified` reports the finished version.
    doc.begin_step("Gain").unwrap();
    doc.update_step(|s| s.with_frame(0, 0, &gain(s, 3.0)))
        .unwrap();
    assert!(!doc.is_modified());
}

#[test]
fn saved_state_dropped_by_the_limit() {
    let mut doc = Document::new(ramp_table(), 1);
    doc.mark_saved();
    gain_step(&mut doc);
    gain_step(&mut doc);
    doc.undo().unwrap();
    assert!(doc.is_modified());
    doc.redo().unwrap();
    assert!(doc.is_modified());
    doc.mark_saved();
    assert!(!doc.is_modified());
}

#[test]
fn saved_state_is_by_serial() {
    let mut doc = Document::new(ramp_table(), 10);
    gain_step(&mut doc); // A
    doc.mark_saved();
    doc.undo().unwrap();
    doc.apply("Other", |s| s.with_frame(0, 0, &gain(s, 3.0)))
        .unwrap(); // B
    assert!(doc.is_modified());
    doc.undo().unwrap();
    assert!(doc.is_modified());
    doc.redo().unwrap();
    assert!(doc.is_modified());
}
