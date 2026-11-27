//! Every way paragraph attributes, locked atoms, the history switch and the change report can
//! go wrong, each as a test that was written, and seen failing, before the code:
//!
//! 1. A split leaves the new paragraph without the attributes of the one it came from.
//! 2. A split gives both halves one id, or an id the counter did not hand out.
//! 3. A split copies the page break or the atom onto the new paragraph.
//! 4. A merge keeps the second paragraph's attributes, or drops the first's id.
//! 5. A paste of several lines leaves attributes out of step with the lines.
//! 6. A deletion across paragraphs shifts the attributes of the paragraphs after it.
//! 7. A plain field starts carrying attributes it never asked for.
//! 8. The caret lands inside an atom's label.
//! 9. An arrow key takes more than one step to cross an atom.
//! 10. Deleting an atom leaves its label, an empty atom, or takes more than one change.
//! 11. A selection ends inside an atom.
//! 12. Text typed beside an atom joins its label, or the atom loses its id.
//! 13. Backspace after an atom, or Delete before one, merges a paragraph into its label.
//! 14. Deleting the only paragraph, an atom, leaves an atom with no label.
//! 15. With history off the field still records, and its undo still changes the text.
//! 16. An edit reports the wrong bytes, or a change of marks or attributes alone reports nothing.
//! 17. An edit that changes nothing reports a change.
//! 18. Changes nobody takes pile up without bound.
//! 19. A run is a subscript and a superscript at once.
//! 20. The field's own undo forgets the attributes.
//! 21. `format_paragraphs` lets its closure rewrite an id or an atom.
//! 22. Return in a paragraph with attributes submits instead of breaking it.

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

/// Three paragraphs with ids 1 to 3, the middle one styled, centred, indented and breaking
/// the page; the next id is 10.
fn doc() -> TextEdit {
    let mut e = TextEdit::from_parts("one\ntwo words\nthree", vec![], vec![]);
    let d = ParagraphAttrs::default();
    e.set_paragraph_attrs(vec![d.id(1), d.id(2).style(4).align(ParagraphAlign::Center).indent(12.0, 6.0).page_break_before(true), d.id(3)]);
    e.set_paragraph_ids(10);
    e
}

/// "before", an atom labelled "Tabelle" 40 points tall with id 2, "after".
fn with_atom() -> TextEdit {
    let mut e = TextEdit::from_parts("before\nTabelle\nafter", vec![], vec![]);
    let d = ParagraphAttrs::default();
    e.set_paragraph_attrs(vec![d.id(1), d.id(2).atom(40.0), d.id(3)]);
    e.set_paragraph_ids(10);
    e.take_changes();
    e
}

#[test]
fn a_split_copies_the_attributes_and_takes_the_next_id() {
    let mut e = doc();
    e.set_caret(7);
    e.key(&k(Key::Enter), None);
    assert_eq!(lines(&e), ["one", "two", " words", "three"]);
    let a = e.paragraph_attrs();
    assert_eq!((a[2].style, a[2].align, a[2].indent, a[2].first_line_indent), (4, ParagraphAlign::Center, 12.0, 6.0));
    assert_eq!(ids(&e), [1, 2, 10, 3]);
    assert_eq!(e.next_paragraph_id(), 11);
    assert!(a[1].page_break_before && !a[2].page_break_before, "the break stays with the paragraph it was on");
}

#[test]
fn a_merge_keeps_the_first_paragraphs_attributes() {
    let mut e = doc();
    e.set_caret(13);
    e.key(&k(Key::Delete), None);
    assert_eq!(lines(&e), ["one", "two wordsthree"]);
    assert_eq!(ids(&e), [1, 2]);
    assert_eq!(e.paragraph(1).style, 4);

    let mut e = doc();
    e.set_caret(4);
    e.key(&k(Key::Backspace), None);
    assert_eq!(lines(&e), ["onetwo words", "three"]);
    assert_eq!((ids(&e), e.paragraph(0).style), (vec![1, 3], 0));
}

#[test]
fn an_empty_first_paragraph_gives_way_to_the_one_merged_into_it() {
    let mut e = TextEdit::from_parts("\nheading", vec![], vec![]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default().id(1), ParagraphAttrs::default().id(2).style(3)]);
    e.set_caret(1);
    e.key(&k(Key::Backspace), None);
    assert_eq!((e.text.as_str(), ids(&e), e.paragraph(0).style), ("heading", vec![2], 3));
}

