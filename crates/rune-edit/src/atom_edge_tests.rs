//! More ways atoms, ids and the change report can go wrong at a paragraph's edge, each a test
//! written, and seen failing, before the code:
//!
//! 1. A rich paste over a selected last atom inserts where the atom was, past the text's end.
//! 2. A rich paste at an atom's edge puts its marks and blocks a byte and a line off.
//! 3. Text typed over an empty paragraph's line break before an atom copies the atom's id.
//! 4. Text typed over an atom and up to the next atom's start copies the next atom's id.
//! 5. `replace_text` leaves every paragraph with id 0 while ids are on.
//! 6. Backspace after an atom, or Delete before one, does nothing, so a keyboard cannot
//!    reach the atom from the paragraph beside it.
//! 7. That first press changes the text, or the second deletes more than the atom.
//! 8. A rich paste over a selected atom takes the atom's line with it and joins the paragraph
//!    beside it, where typed text and a plain paste keep the line.
//! 9. Text that replaces an atom keeps the atom's id, so an app that keys an atom's content by
//!    id takes the text for the atom: typed or pasted over one atom, over a run of them, over
//!    atoms up to the start of the next, or from inside a label.

use super::fuzz_tests::{check, seen};
use super::*;

fn k(key: Key) -> KeyEvent {
    KeyEvent { key, mods: Mods::default() }
}

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

const RICH: &str = "<p><b>X</b>y</p><ul><li>L1</li><li>L2</li></ul>";

#[test]
fn a_rich_paste_over_the_last_atom_lands_where_the_caret_did() {
    let mut e = field("before\nTabelle", &[1]);
    e.set_selection(7, 14);
    reported(&mut e, |e| { e.paste_html("<b>X</b>y", "Xy"); });
    assert_eq!((lines(&e), ids(&e), atoms(&e)), (vec!["before", "Xy"], vec![1, 10], vec![false, false]));
    assert_eq!(e.marks(), [MarkRange::new(7..8, Mark::Bold)]);
    assert_eq!(e.focus, 9);
}

#[test]
fn a_rich_paste_over_a_selected_atom_keeps_its_line_as_typing_does() {
    let over = |f: fn(&mut TextEdit)| {
        let mut e = field("before\nTabelle\nafter", &[1]);
        e.set_selection(7, 14);
        reported(&mut e, f);
        (e.text.clone(), ids(&e), atoms(&e), e.focus)
    };
    let typed = over(|e| { e.key(&k(Key::Text("Xy".into())), None); });
    let plain = over(|e| { e.paste("Xy"); });
    let rich = over(|e| { e.paste_html("<b>X</b>y", "Xy"); });
    assert_eq!(typed, ("before\nXy\nafter".to_string(), vec![1, 10, 3], vec![false; 3], 9));
    assert_eq!((&plain, &rich), (&typed, &typed));
}

#[test]
fn a_rich_paste_of_paragraphs_over_a_selected_atom_stays_between_its_neighbours() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_selection(7, 14);
    reported(&mut e, |e| { e.paste_html(RICH, "Xy\nL1\nL2"); });
    assert_eq!(lines(&e), ["before", "Xy", "L1", "L2", "after"]);
    assert_eq!(e.marks(), [MarkRange::new(7..8, Mark::Bold)]);
    let p = Block::Paragraph;
    assert_eq!(e.blocks(), [p, p, Block::Bullet(0), Block::Bullet(0), p]);
    assert_eq!((ids(&e), atoms(&e)), (vec![1, 10, 11, 12, 3], vec![false; 5]));
}

#[test]
fn a_rich_paste_over_a_run_of_atoms_leaves_one_line() {
    let mut e = field("before\nTabelle\nBild\nafter", &[1, 2]);
    e.set_selection(7, 19);
    reported(&mut e, |e| { e.paste_html("<b>X</b>y", "Xy"); });
    assert_eq!((lines(&e), ids(&e), atoms(&e)), (vec!["before", "Xy", "after"], vec![1, 10, 4], vec![false; 3]));
}

