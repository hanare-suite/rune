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
fn text_typed_over_a_whole_paragraph_joins_the_next_one_which_keeps_its_id() {
    let mut e = field("eins\nzwei", &[]);
    reported(&mut e, |e| {
        e.set_selection(0, 5);
        e.insert("x");
    });
    assert_eq!((lines(&e), ids(&e)), (vec!["xzwei"], vec![2]));
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

#[test]
fn a_paste_at_a_lines_end_joins_the_line_and_brings_paragraphs_without_its_indents() {
    let got = {
        let mut e = start_field("XYZ", false);
        reported(&mut e, |e| {
            e.set_caret(7);
            e.paste_fragment(start_fragment("A\nB", false), "");
        });
        rows(&e)
    };
    assert_eq!(got[1], kept_row("XYZA", None, bullet()));
    assert_eq!(got[2].2, 6);
    assert_eq!((got[2].0.as_str(), got[2].3, got[2].4, got[2].6), ("B", ParagraphAlign::Justified, (0.0, 0.0), bullet()), "{got:?}");
    assert!(![1, 2, 3].contains(&got[2].1), "{got:?}");
}

// A plain paste and Return at a line's start follow the same rule: the line keeps its id,
// attributes, block and atom, and a paragraph the edit brings is new, above it.

fn enter() -> KeyEvent {
    KeyEvent { key: Key::Enter, mods: Mods::default() }
}

/// What the edit at the start of the receiving line leaves between "top" and "end".
fn at_start(mut e: TextEdit, f: impl FnOnce(&mut TextEdit)) -> Vec<Row> {
    reported(&mut e, |e| {
        e.set_caret(4);
        f(e);
    });
    let all = rows(&e);
    let mut seen: Vec<u64> = all.iter().map(|r| r.1).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), all.len(), "every paragraph has an id of its own: {all:?}");
    all[1..all.len() - 1].iter().cloned().map(|mut r| {
        if ![1, 2, 3].contains(&r.1) && r.1 != 0 { r.1 = NEW; }
        r
    }).collect()
}

/// A new paragraph as Return makes it above the line: the line's style, alignment and block.
fn above(text: &str, atom: bool) -> Row {
    (text.into(), NEW, 3, ParagraphAlign::Right, (12.0, 4.0), None, if atom { Block::Paragraph } else { bullet() })
}

fn plain_row(text: &str) -> Row {
    new_row(text, 0, ParagraphAlign::Left, None, Block::Paragraph)
}

#[test]
fn return_at_the_start_of_a_line_adds_a_paragraph_above_and_the_line_keeps_its_id() {
    let got = at_start(start_field("XYZ", false), |e| { e.key(&enter(), None); });
    assert_eq!(got, vec![above("", false), kept_row("XYZ", None, bullet())]);
    let mut e = start_field("", false);
    e.set_blocks(vec![Block::Paragraph, Block::Paragraph, Block::Paragraph]);
    let got = at_start(e, |e| { e.key(&enter(), None); });
    assert_eq!(got, vec![above("", true), kept_row("", None, Block::Paragraph)]);
    let got = at_start(start_field("Tabelle", true), |e| { e.key(&enter(), None); });
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[1], kept_row("Tabelle", Some(40.0), Block::Paragraph));
    assert_eq!((got[0].0.as_str(), got[0].1, got[0].5), ("", NEW, None));
}

#[test]
fn a_plain_paste_at_the_start_of_a_line_follows_the_rule_of_a_fragment() {
    for (text, want) in [
        ("A\nB", vec![plain_row("A"), kept_row("BXYZ", None, bullet())]),
        ("A\nB\n", vec![plain_row("A"), plain_row("B"), kept_row("XYZ", None, bullet())]),
        ("A\n", vec![plain_row("A"), kept_row("XYZ", None, bullet())]),
    ] {
        let got = at_start(start_field("XYZ", false), |e| { e.paste(text); });
        assert_eq!(got, want, "pasted {text:?}");
    }
    let got = at_start(start_field("", false), |e| { e.paste("A\nB"); });
    assert_eq!(got, vec![plain_row("A"), kept_row("B", None, bullet())]);
    for text in ["A\nB", "A\nB\n"] {
        let got = at_start(start_field("Tabelle", true), |e| { e.paste(text); });
        assert_eq!(got.last(), Some(&kept_row("Tabelle", Some(40.0), Block::Paragraph)), "pasted {text:?}: {got:?}");
        assert_eq!(got.iter().filter(|r| r.5.is_some()).count(), 1, "{got:?}");
    }
}

