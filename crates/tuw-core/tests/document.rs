// SPDX-License-Identifier: GPL-3.0-or-later

//! The undo/redo document: its starting state, ids and selection (AC10, AC11, AC12).

use tuw_core::{Document, DocumentError, EditError, Selection, Wavetable};

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
