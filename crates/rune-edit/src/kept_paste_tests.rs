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

// A paste at a line's start inserts the fragment before the line. The line it lands in keeps
// its block, its attributes (id, style, alignment, indents) and its atom; the lines the
// fragment brings are new paragraphs with new ids, its own block, style and alignment, and
// no indents.

/// A line as these tests compare it: text, id, style, alignment, indents, atom and block.
type Row = (String, u64, u16, ParagraphAlign, (f32, f32), Option<f32>, Block);
/// The id a row names when it must be a new one.
const NEW: u64 = u64::MAX;

fn rows(e: &TextEdit) -> Vec<Row> {
    let blocks = e.blocks();
    e.text.split('\n').enumerate().map(|(i, t)| {
        let a = e.paragraph(i);
        (t.to_string(), a.id, a.style, a.align, (a.indent, a.first_line_indent), a.atom, blocks[i])
    }).collect()
}

/// The receiving line's attributes: an id, a style, an alignment and both indents.
fn kept() -> ParagraphAttrs {
    ParagraphAttrs::default().id(2).style(3).align(ParagraphAlign::Right).indent(12.0, 4.0)
}

fn kept_row(text: &str, atom: Option<f32>, block: Block) -> Row {
    (text.into(), 2, 3, ParagraphAlign::Right, (12.0, 4.0), atom, block)
}

/// A paragraph the paste brought: a new id, the fragment's style and alignment, no indents.
fn new_row(text: &str, style: u16, align: ParagraphAlign, atom: Option<f32>, block: Block) -> Row {
    (text.into(), NEW, style, align, (0.0, 0.0), atom, block)
}

/// "top", the receiving line `text`, "end", each with an id from 1, ids on from 10. The
/// receiving line is a bullet, and a locked atom 40 points tall when `atom`.
fn start_field(text: &str, atom: bool) -> TextEdit {
    let d = ParagraphAttrs::default();
    let line = if atom { kept().atom(40.0) } else { kept() };
    let mut e = TextEdit::from_parts(&format!("top\n{text}\nend"), vec![], vec![Block::Paragraph, if atom { Block::Paragraph } else { Block::Bullet(0) }, Block::Paragraph]);
    e.set_paragraph_attrs(vec![d.id(1), line, d.id(3)]);
    e.set_paragraph_ids(10);
    e.set_history(false);
    e.take_changes();
    e
}

/// A fragment of `text`'s lines: bullets in the style 5 centred, then style 6 justified, the
/// last line a locked atom 30 points tall when `atom`; as an app builds one.
fn start_fragment(text: &str, atom: bool) -> TextEdit {
    let n = text.split('\n').count();
    let d = ParagraphAttrs::default();
    let attrs = (0..n).map(|i| match i {
        _ if atom && i == n - 1 => d.id(77).style(6).align(ParagraphAlign::Justified).atom(30.0),
        0 => d.id(77).style(5).align(ParagraphAlign::Center),
        1 => d.id(78).style(6).align(ParagraphAlign::Justified),
        _ => d.id(79 + i as u64),
    });
    let blocks = text.split('\n').enumerate().map(|(i, t)| if t.is_empty() || (atom && i == n - 1) { Block::Paragraph } else { Block::Bullet(0) });
    let mut src = TextEdit::from_parts(text, vec![], blocks.collect());
    src.set_history(false);
    src.set_paragraph_attrs(attrs.collect());
    src
}

/// What a paste leaves between "top" and "end", with `NEW` ids matched against the old ones.
fn pasted_between(mut e: TextEdit, frag: TextEdit) -> Vec<Row> {
    reported(&mut e, |e| {
        e.set_caret(4);
        e.paste_fragment(frag, "");
    });
    let all = rows(&e);
    assert_eq!((all[0].0.as_str(), all[0].1), ("top", 1), "{all:?}");
    assert_eq!((all[all.len() - 1].0.as_str(), all[all.len() - 1].1), ("end", 3), "{all:?}");
    let mut seen: Vec<u64> = all.iter().map(|r| r.1).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), all.len(), "every paragraph has an id of its own: {all:?}");
    all[1..all.len() - 1].iter().cloned().map(|mut r| {
        if ![1, 2, 3].contains(&r.1) && r.1 != 0 { r.1 = NEW; }
        r
    }).collect()
}