#[test]
fn undo_and_redo_give_back_the_ids_of_a_paste_and_a_return_at_a_lines_start() {
    for edit in [|e: &mut TextEdit| { e.key(&enter(), None); }, |e: &mut TextEdit| { e.paste("A\nB"); }] {
        let mut e = start_field("XYZ", false);
        e.set_history(true);
        e.set_caret(4);
        let (text, attrs) = (e.text.clone(), e.paragraph_attrs().to_vec());
        edit(&mut e);
        let (done, done_attrs) = (e.text.clone(), e.paragraph_attrs().to_vec());
        e.undo();
        assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec()), (text, attrs));
        e.redo();
        assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec()), (done, done_attrs));
    }
}

// A selection that starts at a line's start gives way to the text, which goes before what is
// left of the line: the line keeps its id, attributes and block.

#[test]
fn text_over_a_selection_at_a_lines_start_goes_before_what_is_left_of_the_line() {
    let select = |e: &mut TextEdit| e.set_selection(4, 5);
    let want = vec![plain_row("A"), kept_row("BYZ", None, bullet())];
    let edits: [(&str, fn(&mut TextEdit)); 4] = [
        ("paste", |e| { e.set_selection(4, 5); e.paste("A\nB"); }),
        ("insert", |e| { e.set_selection(4, 5); e.insert("A\nB"); }),
        ("paste_html without marks", |e| { e.set_selection(4, 5); e.paste_html("<p>A</p><p>B</p>", "A\nB"); }),
        ("paste_html with marks", |e| { e.set_selection(4, 5); e.paste_html("<p><b>A</b></p><p>B</p>", "A\nB"); }),
    ];
    for (name, edit) in edits {
        let got = at_start(start_field("XYZ", false), edit);
        assert_eq!(got, want, "{name}");
    }
    let got = at_start(start_field("XYZ", false), |e| { select(e); e.key(&enter(), None); });
    assert_eq!(got, vec![above("", false), kept_row("YZ", None, bullet())]);
}

#[test]
fn undo_and_redo_give_back_the_ids_of_text_over_a_selection_at_a_lines_start() {
    let mut e = start_field("XYZ", false);
    e.set_history(true);
    e.set_selection(4, 5);
    let (text, attrs) = (e.text.clone(), e.paragraph_attrs().to_vec());
    e.paste("A\nB");
    let (done, done_attrs) = (e.text.clone(), e.paragraph_attrs().to_vec());
    e.undo();
    assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec()), (text, attrs));
    e.redo();
    assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec()), (done, done_attrs));
}

#[test]
fn a_fragment_ending_in_a_break_pasted_in_a_list_item_leaves_the_tail_in_the_list() {
    let mut e = start_field("XYZ", false);
    reported(&mut e, |e| {
        e.set_caret(5);
        e.paste_fragment(start_fragment("A\n", false), "");
    });
    let got = rows(&e);
    assert_eq!((got[1].0.as_str(), got[1].1, got[1].6), ("XA", 2, bullet()), "{got:?}");
    assert_eq!((got[2].0.as_str(), got[2].6), ("YZ", bullet()), "{got:?}");
}

// A paragraph's identity lives in its line break: the paragraph whose break an edit removes
// goes with it, and the text left joins the paragraph whose break stays, with its id,
// attributes and block.

fn all_rows(mut e: TextEdit, f: impl FnOnce(&mut TextEdit)) -> Vec<Row> {
    reported(&mut e, f);
    rows(&e).into_iter().map(|mut r| {
        if ![1, 2, 3].contains(&r.1) && r.1 != 0 { r.1 = NEW; }
        r
    }).collect()
}

fn top() -> Row { ("top".into(), 1, 0, ParagraphAlign::Left, (0.0, 0.0), None, Block::Paragraph) }
fn end(text: &str) -> Row { (text.into(), 3, 0, ParagraphAlign::Left, (0.0, 0.0), None, Block::Paragraph) }