#[test]
fn a_rich_paste_after_an_atom_keeps_its_marks_and_blocks_in_place() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_caret(14);
    reported(&mut e, |e| { e.paste_html(RICH, "Xy\nL1\nL2"); });
    assert_eq!(lines(&e), ["before", "Tabelle", "Xy", "L1", "L2", "after"]);
    assert_eq!(e.marks(), [MarkRange::new(15..16, Mark::Bold)], "Bold is on the X, not on the line break before it");
    let p = Block::Paragraph;
    assert_eq!(e.blocks(), [p, p, p, Block::Bullet(0), Block::Bullet(0), p]);
    assert_eq!(atoms(&e), [false, true, false, false, false, false]);
    assert_eq!((ids(&e)[1], ids(&e)[5]), (2, 3));
}

#[test]
fn a_rich_paste_before_an_atom_keeps_its_marks_and_blocks_in_place() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_caret(7);
    reported(&mut e, |e| { e.paste_html(RICH, "Xy\nL1\nL2"); });
    assert_eq!(lines(&e), ["before", "Xy", "L1", "L2", "Tabelle", "after"]);
    assert_eq!(e.marks(), [MarkRange::new(7..8, Mark::Bold)]);
    let p = Block::Paragraph;
    assert_eq!(e.blocks(), [p, p, Block::Bullet(0), Block::Bullet(0), p, p]);
    assert_eq!(atoms(&e), [false, false, false, false, true, false]);
    assert_eq!(ids(&e)[4], 2);
    assert_eq!(e.focus, 15, "after the pasted text, before the atom");
}

#[test]
fn text_typed_over_an_empty_paragraph_before_an_atom_keeps_ids_apart() {
    let mut e = field("\nTabelle", &[1]);
    e.set_selection(0, 1);
    reported(&mut e, |e| { e.key(&k(Key::Text("x".into())), None); });
    assert_eq!((lines(&e), ids(&e), atoms(&e)), (vec!["x", "Tabelle"], vec![1, 2], vec![false, true]));
}

#[test]
fn text_typed_over_an_atom_up_to_the_next_keeps_ids_apart() {
    let mut e = field("Tabelle\nBild", &[0, 1]);
    e.set_selection(0, 8);
    reported(&mut e, |e| { e.key(&k(Key::Text("x".into())), None); });
    assert_eq!((lines(&e), atoms(&e)), (vec!["x", "Bild"], vec![false, true]));
    assert_eq!(ids(&e), [10, 2], "the typed paragraph is a new one: an atom's id never names text");
}

#[test]
fn text_typed_over_an_atom_takes_a_new_id() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_selection(7, 14);
    reported(&mut e, |e| { e.key(&k(Key::Text("x".into())), None); });
    assert_eq!((lines(&e), ids(&e), atoms(&e)), (vec!["before", "x", "after"], vec![1, 10, 3], vec![false; 3]));
}

#[test]
fn text_typed_over_a_run_of_atoms_up_to_the_next_takes_a_new_id() {
    let mut e = field("Tabelle\nBild\nGrafik", &[0, 1, 2]);
    e.set_selection(0, 13);
    reported(&mut e, |e| { e.key(&k(Key::Text("x".into())), None); });
    assert_eq!((lines(&e), ids(&e), atoms(&e)), (vec!["x", "Grafik"], vec![10, 3], vec![false, true]));
}

#[test]
fn text_typed_from_inside_an_atoms_label_takes_a_new_id() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_selection(9, 14);
    reported(&mut e, |e| { e.key(&k(Key::Text("x".into())), None); });
    assert_eq!((atoms(&e), ids(&e)), (vec![false; 3], vec![1, 10, 3]));
}

#[test]
fn a_paste_over_atoms_takes_a_new_id() {
    let mut e = field("before\nTabelle\nBild\nafter", &[1, 2]);
    e.set_selection(7, 19);
    reported(&mut e, |e| { e.paste("eins\nzwei"); });
    assert_eq!((lines(&e), atoms(&e)), (vec!["before", "eins", "zwei", "after"], vec![false; 4]));
    assert_eq!(ids(&e), [1, 10, 11, 4]);
}