fn bullet() -> Block { Block::Bullet(0) }
fn centre(t: &str) -> Row { new_row(t, 5, ParagraphAlign::Center, None, bullet()) }
fn justified(t: &str) -> Row { new_row(t, 6, ParagraphAlign::Justified, None, bullet()) }

#[test]
fn a_paste_at_the_start_of_a_line_with_text_pushes_the_line_down_whole() {
    for (frag, want) in [
        ("A\n", vec![centre("A"), kept_row("XYZ", None, bullet())]),
        ("A\nB\n", vec![centre("A"), justified("B"), kept_row("XYZ", None, bullet())]),
        ("A", vec![kept_row("AXYZ", None, bullet())]),
        ("A\nB", vec![centre("A"), kept_row("BXYZ", None, bullet())]),
        ("A\nB\nC", vec![centre("A"), justified("B"), kept_row("CXYZ", None, bullet())]),
    ] {
        let got = pasted_between(start_field("XYZ", false), start_fragment(frag, false));
        assert_eq!(got, want, "pasted {frag:?}");
    }
}

#[test]
fn a_paste_at_an_empty_styled_line_with_an_id_leaves_the_line_its_attributes() {
    for (frag, want) in [
        ("A", vec![kept_row("A", None, bullet())]),
        ("A\n", vec![centre("A"), kept_row("", None, bullet())]),
        ("A\nB", vec![centre("A"), kept_row("B", None, bullet())]),
        ("A\nB\n", vec![centre("A"), justified("B"), kept_row("", None, bullet())]),
    ] {
        let got = pasted_between(start_field("", false), start_fragment(frag, false));
        assert_eq!(got, want, "pasted {frag:?}");
    }
}

#[test]
fn a_paste_at_a_locked_atoms_line_never_joins_it_and_leaves_it_whole() {
    let label = "Tabelle";
    for (frag, want) in [
        ("A", vec![centre("A"), kept_row(label, Some(40.0), Block::Paragraph)]),
        ("A\n", vec![centre("A"), kept_row(label, Some(40.0), Block::Paragraph)]),
        ("A\nB", vec![centre("A"), justified("B"), kept_row(label, Some(40.0), Block::Paragraph)]),
        ("A\nB\n", vec![centre("A"), justified("B"), kept_row(label, Some(40.0), Block::Paragraph)]),
    ] {
        let got = pasted_between(start_field(label, true), start_fragment(frag, false));
        assert_eq!(got, want, "pasted {frag:?}");
    }
}

#[test]
fn a_fragment_with_an_atom_pasted_at_an_empty_line_leaves_the_line_after_it() {
    let atom = new_row("B", 6, ParagraphAlign::Justified, Some(30.0), Block::Paragraph);
    let got = pasted_between(start_field("", false), start_fragment("A\nB", true));
    assert_eq!(got, vec![centre("A"), atom.clone(), kept_row("", None, bullet())]);
    let got = pasted_between(start_field("XYZ", false), start_fragment("A\nB", true));
    assert_eq!(got, vec![centre("A"), atom, kept_row("XYZ", None, bullet())]);
}

#[test]
fn a_fragment_without_paragraph_attributes_brings_paragraphs_with_none_either() {
    let frag = || TextEdit::from_parts("A\nB", vec![], vec![]);
    let got = pasted_between(start_field("XYZ", false), frag());
    assert_eq!(got, vec![new_row("A", 0, ParagraphAlign::Left, None, Block::Paragraph), kept_row("BXYZ", None, bullet())]);
}

#[test]
fn a_single_line_pasted_into_an_empty_plain_line_still_gives_it_the_pasted_attributes() {
    let mut e = TextEdit::from_parts("top\n\nend", vec![], vec![]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default(); 3]);
    e.set_history(false);
    e.set_caret(4);
    e.paste_fragment(start_fragment("A", false), "");
    let got = rows(&e);
    assert_eq!((got[1].0.as_str(), got[1].2, got[1].3, got[1].6), ("A", 5, ParagraphAlign::Center, bullet()), "{got:?}");
}