#[test]
fn text_over_whole_paragraphs_joins_the_paragraph_whose_break_stays() {
    // "XYZ\n" and "top\nXYZ\n" selected, in a field where "end" follows.
    for from in [4, 0] {
        let edits: [(&str, fn(&mut TextEdit), Vec<Row>); 7] = [
            ("insert x", |e| e.insert("x"), vec![end("xend")]),
            ("paste x", |e| { e.paste("x"); }, vec![end("xend")]),
            ("paste_html x without marks", |e| { e.paste_html("<p>x</p>", "x"); }, vec![end("xend")]),
            ("paste_html x with marks", |e| { e.paste_html("<p><b>x</b></p>", "x"); }, vec![end("xend")]),
            ("paste A and a break", |e| { e.paste("A\n"); }, vec![plain_row("A"), end("end")]),
            ("paste_html A and a break without marks", |e| { e.paste_html("<p>A</p><p></p>", "A\n"); }, vec![plain_row("A"), end("end")]),
            ("paste_html A and a break with marks", |e| { e.paste_html("<p><b>A</b></p><p></p>", "A\n"); }, vec![plain_row("A"), end("end")]),
        ];
        for (name, edit, want) in edits {
            let got = all_rows(start_field("XYZ", false), |e| { e.set_selection(from, 8); edit(e); });
            let want = if from == 0 { [vec![], want].concat() } else { [vec![top()], want].concat() };
            assert_eq!(got, want, "{name} over {from}..8");
        }
        let got = all_rows(start_field("XYZ", false), |e| { e.set_selection(from, 8); e.key(&enter(), None); });
        assert_eq!(got.last(), Some(&end("end")), "Return over {from}..8: {got:?}");
        assert_eq!(got[got.len() - 2].0, "", "{got:?}");
    }
}

#[test]
fn a_selection_that_ends_inside_a_line_leaves_the_line_it_starts_at_its_id() {
    let kept = |t: &str| kept_row(t, None, bullet());
    for (name, edit) in [
        ("paste", (|e: &mut TextEdit| { e.paste("A\nB"); }) as fn(&mut TextEdit)),
        ("paste_html with marks", |e| { e.paste_html("<p><b>A</b></p><p>B</p>", "A\nB"); }),
        ("paste_html without marks", |e| { e.paste_html("<p>A</p><p>B</p>", "A\nB"); }),
    ] {
        let got = all_rows(start_field("XYZ", false), |e| { e.set_selection(4, 10); edit(e); });
        assert_eq!(got, vec![top(), plain_row("A"), kept("Bd")], "{name}");
    }
}

#[test]
fn undo_and_redo_give_back_the_ids_of_text_over_a_whole_paragraph() {
    for edit in [|e: &mut TextEdit| e.insert("x"), |e: &mut TextEdit| { e.paste("A\n"); }] {
        let mut e = start_field("XYZ", false);
        e.set_history(true);
        e.set_selection(4, 8);
        let (text, attrs) = (e.text.clone(), e.paragraph_attrs().to_vec());
        edit(&mut e);
        let (done, done_attrs) = (e.text.clone(), e.paragraph_attrs().to_vec());
        e.undo();
        assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec()), (text, attrs));
        e.redo();
        assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec()), (done, done_attrs));
    }
}

#[test]
fn a_composer_loaded_from_aligned_html_still_submits_on_return() {
    let html = r#"<p style="text-align:center">hello</p>"#;
    for mut e in [TextEdit::from_html(html), TextEdit::from_html_with_color(html)] {
        assert!(!e.attributed());
        assert!(e.key(&enter(), None).submit);
    }
}

