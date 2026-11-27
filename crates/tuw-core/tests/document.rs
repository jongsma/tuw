// SPDX-License-Identifier: GPL-3.0-or-later

//! The undo/redo document: its starting state, ids and selection (AC10, AC11, AC12).

use tuw_core::{Document, Selection, Wavetable};

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