#[test]
fn a_paste_of_lines_and_a_deletion_across_paragraphs_keep_attributes_in_step() {
    let mut e = doc();
    e.set_caret(3);
    e.insert("\nA\nB");
    assert_eq!(lines(&e), ["one", "A", "B", "two words", "three"]);
    assert_eq!(ids(&e), [1, 10, 11, 2, 3]);
    assert_eq!(e.paragraph(3).style, 4);
    e.set_selection(2, 8);
    e.insert("");
    assert_eq!(lines(&e), ["ontwo words", "three"]);
    assert_eq!(ids(&e), [1, 3]);
}

#[test]
fn a_plain_field_carries_no_attributes() {
    let mut e = TextEdit::new("a");
    e.insert("\nb\nc");
    e.set_selection(0, 3);
    e.insert("");
    assert!(e.attrs.is_empty() && e.is_plain());
    assert_eq!(e.paragraph_attrs(), vec![ParagraphAttrs::default(); 2]);
}

#[test]
fn the_caret_and_a_selection_stay_out_of_an_atom() {
    let mut e = with_atom();
    e.set_caret(9);
    assert_eq!(e.focus, 7, "the nearer edge");
    e.set_caret(12);
    assert_eq!(e.focus, 14);
    e.set_selection(3, 10);
    assert_eq!((e.anchor, e.focus), (3, 14));
    e.set_selection(18, 9);
    assert_eq!((e.anchor, e.focus), (18, 7));
}

#[test]
fn an_arrow_crosses_an_atom_in_one_step() {
    let mut e = with_atom();
    e.set_caret(7);
    e.key(&k(Key::Right), None);
    assert_eq!(e.focus, 14);
    e.key(&k(Key::Left), None);
    assert_eq!(e.focus, 7);
}

#[test]
fn deleting_an_atom_is_one_edit_that_takes_its_line() {
    let mut e = with_atom();
    e.set_caret(14);
    e.key(&k(Key::Backspace), None);
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "after"], vec![1, 3]));
    assert!(e.paragraph_attrs().iter().all(|a| a.atom.is_none()));
    let changes = e.take_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!((changes[0].range.clone(), changes[0].len, changes[0].kind), (7..15, 0, EditChangeKind::Deleted));

    let mut e = with_atom();
    e.set_caret(7);
    e.key(&k(Key::Delete), None);
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "after"], vec![1, 3]));

    let mut e = with_atom();
    e.set_selection(3, 9);
    e.insert("");
    assert_eq!((lines(&e), ids(&e)), (vec!["bef", "after"], vec![1, 3]));
}

#[test]
fn deleting_the_last_or_only_atom_leaves_no_empty_atom() {
    let mut e = TextEdit::from_parts("before\nTabelle", vec![], vec![]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default().id(1), ParagraphAttrs::default().id(2).atom(40.0)]);
    e.set_caret(14);
    e.key(&k(Key::Backspace), None);
    assert_eq!((e.text.as_str(), ids(&e)), ("before", vec![1]));

    let mut e = TextEdit::from_parts("Tabelle", vec![], vec![]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default().id(2).atom(40.0)]);
    e.set_caret(7);
    e.key(&k(Key::Backspace), None);
    assert_eq!(e.text, "");
    assert_eq!(e.paragraph(0).atom, None);
}

#[test]
fn text_typed_beside_an_atom_gets_a_paragraph_of_its_own() {
    let mut e = with_atom();
    e.set_caret(14);
    e.key(&k(Key::Text("x".into())), None);
    assert_eq!(lines(&e), ["before", "Tabelle", "x", "after"]);
    assert_eq!(ids(&e), [1, 2, 10, 3]);
    assert_eq!(e.focus, 16);
    assert_eq!(e.paragraph_attrs().iter().map(|a| a.atom).collect::<Vec<_>>(), [None, Some(40.0), None, None]);

    let mut e = with_atom();
    e.set_caret(7);
    e.key(&k(Key::Text("y".into())), None);
    assert_eq!(lines(&e), ["before", "y", "Tabelle", "after"]);
    assert_eq!(ids(&e), [1, 10, 2, 3]);
    assert_eq!(e.focus, 8, "after the typed text, before the atom");
    assert_eq!(e.paragraph_attrs().iter().map(|a| a.atom).collect::<Vec<_>>(), [None, None, Some(40.0), None]);
    e.key(&k(Key::Text("z".into())), None);
    assert_eq!(lines(&e), ["before", "yz", "Tabelle", "after"]);
}