#[test]
fn text_with_a_break_pasted_mid_line_leaves_the_tail_in_the_lines_block_with_or_without_marks() {
    let edits: [(&str, fn(&mut TextEdit)); 6] = [
        ("paste break", |e| { e.paste("A\n"); }),
        ("paste_html break without marks", |e| { e.paste_html("<p>A</p><p></p>", "A\n"); }),
        ("paste_html break with marks", |e| { e.paste_html("<p><b>A</b></p><p></p>", "A\n"); }),
        ("paste open", |e| { e.paste("A\nB"); }),
        ("paste_html open without marks", |e| { e.paste_html("<p>A</p><p>B</p>", "A\nB"); }),
        ("paste_html open with marks", |e| { e.paste_html("<p><b>A</b></p><p>B</p>", "A\nB"); }),
    ];
    for (name, edit) in edits {
        let mut e = start_field("XYZ", false);
        reported(&mut e, |e| { e.set_caret(5); edit(e); });
        let got = rows(&e);
        assert_eq!((got[1].0.starts_with('X'), got[1].6), (true, bullet()), "{name}: {got:?}");
        assert_eq!((got[2].0.ends_with("YZ"), got[2].6), (true, bullet()), "{name}: {got:?}");
    }
}

// Every path an edit can take gives one result: Return, `insert`, `paste`, `paste_html`
// without a mark and `paste_html` with one. Each case below names the document, the
// selection and what each path leaves, and checks undo and redo of each.

type Path = (&'static str, fn(&mut TextEdit));

/// The paths that put `A` and a line break over a selection.
const BREAK: [Path; 5] = [
    ("insert", |e| e.insert("A\n")),
    ("paste", |e| { e.paste("A\n"); }),
    ("paste_html unmarked", |e| { e.paste_html("<p>A</p><p></p>", "A\n"); }),
    ("paste_html marked", |e| { e.paste_html("<p><b>A</b></p><p></p>", "A\n"); }),
    ("paste_fragment", |e| { e.paste_fragment(TextEdit::from_parts("A\n", vec![], vec![]), ""); }),
];

/// The paths that put `A`, a line break and `B` over a selection.
const OPEN: [Path; 5] = [
    ("insert", |e| e.insert("A\nB")),
    ("paste", |e| { e.paste("A\nB"); }),
    ("paste_html unmarked", |e| { e.paste_html("<p>A</p><p>B</p>", "A\nB"); }),
    ("paste_html marked", |e| { e.paste_html("<p><b>A</b></p><p>B</p>", "A\nB"); }),
    ("paste_fragment", |e| { e.paste_fragment(TextEdit::from_parts("A\nB", vec![], vec![]), ""); }),
];

fn return_key(e: &mut TextEdit) {
    e.key(&enter(), None);
}

/// `AAA`, a locked atom `Tabelle`, then `end`: ids 1, 2, 3, `end` in style 3 and a bullet.
fn atom_between() -> TextEdit {
    let d = ParagraphAttrs::default();
    let mut e = TextEdit::from_parts("AAA\nTabelle\nend", vec![], vec![Block::Paragraph, Block::Paragraph, bullet()]);
    e.set_paragraph_attrs(vec![d.id(1), d.id(2).style(2).atom(40.0), d.id(3).style(3)]);
    e.set_paragraph_ids(10);
    e.set_history(false);
    e.take_changes();
    e
}

/// `top`, a list item `XYZ` (id 2, style 2, indented), then a plain `end` (id 3, style 3).
fn list_before_plain() -> TextEdit {
    let d = ParagraphAttrs::default();
    let mut e = TextEdit::from_parts("top\nXYZ\nend", vec![], vec![Block::Paragraph, bullet(), Block::Paragraph]);
    e.set_paragraph_attrs(vec![d.id(1), d.id(2).style(2).indent(10.0, 0.0), d.id(3).style(3)]);
    e.set_paragraph_ids(10);
    e.set_history(false);
    e.take_changes();
    e
}

fn row(text: &str, id: u64, style: u16, indent: f32, atom: Option<f32>, block: Block) -> Row {
    (text.into(), id, style, ParagraphAlign::Left, (indent, 0.0), atom, block)
}

fn aaa() -> Row { row("AAA", 1, 0, 0.0, None, Block::Paragraph) }

/// What `edit` leaves of `e` with `from..to` selected, and the same after undo and redo.
fn leaves(e: TextEdit, (from, to): (usize, usize), name: &str, edit: fn(&mut TextEdit)) -> Vec<Row> {
    let got = all_rows(e.clone(), |e| {
        e.set_selection(from, to);
        edit(e);
    });
    let mut e = e;
    e.set_history(true);
    e.set_selection(from, to);
    let (text, attrs, blocks) = (e.text.clone(), e.paragraph_attrs().to_vec(), e.blocks().to_vec());
    edit(&mut e);
    let (done, done_attrs, done_blocks) = (e.text.clone(), e.paragraph_attrs().to_vec(), e.blocks().to_vec());
    e.undo();
    assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec(), e.blocks().to_vec()), (text, attrs, blocks), "{name}: undo");
    e.redo();
    assert_eq!((e.text.clone(), e.paragraph_attrs().to_vec(), e.blocks().to_vec()), (done, done_attrs, done_blocks), "{name}: redo");
    got
}

