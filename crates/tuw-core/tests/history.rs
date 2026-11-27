// SPDX-License-Identifier: GPL-3.0-or-later

//! Undo, redo and the history limit (AC1, AC3, AC5 to AC9).

mod common;

use common::{assert_same, finite_f32, tuw3_table};
use proptest::prelude::*;
use tuw_core::{Axis, DEFAULT_HISTORY_LIMIT, Document, DocumentError, EditError, Wavetable};

/// One step of the AC1 property test.
#[derive(Debug, Clone)]
enum Op {
    Replace(u32, u32, Vec<f32>),
    Insert(bool, u32, u32),
    Remove(bool, u32),
    Name(String),
    Undo,
    Redo,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (
            any::<u32>(),
            any::<u32>(),
            proptest::collection::vec(finite_f32(), 600)
        )
            .prop_map(|(y, z, s)| Op::Replace(y, z, s)),
        (any::<bool>(), any::<u32>(), 1u32..=2).prop_map(|(y, at, n)| Op::Insert(y, at, n)),
        (any::<bool>(), any::<u32>()).prop_map(|(y, at)| Op::Remove(y, at)),
        ".*".prop_map(Op::Name),
        Just(Op::Undo),
        Just(Op::Redo),
    ]
}

fn axis(y: bool) -> Axis {
    if y { Axis::Y } else { Axis::Z }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// AC1: undo and redo walk a linear history exactly like a list of deep snapshots.
    #[test]
    fn undo_and_redo_match_a_reference(
        start in tuw3_table(3),
        ops in proptest::collection::vec(op(), 1..=40),
    ) {
        let mut doc = Document::new(Wavetable::from(start.clone()), DEFAULT_HISTORY_LIMIT);
        let mut reference = vec![start];
        let mut cursor = 0usize;
        for op in ops {
            // The edit's result is snapshotted deeply for the reference.
            let mut snapshot = None;
            let result = match op {
                Op::Undo => {
                    let r = doc.undo();
                    if cursor == 0 {
                        prop_assert_eq!(r, Err(DocumentError::NothingToUndo));
                    } else {
                        prop_assert_eq!(r, Ok(()));
                        cursor -= 1;
                    }
                    None
                }
                Op::Redo => {
                    let r = doc.redo();
                    if cursor + 1 == reference.len() {
                        prop_assert_eq!(r, Err(DocumentError::NothingToRedo));
                    } else {
                        prop_assert_eq!(r, Ok(()));
                        cursor += 1;
                    }
                    None
                }
                Op::Replace(y, z, samples) => Some(doc.apply("edit", |t| {
                    let n = t.frame_size() as usize;
                    let r = t.with_frame(y % t.y_size(), z % t.z_size(), &samples[..n])?;
                    snapshot = Some(r.to_tuw3());
                    Ok(r)
                })),
                Op::Insert(y, at, n) => Some(doc.apply("edit", |t| {
                    let size = if y { t.y_size() } else { t.z_size() };
                    let r = t.insert(axis(y), at % (size + 1), n)?;
                    snapshot = Some(r.to_tuw3());
                    Ok(r)
                })),
                Op::Remove(y, at) => Some(doc.apply("edit", |t| {
                    let size = if y { t.y_size() } else { t.z_size() };
                    if size == 1 {
                        // Skipped: leave the table as it is, which records nothing.
                        return Ok(t.clone());
                    }
                    let r = t.remove(axis(y), at % size, 1)?;
                    snapshot = Some(r.to_tuw3());
                    Ok(r)
                })),
                Op::Name(name) => Some(doc.apply("edit", |t| {
                    let mut metadata = t.metadata().clone();
                    metadata.name = Some(name);
                    let r = t.with_metadata(metadata)?;
                    snapshot = Some(r.to_tuw3());
                    Ok(r)
                })),
            };
            if let Some(result) = result {
                prop_assert_eq!(result, Ok(()));
                if let Some(table) = snapshot {
                    reference.truncate(cursor + 1);
                    reference.push(table);
                    cursor += 1;
                }
            }
            assert_same(&doc.current().to_tuw3(), &reference[cursor]);
        }
    }
}

fn table() -> Wavetable {
    Wavetable::silent(8, 2, 2).unwrap()
}

/// A step that always makes a new version.
fn step(doc: &mut Document, label: &str, value: f32) {
    doc.apply(label, |t| t.with_frame(0, 0, &[value; 8]))
        .unwrap();
}