#[test]
fn return_beside_an_atom_adds_an_empty_paragraph() {
    let mut e = with_atom();
    e.set_caret(14);
    e.key(&k(Key::Enter), None);
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "Tabelle", "", "after"], vec![1, 2, 10, 3]));
    assert_eq!(e.paragraph(2).atom, None);
    let mut e = with_atom();
    e.set_caret(7);
    e.key(&k(Key::Enter), None);
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "", "Tabelle", "after"], vec![1, 10, 2, 3]));
    assert_eq!((e.paragraph(1).atom, e.paragraph(2).atom), (None, Some(40.0)));
}

#[test]
fn no_paragraph_merges_into_an_atoms_label() {
    // The first press selects the atom (`atom_edge_tests`) and changes nothing.
    let mut e = with_atom();
    e.set_caret(15);
    e.key(&k(Key::Backspace), None);
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "Tabelle", "after"], vec![1, 2, 3]));
    e.set_caret(6);
    e.key(&k(Key::Delete), None);
    assert_eq!((lines(&e), ids(&e)), (vec!["before", "Tabelle", "after"], vec![1, 2, 3]));
    assert!(e.take_changes().is_empty(), "nothing changed, nothing reported");

    // An empty paragraph beside the atom goes, and the atom stays.
    let mut e = TextEdit::from_parts("\nTabelle\n", vec![], vec![]);
    let d = ParagraphAttrs::default();
    e.set_paragraph_attrs(vec![d.id(1), d.id(2).atom(40.0), d.id(3)]);
    e.set_caret(9);
    e.key(&k(Key::Backspace), None);
    assert_eq!((e.text.as_str(), ids(&e)), ("\nTabelle", vec![1, 2]));
    e.set_caret(0);
    e.key(&k(Key::Delete), None);
    assert_eq!((e.text.as_str(), ids(&e), e.paragraph(0).atom), ("Tabelle", vec![2], Some(40.0)));
}

#[test]
fn insert_atom_puts_it_on_a_line_of_its_own() {
    let mut e = doc();
    e.set_caret(7);
    e.insert_atom("Bild", 30.0);
    assert_eq!(lines(&e), ["one", "two", "Bild", " words", "three"]);
    assert_eq!(e.paragraph_attrs().iter().map(|a| a.atom).collect::<Vec<_>>(), [None, None, Some(30.0), None, None]);
    let got = ids(&e);
    assert_eq!((got[0], got[1], got[4]), (1, 2, 3));
    assert!(got[2] >= 10 && got[3] >= 10 && got[2] != got[3]);
    assert_eq!(e.focus, 12);
}

#[test]
fn with_history_off_the_field_records_nothing() {
    let mut e = doc();
    e.set_history(false);
    e.set_caret(3);
    e.key(&k(Key::Text("x".into())), None);
    e.set_selection(0, 3);
    e.toggle_mark(Mark::Bold);
    assert_eq!(e.history_len(), (0, 0));
    e.undo();
    assert_eq!(lines(&e)[0], "onex");
    assert!(!e.records_history());
    assert!(e.key(&KeyEvent { key: Key::Command('z'), mods: Mods { cmd: true, ..Mods::default() } }, None) == EditOutcome::default());
    assert_eq!(lines(&e)[0], "onex");
}