#[test]
fn over_a_whole_atom_every_path_leaves_the_next_paragraph_its_id() {
    let end = |t: &str| row(t, 3, 3, 0.0, None, bullet());
    // The atom and its break (4..12), and the atom and the start of `end` (4..14).
    for (to, tail) in [(12, "end"), (14, "d")] {
        let sel = (4, to);
        for (name, edit) in BREAK {
            assert_eq!(leaves(atom_between(), sel, name, edit), vec![aaa(), plain_row("A"), end(tail)], "{name} over {sel:?}");
        }
        for (name, edit) in OPEN {
            let joined = format!("B{tail}");
            assert_eq!(leaves(atom_between(), sel, name, edit), vec![aaa(), plain_row("A"), end(&joined)], "{name} over {sel:?}");
        }
        let got = leaves(atom_between(), sel, "Return", return_key);
        assert_eq!(got, vec![aaa(), ("".into(), NEW, 3, ParagraphAlign::Left, (0.0, 0.0), None, Block::Paragraph), end(tail)], "Return over {sel:?}");
    }
}

#[test]
fn a_selection_that_ends_at_a_later_lines_start_leaves_that_line_whole() {
    let item = |t: &str| row(t, 2, 2, 10.0, None, bullet());
    let end = |t: &str| row(t, 3, 3, 0.0, None, Block::Paragraph);
    for (name, edit) in BREAK {
        assert_eq!(leaves(list_before_plain(), (5, 8), name, edit), vec![row("top", 1, 0, 0.0, None, Block::Paragraph), item("XA"), end("end")], "{name}");
    }
    for (name, edit) in OPEN {
        assert_eq!(leaves(list_before_plain(), (5, 8), name, edit), vec![row("top", 1, 0, 0.0, None, Block::Paragraph), item("XA"), end("Bend")], "{name}");
    }
    let got = leaves(list_before_plain(), (5, 8), "Return", return_key);
    assert_eq!(got, vec![row("top", 1, 0, 0.0, None, Block::Paragraph), item("X"), end("end")], "Return");
}

#[test]
fn without_attributes_a_selection_that_ends_at_a_later_lines_start_leaves_its_block() {
    let plain = || TextEdit::from_parts("top\nXYZ\nend", vec![], vec![Block::Paragraph, bullet(), Block::Paragraph]);
    let blocks = [Block::Paragraph, bullet(), Block::Paragraph];
    let edits: [Path; 8] = [
        ("insert", |e| e.insert("A\n")),
        ("paste", |e| { e.paste("A\n"); }),
        ("paste_html unmarked", |e| { e.paste_html("<p>A</p><p></p>", "A\n"); }),
        ("paste_html marked", |e| { e.paste_html("<p><b>A</b></p><p></p>", "A\n"); }),
        ("insert open", |e| e.insert("A\nB")),
        ("paste open", |e| { e.paste("A\nB"); }),
        ("paste_html open unmarked", |e| { e.paste_html("<p>A</p><p>B</p>", "A\nB"); }),
        ("paste_html open marked", |e| { e.paste_html("<p><b>A</b></p><p>B</p>", "A\nB"); }),
    ];
    for (name, edit) in edits.into_iter().chain([("Return", return_key as fn(&mut TextEdit))]) {
        let mut e = plain();
        e.set_selection(5, 8);
        edit(&mut e);
        assert_eq!(e.blocks(), blocks, "{name}: {:?}", lines(&e));
    }
}