#[test]
fn a_gesture_undoes_in_one() {
    let start = table();
    let mut doc = Document::new(start.clone(), 10);
    doc.begin_step("Drag").unwrap();
    for i in 0..50 {
        doc.update_step(|t| t.with_frame(0, 0, &[i as f32 / 100.0; 8]))
            .unwrap();
    }
    doc.finish_step().unwrap();
    let finished = doc.current().clone();
    doc.undo().unwrap();
    assert!(Wavetable::ptr_eq(doc.current(), &start));
    doc.redo().unwrap();
    assert!(Wavetable::ptr_eq(doc.current(), &finished));
    doc.undo().unwrap();
    assert_eq!(doc.undo(), Err(DocumentError::NothingToUndo));
}

#[test]
fn undo_and_redo_refused_while_open() {
    let mut doc = Document::new(table(), 10);
    step(&mut doc, "A", 0.5);
    doc.undo().unwrap();
    doc.begin_step("B").unwrap();
    doc.update_step(|t| t.with_frame(0, 0, &[0.25; 8])).unwrap();
    let before = doc.current().to_tuw3();
    let finished = doc.finished().clone();
    assert_eq!(doc.undo(), Err(DocumentError::StepOpen));
    assert!(Wavetable::ptr_eq(doc.finished(), &finished));
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.redo_label(), Some("A"));
    assert_eq!(doc.redo(), Err(DocumentError::StepOpen));
    assert!(Wavetable::ptr_eq(doc.finished(), &finished));
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.redo_label(), Some("A"));
    assert_same(&doc.current().to_tuw3(), &before);
    assert!(doc.is_step_open());
}

#[test]
fn a_new_step_drops_redo() {
    let mut doc = Document::new(table(), 10);
    step(&mut doc, "A", 0.1);
    step(&mut doc, "B", 0.2);
    doc.undo().unwrap();
    step(&mut doc, "C", 0.3);
    assert_eq!(doc.redo(), Err(DocumentError::NothingToRedo));
    assert_eq!(doc.undo_label(), Some("C"));
}

#[test]
fn history_limit_drops_oldest() {
    for limit in [0usize, 1, 3] {
        let mut doc = Document::new(table(), limit);
        let mut snapshots = Vec::new();
        for i in 0..limit + 2 {
            step(&mut doc, &format!("S{i}"), (i + 1) as f32 / 10.0);
            snapshots.push(doc.current().to_tuw3());
        }
        for _ in 0..limit {
            doc.undo().unwrap();
        }
        assert_eq!(doc.undo(), Err(DocumentError::NothingToUndo));
        // The oldest reachable version is the one after step 2 (index 1); with limit 0 it is
        // also the last step, the only state left.
        assert_same(&doc.current().to_tuw3(), &snapshots[1]);
        assert_eq!(doc.undo_label(), None);
    }
}

#[test]
fn history_limit_usize_max() {
    let mut doc = Document::new(table(), usize::MAX);
    for i in 0..3 {
        step(&mut doc, "S", (i + 1) as f32 / 10.0);
    }
    for _ in 0..3 {
        doc.undo().unwrap();
    }
    assert_eq!(doc.undo(), Err(DocumentError::NothingToUndo));
}

#[test]
fn labels_follow_the_cursor() {
    let mut doc = Document::new(table(), 10);
    step(&mut doc, "A", 0.1);
    step(&mut doc, "B", 0.2);
    assert_eq!(doc.undo_label(), Some("B"));
    assert_eq!(doc.redo_label(), None);
    doc.undo().unwrap();
    assert_eq!(doc.undo_label(), Some("A"));
    assert_eq!(doc.redo_label(), Some("B"));
    doc.undo().unwrap();
    assert_eq!(doc.undo_label(), None);
    assert_eq!(doc.redo_label(), Some("A"));
}

#[test]
fn failed_apply_keeps_redo() {
    let mut doc = Document::new(table(), 10);
    step(&mut doc, "A", 0.1);
    step(&mut doc, "B", 0.2);
    doc.undo().unwrap();
    let r = doc.apply("bad", |t| t.with_frame(9, 9, &[0.0; 8]));
    assert!(matches!(
        r,
        Err(DocumentError::Edit(EditError::OutsideGrid { .. }))
    ));
    doc.redo().unwrap();
}
