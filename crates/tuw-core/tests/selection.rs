// SPDX-License-Identifier: GPL-3.0-or-later

//! The rectangular selection and how it follows inserts and removes (AC6).

mod common;

use common::tuw3_table;
use proptest::prelude::*;
use tuw_core::{Axis, EditError, Selection, Wavetable};

fn table(y: u32, z: u32) -> Wavetable {
    Wavetable::silent(8, y, z).unwrap()
}

/// A selection with the given Y range on a 6 × 5 table (all Z).
fn y_sel(s: u32, e: u32) -> Selection {
    Selection::rect(&table(6, 5), (s, 0), (e, 4)).unwrap()
}

#[test]
fn constructors_give_the_expected_rectangle() {
    let t = table(6, 5);
    let f = Selection::frame(&t, 2, 3).unwrap();
    assert_eq!((f.y_range(), f.z_range()), (2..=2, 3..=3));
    let r = Selection::row(&t, 4).unwrap();
    assert_eq!((r.y_range(), r.z_range()), (0..=5, 4..=4));
    let c = Selection::column(&t, 1).unwrap();
    assert_eq!((c.y_range(), c.z_range()), (1..=1, 0..=4));
    let b = Selection::rect(&t, (4, 1), (2, 3)).unwrap();
    assert_eq!((b.y_range(), b.z_range()), (2..=4, 1..=3));
    let a = Selection::all(&t);
    assert_eq!((a.y_range(), a.z_range()), (0..=5, 0..=4));
    assert!(b.contains(3, 2) && !b.contains(5, 2) && !b.contains(3, 4));
    assert_eq!(
        Selection::frame(&t, 6, 0),
        Err(EditError::OutsideGrid { y: 6, z: 0 })
    );
    assert_eq!(
        Selection::row(&t, 5),
        Err(EditError::IndexOutOfRange {
            axis: Axis::Z,
            index: 5,
            size: 5
        })
    );
}

#[test]
fn insert_before_inside_and_after() {
    let t = table(6, 5);
    let after = t.insert(Axis::Y, 0, 2).unwrap();
    let sel = y_sel(2, 4);
    for (at, expected) in [(0, 4..=6), (2, 4..=6), (3, 2..=6), (4, 2..=6), (5, 2..=4)] {
        let s = sel.after_insert(Axis::Y, at, 2, &after);
        assert_eq!(s.y_range(), expected, "at {at}");
        assert_eq!(s.z_range(), 0..=4);
    }
}

#[test]
fn remove_partial_and_full() {
    let sel = y_sel(2, 4);
    let cases = [
        (0, 3, 0..=1),
        (3, 2, 2..=2),
        (4, 2, 2..=3),
        (2, 3, 2..=2),
        (1, 5, 0..=0),
        (0, 1, 1..=3),
        (5, 1, 2..=4),
        (3, 1, 2..=3),
    ];
    for (at, count, expected) in cases {
        let after = table(6, 5).remove(Axis::Y, at, count).unwrap();
        let s = sel.after_remove(Axis::Y, at, count, &after);
        assert_eq!(s.y_range(), expected, "remove ({at}, {count})");
    }
    let t = table(6, 5);
    let z_sel = Selection::rect(&t, (0, 1), (5, 3)).unwrap();
    let after = t.remove(Axis::Z, 0, 1).unwrap();
    let s = z_sel.after_remove(Axis::Z, 0, 1, &after);
    assert_eq!((s.y_range(), s.z_range()), (0..=5, 0..=2));
}

#[test]
fn fit_clamps_into_any_grid() {
    let sel = y_sel(2, 4);
    assert_eq!(sel.fit(&table(3, 5)).y_range(), 2..=2);
    assert_eq!(sel.fit(&table(1, 5)).y_range(), 0..=0);
}

/// Reference model: map each selected index of one axis, return `(min, max)`.
fn model_insert(s: u32, e: u32, at: u32, count: u32) -> (u32, u32) {
    let mut set: Vec<u32> = (s..=e)
        .map(|i| if i >= at { i + count } else { i })
        .collect();
    if s < at && at <= e {
        set.extend(at..at + count); // inserted indices strictly inside join
    }
    (*set.iter().min().unwrap(), *set.iter().max().unwrap())
}

fn model_remove(s: u32, e: u32, at: u32, count: u32, size_after: u32) -> (u32, u32) {
    let set: Vec<u32> = (s..=e)
        .filter(|i| !(at..at + count).contains(i))
        .map(|i| if i >= at + count { i - count } else { i })
        .collect();
    match (set.iter().min(), set.iter().max()) {
        (Some(a), Some(b)) => (*a, *b),
        _ => (s.min(size_after - 1), s.min(size_after - 1)),
    }
}

proptest! {
    #[test]
    fn adjusted_selections_match_a_reference_model(
        t in tuw3_table(6),
        corners in (0u32..6, 0u32..6, 0u32..6, 0u32..6),
        y_axis in any::<bool>(),
        insert in any::<bool>(),
        at_seed in 0u32..8,
        count_seed in 1u32..4,
    ) {
        let t = Wavetable::from(t);
        let axis = if y_axis { Axis::Y } else { Axis::Z };
        let size = if y_axis { t.y_size() } else { t.z_size() };
        let sel = Selection::rect(
            &t,
            (corners.0 % t.y_size(), corners.1 % t.z_size()),
            (corners.2 % t.y_size(), corners.3 % t.z_size()),
        ).unwrap();
        let (s, e, other) = if y_axis {
            (*sel.y_range().start(), *sel.y_range().end(), sel.z_range())
        } else {
            (*sel.z_range().start(), *sel.z_range().end(), sel.y_range())
        };
        let (after, expected, at, count);
        if insert {
            at = at_seed % (size + 1);
            count = count_seed;
            after = t.insert(axis, at, count).unwrap();
            expected = model_insert(s, e, at, count);
        } else if size > 1 {
            count = 1 + count_seed % (size - 1);
            at = at_seed % (size - count + 1);
            after = t.remove(axis, at, count).unwrap();
            expected = model_remove(s, e, at, count, size - count);
        } else {
            return Ok(());
        }
        let next = if insert {
            sel.after_insert(axis, at, count, &after)
        } else {
            sel.after_remove(axis, at, count, &after)
        };
        let (ns, ne, nother) = if y_axis {
            (*next.y_range().start(), *next.y_range().end(), next.z_range())
        } else {
            (*next.z_range().start(), *next.z_range().end(), next.y_range())
        };
        prop_assert_eq!((ns, ne), expected);
        prop_assert_eq!(nother, other);
        prop_assert_eq!(next.fit(&after), next);
        let size_after = if y_axis { after.y_size() } else { after.z_size() };
        prop_assert!(ne < size_after);
    }

    #[test]
    fn fit_of_any_selection_lies_inside_any_table(
        a in tuw3_table(6),
        b in tuw3_table(6),
        corners in (0u32..6, 0u32..6, 0u32..6, 0u32..6),
    ) {
        let (a, b) = (Wavetable::from(a), Wavetable::from(b));
        let sel = Selection::rect(
            &a,
            (corners.0 % a.y_size(), corners.1 % a.z_size()),
            (corners.2 % a.y_size(), corners.3 % a.z_size()),
        ).unwrap();
        let fitted = sel.fit(&b);
        prop_assert!(*fitted.y_range().end() < b.y_size());
        prop_assert!(*fitted.z_range().end() < b.z_size());
        prop_assert!(fitted.y_range().start() <= fitted.y_range().end());
    }
}