#[test]
fn a_bare_line_break_at_a_lines_start_copies_nothing_of_the_line_but_return_does() {
    for (name, edit) in [("insert", (|e: &mut TextEdit| e.insert("\n")) as fn(&mut TextEdit)), ("paste", |e| { e.paste("\n"); })] {
        let got = at_start(start_field("XYZ", false), edit);
        assert_eq!(got, vec![new_row("", 0, ParagraphAlign::Left, None, Block::Paragraph), kept_row("XYZ", None, bullet())], "{name}");
    }
    let got = at_start(start_field("XYZ", false), |e| { e.key(&enter(), None); });
    assert_eq!(got, vec![above("", false), kept_row("XYZ", None, bullet())]);
}

#[test]
fn a_single_line_pasted_into_an_empty_line_with_only_an_id_gives_it_the_pasted_attributes() {
    let mut e = TextEdit::from_parts("top\n\nend", vec![], vec![]);
    let d = ParagraphAttrs::default();
    e.set_paragraph_attrs(vec![d.id(1), d.id(2), d.id(3)]);
    e.set_paragraph_ids(10);
    e.set_history(false);
    let got = pasted_between(e, start_fragment("A", false));
    assert_eq!(got, vec![("A".into(), 2, 5, ParagraphAlign::Center, (0.0, 0.0), None, bullet())]);
}

/// The paths that put `A`, `B` and `C`, or `A` to `D`, on lines of their own.
const THREE: [Path; 5] = [
    ("insert", |e| e.insert("A\nB\nC")),
    ("paste", |e| { e.paste("A\nB\nC"); }),
    ("paste_html unmarked", |e| { e.paste_html("<p>A</p><p>B</p><p>C</p>", "A\nB\nC"); }),
    ("paste_html marked", |e| { e.paste_html("<p><b>A</b></p><p>B</p><p>C</p>", "A\nB\nC"); }),
    ("paste_fragment", |e| { e.paste_fragment(TextEdit::from_parts("A\nB\nC", vec![], vec![]), ""); }),
];
const FOUR: [Path; 5] = [
    ("insert", |e| e.insert("A\nB\nC\nD\n")),
    ("paste", |e| { e.paste("A\nB\nC\nD\n"); }),
    ("paste_html unmarked", |e| { e.paste_html("<p>A</p><p>B</p><p>C</p><p>D</p><p></p>", "A\nB\nC\nD\n"); }),
    ("paste_html marked", |e| { e.paste_html("<p><b>A</b></p><p>B</p><p>C</p><p>D</p><p></p>", "A\nB\nC\nD\n"); }),
    ("paste_fragment", |e| { e.paste_fragment(TextEdit::from_parts("A\nB\nC\nD\n", vec![], vec![]), ""); }),
];

fn typed(e: &mut TextEdit, s: &str) {
    e.key(&KeyEvent { key: Key::Text(s.into()), mods: Mods::default() }, None);
}

/// One character, and a bare line break, on every path including typing.
const ONE: [Path; 6] = [
    ("typed", |e| typed(e, "x")),
    ("insert", |e| e.insert("x")),
    ("paste", |e| { e.paste("x"); }),
    ("paste_html unmarked", |e| { e.paste_html("<p>x</p>", "x"); }),
    ("paste_html marked", |e| { e.paste_html("<p><b>x</b></p>", "x"); }),
    ("paste_fragment", |e| { e.paste_fragment(TextEdit::from_parts("x", vec![], vec![]), ""); }),
];
const NEWLINE: [Path; 6] = [
    ("typed", |e| typed(e, "\n")),
    ("insert", |e| e.insert("\n")),
    ("paste", |e| { e.paste("\n"); }),
    ("paste_html unmarked", |e| { e.paste_html("<p></p><p></p>", "\n"); }),
    ("paste_html marked", |e| { e.paste_html("<p></p><p></p>", "\n"); }),
    ("paste_fragment", |e| { e.paste_fragment(TextEdit::from_parts("\n", vec![], vec![]), ""); }),
];