#[test]
fn replace_text_numbers_its_paragraphs() {
    let mut e = field("one\ntwo", &[]);
    e.replace_text("a\nb\nc");
    assert_eq!((ids(&e), e.next_paragraph_id()), (vec![10, 11, 12], 13));

    let mut plain = TextEdit::new("one");
    plain.replace_text("a\nb");
    assert!(plain.is_plain(), "a field without ids gains no attributes");
}

#[test]
fn backspace_after_an_atom_selects_it_and_then_deletes_it() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_caret(15);
    let first = reported(&mut e, |e| { e.key(&k(Key::Backspace), None); });
    assert_eq!(first, [], "the first press changes nothing");
    assert_eq!((e.text.as_str(), e.range()), ("before\nTabelle\nafter", (7, 14)));
    let second = reported(&mut e, |e| { e.key(&k(Key::Backspace), None); });
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "after"], vec![1, 3]));
    assert_eq!(second.iter().map(|c| (c.range.clone(), c.len, c.kind)).collect::<Vec<_>>(), [(7..15, 0, EditChangeKind::Deleted)]);
}

#[test]
fn delete_before_an_atom_selects_it_and_then_deletes_it() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_caret(6);
    let first = reported(&mut e, |e| { e.key(&k(Key::Delete), None); });
    assert_eq!(first, []);
    assert_eq!((e.text.as_str(), e.range()), ("before\nTabelle\nafter", (7, 14)));
    let second = reported(&mut e, |e| { e.key(&k(Key::Delete), None); });
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "after"], vec![1, 3]));
    assert_eq!(second.iter().map(|c| (c.range.clone(), c.len, c.kind)).collect::<Vec<_>>(), [(7..15, 0, EditChangeKind::Deleted)]);
}

#[test]
fn the_two_presses_are_one_undo_step_of_the_fields_own_history() {
    let mut e = field("before\nTabelle\nafter", &[1]);
    e.set_history(true);
    e.set_caret(15);
    e.key(&k(Key::Backspace), None);
    assert_eq!(e.history_len(), (0, 0), "selecting records nothing");
    e.key(&k(Key::Backspace), None);
    assert_eq!(e.history_len(), (1, 0));
    e.undo();
    assert_eq!((e.text.as_str(), atoms(&e), ids(&e)), ("before\nTabelle\nafter", vec![false, true, false], vec![1, 2, 3]));
}

#[test]
fn an_empty_paragraph_beside_an_atom_goes_at_the_first_press() {
    let mut e = field("before\nTabelle\n\nafter", &[1]);
    e.set_caret(15);
    reported(&mut e, |e| { e.key(&k(Key::Backspace), None); });
    assert_eq!((lines(&e), ids(&e), atoms(&e)), (vec!["before", "Tabelle", "after"], vec![1, 2, 4], vec![false, true, false]));
    assert_eq!(e.range(), (14, 14));

    let mut e = field("before\n\nTabelle\nafter", &[2]);
    e.set_caret(7);
    reported(&mut e, |e| { e.key(&k(Key::Delete), None); });
    assert_eq!((lines(&e), ids(&e), atoms(&e)), (vec!["before", "Tabelle", "after"], vec![1, 3, 4], vec![false, true, false]));
    assert_eq!(e.range(), (7, 7));
}

#[test]
fn a_list_item_after_an_atom_leaves_its_list_before_it_selects_the_atom() {
    let mut e = field("Tabelle\nPunkt", &[0]);
    e.set_blocks(vec![Block::Paragraph, Block::Bullet(0)]);
    e.set_caret(8);
    e.key(&k(Key::Backspace), None);
    assert_eq!((e.blocks()[1], e.range()), (Block::Paragraph, (8, 8)));
    e.key(&k(Key::Backspace), None);
    assert_eq!((e.text.as_str(), e.range()), ("Tabelle\nPunkt", (0, 7)));
}
