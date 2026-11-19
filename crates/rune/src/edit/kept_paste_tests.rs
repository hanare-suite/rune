//! What a word processor loses when the field deletes, copies and pastes whole paragraphs,
//! each a test written, and seen failing, before the code:
//!
//! 1. A deletion that takes a paragraph whole, from its start to the start of the next, leaves
//!    the deleted paragraph's attributes, block and id on the one that remains.
//! 2. A copy that ends behind a line break pastes without it, so a paragraph copied whole
//!    joins the paragraph it is pasted before.
//! 3. An atom copies as a paragraph of its label, so a paste makes text of it.
//! 4. An app cannot see or change what a paste will insert: it cannot drop what it has no
//!    content for, or give a pasted pill an id of its own.

use super::fuzz_tests::{check, seen};
use super::*;

fn lines(e: &TextEdit) -> Vec<&str> {
    e.text.split('\n').collect()
}

fn ids(e: &TextEdit) -> Vec<u64> {
    e.paragraph_attrs().iter().map(|a| a.id).collect()
}

fn atoms(e: &TextEdit) -> Vec<bool> {
    e.paragraph_attrs().iter().map(|a| a.atom.is_some()).collect()
}

/// `text` with ids from 1, the lines `atom` names locked, the next id 10, its history off.
fn field(text: &str, atom: &[usize]) -> TextEdit {
    let mut e = TextEdit::from_parts(text, vec![], vec![]);
    let d = ParagraphAttrs::default();
    let n = text.split('\n').count();
    e.set_paragraph_attrs((0..n).map(|i| if atom.contains(&i) { d.id(i as u64 + 1).atom(40.0) } else { d.id(i as u64 + 1) }).collect());
    e.set_paragraph_ids(10);
    e.set_history(false);
    e.take_changes();
    e
}

/// Runs `f` and checks the changes it reported against what it did.
fn reported(e: &mut TextEdit, f: impl FnOnce(&mut TextEdit)) -> Vec<EditChange> {
    let before = seen(e);
    f(e);
    let changes = e.take_changes();
    if let Err(why) = check(&before, e, &changes) {
        panic!("{why}\n text {:?}\n changes {changes:?}", e.text);
    }
    changes
}

#[test]
fn a_paragraph_deleted_whole_leaves_the_next_one_its_attributes_block_and_id() {
    let mut e = TextEdit::from_parts("eins\nzwei\ndrei", vec![], vec![Block::Paragraph, Block::Numbered(1), Block::Paragraph]);
    let d = ParagraphAttrs::default();
    e.set_paragraph_attrs(vec![d.id(1).style(1), d.id(2).style(2).align(ParagraphAlign::Center), d.id(3)]);
    e.set_paragraph_ids(10);
    e.set_history(false);
    e.take_changes();
    reported(&mut e, |e| {
        e.set_selection(0, 5);
        e.insert("");
    });
    assert_eq!(lines(&e), ["zwei", "drei"]);
    assert_eq!(e.paragraph(0), d.id(2).style(2).align(ParagraphAlign::Center));
    assert_eq!(e.blocks(), [Block::Numbered(1), Block::Paragraph]);
}

#[test]
fn a_paragraph_deleted_whole_by_backspace_does_the_same() {
    let mut e = field("eins\nzwei", &[]);
    e.set_selection(0, 5);
    reported(&mut e, |e| {
        e.key(&KeyEvent { key: Key::Backspace, mods: Mods::default() }, None);
    });
    assert_eq!((lines(&e), ids(&e)), (vec!["zwei"], vec![2]));
}

#[test]
fn a_join_inside_the_first_paragraph_still_keeps_the_first() {
    let mut e = field("eins\nzwei", &[]);
    reported(&mut e, |e| {
        e.set_selection(2, 5);
        e.insert("");
    });
    assert_eq!((lines(&e), ids(&e)), (vec!["eizwei"], vec![1]));
    let mut e = field("eins\nzwei", &[]);
    reported(&mut e, |e| {
        e.set_selection(4, 5);
        e.insert("");
    });
    assert_eq!((lines(&e), ids(&e)), (vec!["einszwei"], vec![1]));
}

#[test]
fn text_typed_over_a_whole_paragraph_keeps_that_paragraph() {
    let mut e = field("eins\nzwei", &[]);
    reported(&mut e, |e| {
        e.set_selection(0, 5);
        e.insert("x");
    });
    assert_eq!((lines(&e), ids(&e)), (vec!["xzwei"], vec![1]));
}

#[test]
fn a_copy_that_ends_behind_a_line_break_pastes_as_a_paragraph_of_its_own() {
    let mut e = field("eins\nzwei", &[]);
    e.set_selection(0, 5);
    let html = e.selected_html();
    reported(&mut e, |e| {
        e.set_caret(5);
        e.paste_html(&html, "eins\n");
    });
    assert_eq!(lines(&e), ["eins", "eins", "zwei"], "{html}");
    assert_eq!(e.range(), (10, 10), "the caret is behind the pasted break");
}