#[test]
fn every_path_agrees_over_every_shape_of_selection() {
    let docs: [(&str, fn() -> TextEdit, Vec<(usize, usize)>); 2] = [
        // The atom's line is 4..11. Its start, its end, the break before it, and what
        // follows it; and from after it to the end of the next line.
        ("atom", atom_between, vec![(4, 12), (4, 14), (0, 12), (0, 14), (2, 12), (4, 11), (4, 4), (0, 4), (3, 4), (1, 4), (11, 11), (11, 15)]),
        // Over lines, from the middle of one to the middle of another, and to a line's start.
        ("list", list_before_plain, vec![(4, 8), (4, 10), (5, 8), (5, 10), (0, 8), (0, 10), (2, 8), (4, 4), (5, 5), (7, 7), (5, 9), (6, 9), (7, 9), (7, 8), (6, 10)]),
    ];
    for (doc, make, shapes) in docs {
        for sel in shapes {
            let sets: [(&str, &[Path]); 6] = [("break", &BREAK), ("open", &OPEN), ("three", &THREE), ("four", &FOUR), ("one", &ONE), ("newline", &NEWLINE)];
            for (kind, paths) in sets {
                let first = leaves(make(), sel, paths[0].0, paths[0].1);
                for (name, edit) in &paths[1..] {
                    assert_eq!(leaves(make(), sel, name, *edit), first, "{doc} {kind}: {name} against {} over {sel:?}", paths[0].0);
                }
            }
        }
    }
}