#[test]
fn every_edit_reports_what_it_changed() {
    let mut e = doc();
    e.take_changes();
    e.set_caret(3);
    e.key(&k(Key::Text("xy".into())), None);
    e.key(&k(Key::Backspace), None);
    e.key(&k(Key::Enter), None);
    let c = e.take_changes();
    let got: Vec<_> = c.iter().map(|c| (c.range.clone(), c.len, c.kind)).collect();
    assert_eq!(got, [(3..3, 2, EditChangeKind::Typed), (4..5, 0, EditChangeKind::Deleted), (4..4, 1, EditChangeKind::LineBreak)]);

    e.set_selection(0, 3);
    e.toggle_mark(Mark::Bold);
    e.set_mark(Mark::family("Carlito"));
    let c = e.take_changes();
    assert_eq!(c.iter().map(|c| (c.range.clone(), c.len, c.kind)).collect::<Vec<_>>(), vec![(0..3, 3, EditChangeKind::Marks); 2]);

    // The text is "onex", an empty paragraph, "two words", "three": the selection touches the first.
    e.set_selection(1, 2);
    e.format_paragraphs(|a| a.align = ParagraphAlign::Right);
    e.set_block(Block::Bullet(0));
    let c = e.take_changes();
    assert_eq!(c.iter().map(|c| (c.range.clone(), c.len, c.kind)).collect::<Vec<_>>(), vec![(0..4, 4, EditChangeKind::Paragraphs); 2]);

    e.set_selection(2, 4);
    e.insert("QQQ");
    let c = e.take_changes();
    assert_eq!((c[0].range.clone(), c[0].len, c[0].kind), (2..4, 3, EditChangeKind::Replaced));
}

#[test]
fn an_edit_that_changes_nothing_reports_nothing() {
    let mut e = doc();
    e.take_changes();
    e.set_selection(0, 3);
    e.insert("one");
    e.format_paragraphs(|_| {});
    e.set_block(Block::Paragraph);
    e.set_caret(0);
    e.key(&k(Key::Backspace), None);
    e.key(&k(Key::Left), None);
    assert_eq!(e.take_changes(), []);
}

#[test]
fn untaken_changes_fold_into_one() {
    let mut e = TextEdit::new("start");
    for _ in 0..1000 {
        e.key(&k(Key::Text("a".into())), None);
    }
    let c = e.take_changes();
    assert!(c.len() <= 257, "{} changes kept", c.len());
    assert_eq!((c[0].range.clone(), c[0].kind), (0..5, EditChangeKind::Replaced));
    assert!(c[0].len > 5);
    assert!(e.take_changes().is_empty());
}

#[test]
fn a_run_is_a_subscript_or_a_superscript_not_both() {
    let mut e = doc();
    e.set_selection(0, 3);
    e.set_mark(Mark::Baseline(Baseline::Sub));
    e.set_mark(Mark::Baseline(Baseline::Super));
    assert_eq!(e.marks(), [MarkRange::new(0..3, Mark::Baseline(Baseline::Super))]);
    e.toggle_mark(Mark::Baseline(Baseline::Super));
    assert!(e.marks().is_empty());
}

#[test]
fn the_fields_own_undo_restores_attributes() {
    let mut e = doc();
    e.set_caret(7);
    e.key(&k(Key::Enter), None);
    e.set_selection(0, 0);
    e.format_paragraphs(|a| a.align = ParagraphAlign::Justified);
    e.undo();
    assert_eq!(e.paragraph(0).align, ParagraphAlign::Left);
    e.undo();
    assert_eq!((lines(&e), ids(&e)), (vec!["one", "two words", "three"], vec![1, 2, 3]));
    e.redo();
    assert_eq!(ids(&e), [1, 2, 10, 3]);
}

#[test]
fn format_paragraphs_keeps_ids_and_atoms() {
    let mut e = with_atom();
    e.set_selection(0, e.text.len());
    e.format_paragraphs(|a| *a = ParagraphAttrs::default().style(2).id(99).atom(5.0));
    assert_eq!(ids(&e), [1, 2, 3]);
    assert_eq!(e.paragraph_attrs().iter().map(|a| (a.style, a.atom)).collect::<Vec<_>>(), [(2, None), (2, Some(40.0)), (2, None)]);
}

#[test]
fn return_breaks_a_paragraph_that_carries_attributes() {
    let mut e = doc();
    e.set_caret(3);
    let out = e.key(&k(Key::Enter), None);
    assert!(!out.submit);
    assert_eq!(lines(&e).len(), 4);
    let mut plain = TextEdit::new("a");
    assert!(plain.key(&k(Key::Enter), None).submit, "a plain field's Return still submits");
}