#[test]
fn a_copy_with_its_line_break_keeps_its_formatting_and_leaves_the_line_it_lands_before() {
    let mut e = TextEdit::from_parts("eins\nzwei", vec![MarkRange::new(0..4, Mark::Bold)], vec![Block::Paragraph, Block::Bullet(0)]);
    let d = ParagraphAttrs::default();
    e.set_paragraph_attrs(vec![d.id(1).align(ParagraphAlign::Center), d.id(2)]);
    e.set_paragraph_ids(10);
    e.set_history(false);
    e.set_selection(0, 5);
    let html = e.selected_html();
    e.take_changes();
    reported(&mut e, |e| {
        e.set_caret(5);
        e.paste_html(&html, "eins\n");
    });
    assert_eq!(lines(&e), ["eins", "eins", "zwei"], "{html}");
    assert_eq!(e.blocks(), [Block::Paragraph, Block::Paragraph, Block::Bullet(0)], "the bullet stays a bullet");
    assert_eq!(e.paragraph(1).align, ParagraphAlign::Center);
    assert_eq!(e.paragraph(2).id, 2);
    assert!(e.marks().iter().any(|m| m.mark == Mark::Bold && m.range() == (5..9)), "{:?}", e.marks());
}

#[test]
fn a_copy_without_a_trailing_break_still_joins_the_line_it_lands_in() {
    let mut e = field("eins\nzwei", &[]);
    e.set_selection(0, 4);
    let html = TextEdit::from_parts("eins", vec![MarkRange::new(0..4, Mark::Bold)], vec![]).to_html();
    reported(&mut e, |e| {
        e.set_caret(5);
        e.paste_html(&html, "eins");
    });
    assert_eq!(lines(&e), ["eins", "einszwei"]);
}

#[test]
fn an_atom_copies_with_its_id_and_height_and_a_fragment_shows_them() {
    let mut e = field("a\nTabelle\nb", &[1]);
    e.set_selection(2, 9);
    let html = e.selected_html();
    assert!(html.contains("data-atom=\"2\""), "{html}");
    let frag = e.html_fragment(&html);
    assert_eq!(frag.text, "Tabelle");
    assert_eq!((frag.paragraph(0).atom, frag.paragraph(0).id), (Some(40.0), 2));
}

#[test]
fn a_fragment_with_an_atom_pastes_the_atom_on_a_line_of_its_own() {
    for (caret, want, locked) in [
        (12, vec!["a", "Tabelle", "bc", "Tabelle"], vec![false, true, false, true]),
        (11, vec!["a", "Tabelle", "b", "Tabelle", "c"], vec![false, true, false, true, false]),
        (10, vec!["a", "Tabelle", "Tabelle", "bc"], vec![false, true, true, false]),
        (9, vec!["a", "Tabelle", "Tabelle", "bc"], vec![false, true, true, false]),
        (0, vec!["Tabelle", "a", "Tabelle", "bc"], vec![true, false, true, false]),
    ] {
        let mut e = field("a\nTabelle\nbc", &[1]);
        e.set_selection(2, 9);
        let frag = e.html_fragment(&e.selected_html());
        reported(&mut e, |e| {
            e.set_caret(caret);
            e.paste_fragment(frag, "");
        });
        assert_eq!((lines(&e), atoms(&e)), (want, locked), "pasted at {caret}");
        let mut seen = ids(&e);
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), ids(&e).len(), "pasted at {caret}: every paragraph has an id of its own: {:?}", ids(&e));
        assert!(!ids(&e).contains(&0), "pasted at {caret}: {:?}", ids(&e));
    }
}

#[test]
fn a_pasted_atom_is_an_atom_to_the_keys() {
    let mut e = field("a\nTabelle\nbc", &[1]);
    e.set_selection(2, 9);
    let frag = e.html_fragment(&e.selected_html());
    e.set_caret(12);
    e.paste_fragment(frag, "");
    e.take_changes();
    // Typing at the pasted atom's end goes on a line of its own, as beside any atom.
    reported(&mut e, |e| e.insert("x"));
    assert_eq!(lines(&e), ["a", "Tabelle", "bc", "Tabelle", "x"]);
}

#[test]
fn paste_html_makes_no_atom_of_a_copied_one() {
    let mut e = field("a\nTabelle\nb", &[1]);
    e.set_selection(2, 9);
    let html = e.selected_html();
    reported(&mut e, |e| {
        e.set_caret(11);
        e.paste_html_with_pills(&html, "Tabelle");
    });
    assert_eq!(atoms(&e).iter().filter(|a| **a).count(), 1, "{:?}", lines(&e));
}

#[test]
fn an_app_can_change_a_fragment_before_it_pastes_it() {
    let mut e = TextEdit::from_parts("a @x b", vec![MarkRange::new(2..4, Mark::Pill("old".into()))], vec![]);
    e.set_selection(0, 6);
    let mut frag = e.html_fragment(&e.selected_html());
    assert_eq!(frag.marks().iter().filter_map(|m| m.mark.pill_id()).collect::<Vec<_>>(), ["old"]);
    let renamed: Vec<MarkRange> = frag.marks().iter().map(|m| if m.mark.pill_id().is_some() { MarkRange::new(m.range(), Mark::Pill("new".into())) } else { m.clone() }).collect();
    frag.set_marks(renamed);
    e.set_caret(6);
    e.paste_fragment(frag, "");
    let pills: Vec<(&str, Range<usize>)> = e.marks().iter().filter_map(|m| m.mark.pill_id().map(|id| (id, m.range()))).collect();
    assert_eq!(pills, [("old", 2..4), ("new", 8..10)]);
    assert_eq!(e.text, "a @x ba @x b");
}