#[test]
fn new_paragraphs_in_the_middle_of_a_line_are_defaults_on_every_path() {
    let item = |t: &str| row(t, 2, 2, 10.0, None, bullet());
    let tail = |t: &str| row(t, NEW, 2, 10.0, None, bullet());
    for (name, edit) in THREE {
        let got = leaves(list_before_plain(), (5, 5), name, edit);
        let want = vec![row("top", 1, 0, 0.0, None, Block::Paragraph), item("XA"), plain_row("B"), tail("CYZ"), row("end", 3, 3, 0.0, None, Block::Paragraph)];
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn an_atoms_label_selected_without_its_break_leaves_its_paragraph_as_text_on_every_path() {
    // `Tabelle` is selected (4..11): the paragraph keeps its style and loses the lock.
    let text = |t: &str| row(t, NEW, 2, 0.0, None, Block::Paragraph);
    let end = row("end", 3, 3, 0.0, None, bullet());
    for (name, edit) in BREAK {
        assert_eq!(leaves(atom_between(), (4, 11), name, edit), vec![aaa(), plain_row("A"), text(""), end.clone()], "{name}");
    }
    for (name, edit) in OPEN {
        assert_eq!(leaves(atom_between(), (4, 11), name, edit), vec![aaa(), plain_row("A"), text("B"), end.clone()], "{name}");
    }
    let typed: [Path; 3] = [("insert", |e| e.insert("x")), ("paste", |e| { e.paste("x"); }), ("paste_html marked", |e| { e.paste_html("<b>x</b>", "x"); })];
    for (name, edit) in typed {
        assert_eq!(leaves(atom_between(), (4, 11), name, edit), vec![aaa(), text("x"), end.clone()], "{name}");
    }
}

// An edit that starts in the middle of a line, ends in a later one and inserts a line break
// leaves the first line its own and ends in the later paragraph: its id, attributes and block.

#[test]
fn text_with_a_break_from_the_middle_of_a_line_to_the_middle_of_a_later_one_ends_in_the_later_paragraph() {
    let top = row("top", 1, 0, 0.0, None, Block::Paragraph);
    let item = row("XA", 2, 2, 10.0, None, bullet());
    let later = |t: &str| row(t, 3, 3, 0.0, None, Block::Paragraph);
    for (name, edit) in BREAK {
        assert_eq!(leaves(list_before_plain(), (5, 9), name, edit), vec![top.clone(), item.clone(), later("nd")], "{name}");
    }
    for (name, edit) in OPEN {
        assert_eq!(leaves(list_before_plain(), (5, 9), name, edit), vec![top.clone(), item.clone(), later("Bnd")], "{name}");
    }
    for (name, edit) in THREE {
        let got = leaves(list_before_plain(), (5, 9), name, edit);
        assert_eq!(got, vec![top.clone(), item.clone(), plain_row("B"), later("Cnd")], "{name}");
    }
    // The same in a field without attributes: the later paragraph's block.
    let blocks = [Block::Paragraph, bullet(), Block::Paragraph];
    for (name, edit) in OPEN {
        let mut e = TextEdit::from_parts("top\nXYZ\nend", vec![], blocks.to_vec());
        e.set_selection(5, 9);
        edit(&mut e);
        assert_eq!((lines(&e), e.blocks().to_vec()), (vec!["top", "XA", "Bnd"], blocks.to_vec()), "{name}");
    }
}

#[test]
fn text_over_the_break_behind_an_atom_leaves_the_next_paragraph_its_block() {
    let atom = row("Tabelle", 2, 2, 0.0, Some(40.0), Block::Paragraph);
    for sel in [(11, 12), (11, 13)] {
        let tail = if sel.1 == 12 { "xend" } else { "xnd" };
        for (name, edit) in ONE {
            let got = leaves(atom_between(), sel, name, edit);
            assert_eq!(got, vec![aaa(), atom.clone(), row(tail, 3, 3, 0.0, None, bullet())], "{name} over {sel:?}");
        }
    }
}

// An atom copies and pastes whole in every block, a spaced rgb() highlight reads, a paste of what
// is selected leaves the caret behind it, and unformatted HTML pastes as unmarked text.

#[test]
fn an_atom_copies_and_pastes_whole_in_every_block() {
    for blk in [Block::Paragraph, Block::Bullet(0), Block::Numbered(0), Block::Quote, Block::Code] {
        let d = ParagraphAttrs::default();
        let mut e = TextEdit::from_parts("aa\nTabelle\nbb", vec![], vec![Block::Paragraph, blk, Block::Paragraph]);
        e.set_paragraph_attrs(vec![d.id(1), d.id(2).atom(40.0), d.id(3)]);
        e.set_selection(0, e.text.len());
        let html = e.selected_html();
        let frag = e.html_fragment(&html);
        assert_eq!(frag.text, "aa\nTabelle\nbb", "{blk:?}: {html}");
        assert_eq!((frag.paragraph(1).atom, frag.paragraph(1).id, frag.blocks()[1]), (Some(40.0), 2, blk), "{blk:?}: {html}");
        assert!(frag.paragraph(0).atom.is_none() && frag.paragraph(2).atom.is_none(), "{blk:?}: {html}");
        // and pasted into another field it is an atom of that block under an id of its own
        let mut t = TextEdit::from_parts("x\n\ny", vec![], vec![Block::Paragraph; 3]);
        t.set_paragraph_attrs(vec![d.id(1), d.id(2), d.id(3)]);
        t.set_paragraph_ids(10);
        t.set_caret(2);
        t.paste_fragment(frag, "");
        let at = (0..t.blocks().len()).find(|&i| t.paragraph(i).atom.is_some()).expect("an atom");
        assert_eq!((t.text.split('\n').nth(at), t.blocks()[at]), (Some("Tabelle"), blk), "{blk:?}");
        assert!(t.paragraph(at).id >= 10, "{blk:?}: {:?}", t.paragraph_attrs());
    }
}

#[test]
fn a_spaced_rgb_background_keeps_its_highlight() {
    for css in ["rgb(255, 255, 0)", "rgb(255,255,0)", "RGB( 255 , 255 , 0 )", "#ffff00"] {
        let mut e = start_field("XYZ", false);
        e.set_caret(4);
        e.paste_html(&format!("<p><span style=\"background-color: {css}\">hi</span></p>"), "hi");
        assert!(e.marks().iter().any(|m| matches!(m.mark, Mark::Highlight(_)) && m.range() == (4..6)), "{css}: {:?}", e.marks());
    }
}

#[test]
fn a_paste_of_what_is_selected_leaves_the_caret_behind_it() {
    let mut e = start_field("XYZ", false);
    e.set_selection(4, 7);
    let mut f = start_fragment("XYZ", false);
    f.set_paragraph_attrs(vec![ParagraphAttrs::default().id(9).style(3).align(ParagraphAlign::Right).indent(12.0, 4.0)]);
    e.paste_fragment(f, "XYZ");
    assert_eq!(e.range(), (7, 7));
}

#[test]
fn unformatted_html_pastes_as_unmarked_text_where_the_caret_is_marked() {
    let mut e = TextEdit::from_parts("abcdef", vec![MarkRange::new(0..6, Mark::Bold)], vec![]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default().id(1)]);
    e.set_caret(3);
    e.paste_html("<p>x</p>", "");
    assert_eq!(e.text, "abcxdef");
    assert!(!e.marks().iter().any(|m| m.start <= 3 && m.end > 3), "{:?}", e.marks());
}
