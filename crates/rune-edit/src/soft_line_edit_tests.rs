//! Soft lines and atoms: the edits that reach an atom from a soft line, `insert_atom`, a pasted atom beside soft lines,
//! the ids turned on and the armed typing style. The helpers are `soft_line_tests`'.

use super::soft_line_tests::{atoms_kept, id, inv, key, show, st};
use super::*;

fn atom_at_start_doc() -> TextEdit {
    let p = Block::Paragraph;
    let mut e = TextEdit::from_parts("ab\nTbl\nef", vec![], vec![p, p, p]);
    e.set_paragraph_attrs(vec![id(1), id(2).atom(20.0), id(3)]);
    e.set_paragraph_ids(100);
    e.take_changes();
    e
}

/// An empty edit at an atom's start leaves the atom whole while a typing style is armed, reports nothing and records no
/// undo step, and the armed style stays; an atom inserted there goes on a line of its own, before it.
#[test]
fn an_empty_edit_at_an_atoms_start_leaves_it_alone_with_a_style_armed() {
    let e = atom_at_start_doc();
    let calls: Vec<(&str, Box<dyn Fn(&mut TextEdit)>)> = vec![
        ("insert", Box::new(|x| x.insert(""))),
        ("paste", Box::new(|x| { x.paste(""); })),
        ("paste_html empty p", Box::new(|x| { x.paste_html("<p></p>", ""); })),
        ("paste_html img", Box::new(|x| { x.paste_html("<img src=\"x.png\">", ""); })),
    ];
    for (name, call) in calls {
        let mut x = e.clone();
        x.set_caret(3);
        x.toggle_mark(Mark::Bold);
        x.take_changes();
        let steps = x.history_len();
        call(&mut x);
        assert_eq!(atoms_kept(&e, &x, 3, 3), "", "{name}: {}", show(&x));
        assert_eq!(st(&x), st(&e), "{name}: {}", show(&x));
        assert!(x.take_changes().is_empty(), "{name}: nothing reported");
        assert_eq!(x.history_len(), steps, "{name}: no undo step");
        assert!(x.has_mark(&Mark::Bold), "{name}: the armed style stays");
    }
    let mut x = e.clone();
    x.set_caret(3);
    x.insert_atom("N", 20.0);
    assert_eq!(x.text, "ab\nN\nTbl\nef", "{}", show(&x));
    assert_eq!(atoms_kept(&e, &x, 3, 3), "", "{}", show(&x));
    assert_eq!((x.paragraph(1).atom, x.paragraph(2).atom, x.paragraph(2).id), (Some(20.0), Some(20.0), 2), "{}", show(&x));
    let mut y = e.clone();
    y.set_caret(3);
    y.toggle_mark(Mark::Bold);
    y.insert_atom("N", 20.0);
    assert_eq!(atoms_kept(&e, &y, 3, 3), "", "armed: {}", show(&y));
    assert_eq!(y.text, "ab\nN\nTbl\nef", "armed: {}", show(&y));
}

/// A break typed or pasted at the end of a paragraph's first line leaves what remains of the paragraph, behind the break,
/// its attributes and block: its trailing soft lines are the rest of it.
#[test]
fn a_break_at_the_end_of_a_first_line_keeps_the_paragraph_whole() {
    let (p, u) = (Block::Paragraph, Block::Bullet(0));
    for attributed in [true, false] {
        let mut e = TextEdit::from_parts("ab\ncd\nef", vec![], vec![u, u, p]);
        if attributed { e.set_paragraph_attrs(vec![id(1).align(ParagraphAlign::Center), id(2).align(ParagraphAlign::Center), id(3)]); e.set_paragraph_ids(100); }
        e.set_soft_breaks(vec![1]);
        e.take_changes();
        for how in ["insert", "paste", "return"] {
            let mut x = e.clone();
            x.set_caret(2);
            match how { "insert" => x.insert("\n"), "paste" => { x.paste("\n"); } _ => { x.key(&key(Key::Enter, false), None); } }
            assert_eq!(x.text, "ab\n\ncd\nef", "{how}");
            assert_eq!(x.soft_breaks(), &[2], "attributed {attributed} {how}: {}", show(&x));
            assert_eq!(x.blocks().to_vec(), vec![u, u, u, p], "attributed {attributed} {how}: {}", show(&x));
            if attributed { assert_eq!((x.paragraph(1).align, x.paragraph(2).align), (ParagraphAlign::Center, ParagraphAlign::Center), "{how}: {}", show(&x)); }
            assert_eq!(inv(&x), "", "{how}");
        }
    }
}

/// A mark command over code reports nothing: the marks did not change.
#[test]
fn a_mark_command_over_code_changes_nothing_and_reports_nothing() {
    let c = Block::Code;
    let mut e = TextEdit::from_parts("ab\ncd", vec![], vec![c, c]);
    e.set_selection(0, 1);
    e.take_changes();
    let steps = e.history_len();
    e.toggle_mark(Mark::Bold);
    assert!(e.marks().is_empty());
    assert!(e.take_changes().is_empty(), "nothing reported");
    assert_eq!(e.history_len(), steps);
}

/// The atom of an empty line is cleared whichever way round the attributes and the ids come.
#[test]
fn an_empty_line_atom_cleared_at_load_gets_an_id_in_either_order() {
    let p = Block::Paragraph;
    let attrs = || vec![id(1), id(2).atom(20.0), id(3)];
    let mut a = TextEdit::from_parts("ab\n\ncd", vec![], vec![p, p, p]);
    a.set_paragraph_ids(100);
    a.set_paragraph_attrs(attrs());
    let mut b = TextEdit::from_parts("ab\n\ncd", vec![], vec![p, p, p]);
    b.set_paragraph_attrs(attrs());
    b.set_paragraph_ids(100);
    for (name, e) in [("ids first", &a), ("attributes first", &b)] {
        assert_eq!(e.paragraph(1).atom, None, "{name}");
        let ids: Vec<u64> = (0..3).map(|i| e.paragraph(i).id).collect();
        assert!(ids.iter().all(|&i| i != 0), "{name}: {ids:?}");
        let mut u = ids.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), 3, "{name}: {ids:?}");
    }
}

/// A whole-document export keeps a trailing empty code line as code: `from_html(to_html())` is the document.
#[test]
fn a_document_ending_in_an_empty_code_line_round_trips_through_html() {
    let (p, c) = (Block::Paragraph, Block::Code);
    for (text, blocks) in [("ab\nef\n", vec![p, c, c]), ("ab\n", vec![p, c]), ("\n", vec![c, c]), ("ab\n\n", vec![p, c, c])] {
        let e = TextEdit::from_parts(text, vec![], blocks.clone());
        let r = TextEdit::from_html(&e.to_html());
        assert_eq!((r.text.as_str(), r.blocks().to_vec()), (text, blocks), "{text:?}: {}", e.to_html());
    }
}

/// The bytes an edit took: where the text before and after differ, when it only took (a deletion key reaches as far as
/// the word or the line it deletes), else the selection.
fn acted_on(before: &str, after: &str, a: usize, b: usize) -> (usize, usize) {
    if after.len() >= before.len() { return (a, b); }
    let p = before.bytes().zip(after.bytes()).take_while(|(x, y)| x == y).count();
    let s = before.bytes().rev().zip(after.bytes().rev()).take_while(|(x, y)| x == y).count().min(after.len() - p);
    (p, before.len() - s)
}

fn atom_below_soft_docs() -> Vec<(String, TextEdit)> {
    let (p, u, q, c) = (Block::Paragraph, Block::Bullet(0), Block::Quote, Block::Code);
    let mut out = Vec::new();
    for (name, text, blocks, soft) in [
        ("empty soft line", "ab\n\nTbl\nef", vec![u, u, p, p], vec![1u32]),
        ("soft line", "ab\ncd\nTbl\nef", vec![u, u, p, p], vec![1]),
        ("quote", "ab\ncd\nTbl\nef", vec![q, q, p, p], vec![1]),
        ("code atom", "ab\ncd\nTbl\nef", vec![u, u, c, p], vec![1]),
        ("two soft lines", "ab\ncd\nxy\nTbl\nef", vec![q, q, q, p, p], vec![1, 2]),
    ] {
        let atom = text.split('\n').position(|l| l == "Tbl").unwrap();
        let mut e = TextEdit::from_parts(text, vec![], blocks.clone());
        e.set_paragraph_attrs((0..blocks.len()).map(|i| if i == atom { id(1 + i as u64).atom(20.0).align(ParagraphAlign::Center) } else { id(1 + i as u64) }).collect());
        e.set_paragraph_ids(100);
        e.set_soft_breaks(soft);
        e.take_changes();
        out.push((name.to_string(), e));
    }
    out
}

/// An atom whose block and alignment differ from the soft line directly above it keeps both through every edit that
/// reaches it from that line: the delete family, the empty edits, Shift+Return and `insert_atom`.
#[test]
fn an_atom_below_a_soft_line_keeps_its_block_and_alignment() {
    let alt = |k: Key| KeyEvent { key: k, mods: Mods { alt: true, ..Mods::default() } };
    let cmd = |k: Key| KeyEvent { key: k, mods: Mods { cmd: true, ..Mods::default() } };
    type Op = Box<dyn Fn(&mut TextEdit)>;
    let ops: Vec<(&str, Op)> = vec![
        ("bksp", Box::new(|x| { x.key(&key(Key::Backspace, false), None); })),
        ("del", Box::new(|x| { x.key(&key(Key::Delete, false), None); })),
        ("alt bksp", Box::new(move |x| { x.key(&alt(Key::Backspace), None); })),
        ("cmd bksp", Box::new(move |x| { x.key(&cmd(Key::Backspace), None); })),
        ("alt del", Box::new(move |x| { x.key(&alt(Key::Delete), None); })),
        ("cmd del", Box::new(move |x| { x.key(&cmd(Key::Delete), None); })),
        ("insert empty", Box::new(|x| x.insert(""))),
        ("paste empty", Box::new(|x| { x.paste(""); })),
        ("shift return", Box::new(|x| { x.key(&key(Key::Enter, true), None); })),
        ("insert_atom", Box::new(|x| x.insert_atom("N", 20.0))),
    ];
    let mut fails: Vec<String> = Vec::new();
    for (name, e) in atom_below_soft_docs() {
        let len = e.text.len();
        for a in 0..=len { for b in a..=len {
            for (op, f) in &ops {
                let mut x = e.clone();
                x.set_selection(a, b);
                if x.range() != (a, b) { continue; }
                f(&mut x);
                let (wa, wb) = acted_on(&e.text, &x.text, a, b);
                let bad = atoms_kept(&e, &x, wa, wb);
                if !bad.is_empty() || inv(&x) != "" { fails.push(format!("{name} {a}..{b} {op}:{bad} {}\n   from {}\n   to   {}", inv(&x), show(&e), show(&x))); }
            }
        } }
    }
    assert!(fails.is_empty(), "{} failures, the first:\n{}", fails.len(), fails.iter().take(5).cloned().collect::<Vec<_>>().join("\n"));
}

/// `insert_atom` always makes a new paragraph: a fresh id, the default attributes and the Paragraph block, whatever the
/// position, and it clears the typing style.
#[test]
fn insert_atom_makes_a_plain_paragraph_with_a_fresh_id_anywhere() {
    let (p, u) = (Block::Paragraph, Block::Bullet(0));
    let mut base = TextEdit::from_parts("ab\ncd\n\nef", vec![], vec![u, u, u, p]);
    base.set_paragraph_attrs(vec![id(1).align(ParagraphAlign::Center).style(2), id(2), id(3).align(ParagraphAlign::Center), id(4)]);
    base.set_paragraph_ids(100);
    base.set_soft_breaks(vec![1]);
    base.take_changes();
    // (a, b, what): line end, a soft line's start, mid-line, over a selection, on an empty line, at a paragraph's start
    for (a, b, what) in [(2, 2, "end of the head"), (3, 3, "soft line start"), (4, 4, "mid soft line"), (1, 4, "over a selection"), (6, 6, "empty line"), (0, 0, "paragraph start"), (5, 5, "end of the soft line")] {
        for armed in [false, true] {
            let mut x = base.clone();
            x.set_selection(a, b);
            if armed { x.toggle_mark(Mark::Bold); }
            let old: Vec<u64> = (0..4).map(|i| base.paragraph(i).id).collect();
            x.insert_atom("N", 20.0);
            let n = x.text.split('\n').position(|l| l == "N").unwrap_or_else(|| panic!("{what}: {}", show(&x)));
            let at = x.paragraph(n);
            assert_eq!(at.atom, Some(20.0), "{what} armed {armed}");
            assert!(!old.contains(&at.id) && at.id != 0, "{what} armed {armed}: id {} is new: {}", at.id, show(&x));
            assert_eq!((at.style, at.align, at.indent, at.page_break_before), (0, ParagraphAlign::Left, 0.0, false), "{what} armed {armed}: default attributes: {}", show(&x));
            assert_eq!(x.blocks()[n], p, "{what} armed {armed}: Paragraph block: {}", show(&x));
            assert_eq!(inv(&x), "", "{what} armed {armed}: {}", show(&x));
            // The style armed before is gone: text typed after it is plain.
            x.key(&key(Key::Text("t".into()), false), None);
            assert!(!x.marks().iter().any(|m| m.mark == Mark::Bold && x.text[m.start..m.end].contains('t')), "{what}: the typing style is cleared");
        }
    }
}

/// A fragment's atom pasted at the end of a head line whose soft lines follow leaves no empty paragraph: the soft lines
/// start the next one, as `insert_atom` does it.
#[test]
fn a_pasted_atom_at_the_end_of_a_head_line_leaves_no_empty_paragraph() {
    let p = Block::Paragraph;
    let mut e = TextEdit::from_parts("ab\ncd\nef", vec![], vec![p, p, p]);
    e.set_paragraph_attrs(vec![id(1), id(2), id(3)]);
    e.set_paragraph_ids(100);
    e.set_soft_breaks(vec![1]);
    e.take_changes();
    let mut by_insert = e.clone();
    by_insert.set_caret(2);
    by_insert.insert_atom("T", 20.0);
    let mut f = TextEdit::from_parts("T", vec![], vec![p]);
    f.set_paragraph_attrs(vec![id(500).atom(20.0)]);
    let mut x = e.clone();
    x.set_caret(2);
    x.paste_fragment(f, "");
    assert_eq!(x.text, "ab\nT\ncd\nef", "{}", show(&x));
    assert_eq!(x.text, by_insert.text);
    assert_eq!(x.soft_breaks(), by_insert.soft_breaks(), "{} / {}", show(&x), show(&by_insert));
    assert_eq!(inv(&x), "");
}

/// With ids on, setting the attributes gives an id to any line that has none, so the order of the two calls does not matter.
#[test]
fn set_paragraph_attrs_and_ids_end_the_same_in_either_order() {
    let p = Block::Paragraph;
    let attrs = || vec![id(1), ParagraphAttrs::default(), id(3)];
    let mut a = TextEdit::from_parts("ab\ncd\nef", vec![], vec![p, p, p]);
    a.set_paragraph_ids(100);
    a.set_paragraph_attrs(attrs());
    let mut b = TextEdit::from_parts("ab\ncd\nef", vec![], vec![p, p, p]);
    b.set_paragraph_attrs(attrs());
    b.set_paragraph_ids(100);
    for (name, e) in [("ids first", &a), ("attributes first", &b)] {
        assert!((0..3).all(|i| e.paragraph(i).id != 0), "{name}: {}", show(e));
    }
    assert_eq!(a.next_paragraph_id(), b.next_paragraph_id());
    // And one line, set after the ids.
    let mut c = TextEdit::from_parts("ab\ncd\nef", vec![], vec![p, p, p]);
    c.set_paragraph_attrs(vec![id(1), id(2), id(3)]);
    c.set_paragraph_ids(100);
    c.set_paragraph(1, ParagraphAttrs::default().align(ParagraphAlign::Center));
    assert!(c.paragraph(1).id != 0, "set_paragraph: {}", show(&c));
}

/// An empty one-line fragment pasted on an empty plain line keeps the armed typing style.
#[test]
fn an_empty_fragment_on_an_empty_line_keeps_the_armed_style() {
    let mut e = TextEdit::from_parts("ab\n", vec![], vec![Block::Paragraph; 2]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default(); 2]);
    e.set_caret(3);
    e.toggle_mark(Mark::Bold);
    let f = TextEdit::from_parts("", vec![], vec![Block::Paragraph]);
    e.paste_fragment(f, "");
    assert!(e.has_mark(&Mark::Bold), "the armed style stays");
}

fn nondefault_docs() -> Vec<(&'static str, TextEdit)> {
    let (p, u, q) = (Block::Paragraph, Block::Bullet(0), Block::Quote);
    let d = ParagraphAttrs::default;
    let c = |i: u64| d().id(i).align(ParagraphAlign::Center).indent(10.0, 0.0).style(2);
    let mut soft = TextEdit::from_parts("ab\ncd\nxy\nef", vec![], vec![u, u, u, p]);
    soft.set_paragraph_attrs(vec![c(1), c(2), c(3), d().id(4)]);
    soft.set_paragraph_ids(100);
    soft.set_soft_breaks(vec![1, 2]);
    soft.take_changes();
    let mut hard = TextEdit::from_parts("ab\ncd\nxy\nef", vec![], vec![u, u, u, p]);
    hard.set_paragraph_attrs(vec![c(1), c(2), c(3), d().id(4)]);
    hard.set_paragraph_ids(100);
    hard.take_changes();
    let plain = TextEdit::from_parts("ab\ncd\nxy\nef", vec![], vec![q, q, q, p]).with_soft_breaks(vec![1, 2]);
    vec![("soft", soft), ("hard", hard), ("plain-soft", plain)]
}

/// What the lines of an edit are, ids aside: text, softness, blocks, and the attributes a paste can say.
fn shape(e: &TextEdit) -> (String, Vec<u32>, Vec<Block>, Vec<(ParagraphAlign, f32, u16, bool)>) {
    (e.text.clone(), e.soft_breaks().to_vec(), e.blocks().to_vec(), e.paragraph_attrs().iter().map(|a| (a.align, a.indent, a.style, a.atom.is_some())).collect())
}

/// An atom goes in the same by `insert_atom` and by a pasted fragment: on a line of its own with the defaults, the rest of
/// the paragraph behind it (text or soft lines) keeping the attributes and block it had, at every caret and selection,
/// at a soft line's start too (no empty soft line): one code path, so the same documented rules over every selection.
#[test]
fn insert_atom_and_a_pasted_atom_agree_in_paragraphs_with_soft_lines() {
    let mut fails: Vec<String> = Vec::new();
    // (A field without paragraph attributes has no atoms: `insert_atom` makes it attributed first, and is compared with an
    // attributed paste.)
    for (name, e) in nondefault_docs() {
        let len = e.text.len();
        for (a, b) in (0..=len).flat_map(|a| (a..=len).map(move |b| (a, b))) {
            let mut i = e.clone();
            i.set_selection(a, b);
            i.insert_atom("N", 20.0);
            let mut f = e.clone();
            if !f.attributed() { f = f.rich(); f.set_paragraph_attrs(vec![ParagraphAttrs::default(); f.text.split('\n').count()]); }
            f.set_selection(a, b);
            let fr = f.html_fragment("<p data-atom=\"9\" data-atom-height=\"20\">N</p>");
            f.paste_fragment(fr, "N");
            // Full equality: text, softness, blocks, attributes and atoms; ids are the old ones or fresh ones.
            let ids = |x: &TextEdit| x.paragraph_attrs().iter().map(|p| if p.id >= 100 { 0 } else { p.id }).collect::<Vec<_>>();
            let differ = shape(&i) != shape(&f) || ids(&i) != ids(&f) || i.soft_breaks() != f.soft_breaks();
            if differ || inv(&i) != "" || inv(&f) != "" {
                fails.push(format!("{name} {a}..{b}\n   insert_atom {}\n   fragment    {}", show(&i), show(&f)));
            }
        }
    }
    assert!(fails.is_empty(), "{} failures, the first:\n{}", fails.len(), fails.iter().take(4).cloned().collect::<Vec<_>>().join("\n"));
    // The remainder of a paragraph behind a break at the end of its first line keeps the paragraph's attributes.
    for (name, e) in nondefault_docs().into_iter().filter(|(n, _)| *n != "hard") {
        for how in ["insert", "paste"] {
            let mut x = e.clone();
            x.set_caret(2);
            if how == "insert" { x.insert("x\ny"); } else { x.paste("x\ny"); }
            let shape_of = |l: usize| (x.paragraph(l).align, x.paragraph(l).indent, x.paragraph(l).style, x.blocks()[l]);
            assert_eq!(shape_of(1), shape_of(0), "{name} {how}: y keeps the paragraph's attributes: {}", show(&x));
            assert_eq!(x.blocks()[1], e.blocks()[0], "{name} {how}: block");
        }
    }
}

/// Turning ids on clears the undo history (the states from before had no ids): undo after it brings nothing back.
#[test]
fn turning_ids_on_clears_the_history() {
    let p = Block::Paragraph;
    let mut e = TextEdit::from_parts("ab\ncd", vec![], vec![p, p]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default(); 2]);
    e.set_selection(2, 2);
    e.key(&key(Key::Text("x".into()), false), None);
    assert!(e.history_len().0 > 0, "a step is recorded");
    e.set_paragraph_ids(100);
    let (text, ids) = (e.text.clone(), (0..2).map(|i| e.paragraph(i).id).collect::<Vec<_>>());
    e.undo();
    assert_eq!(e.text, text, "undo brings nothing back");
    assert_eq!((0..2).map(|i| e.paragraph(i).id).collect::<Vec<_>>(), ids);
    e.redo();
    assert_eq!(e.text, text);
}

fn doc_kinds() -> Vec<(&'static str, TextEdit)> {
    let mut v = nondefault_docs();
    let plain = TextEdit::from_parts("ab\ncd\nxy\nef", vec![], vec![Block::Paragraph; 4]);
    v.push(("plain", plain.clone()));
    v.push(("plain-rich", plain.rich()));
    v.push(("atom-next", atom_at_start_doc()));
    v
}

/// The line of the new atom `N` in `x`.
fn new_atom_line(x: &TextEdit) -> Option<usize> {
    x.text.split('\n').enumerate().position(|(i, t)| t == "N" && x.paragraph(i).atom.is_some())
}

/// `insert_atom` works in any field, as before: a field without attributes becomes attributed, a field that is not rich
/// still gets the atom on a line of its own, and the caret ends at the new atom's end, at every caret and selection (also
/// before another atom).
#[test]
fn insert_atom_works_in_any_field_and_leaves_the_caret_at_the_new_atom() {
    let mut fails: Vec<String> = Vec::new();
    for (name, e) in doc_kinds() {
        let len = e.text.len();
        for (a, b) in (0..=len).flat_map(|a| (a..=len).map(move |b| (a, b))) {
            let mut x = e.clone();
            x.set_selection(a, b);
            if x.range() != (a, b) { continue; }
            x.insert_atom("N", 20.0);
            let Some(n) = new_atom_line(&x) else { fails.push(format!("{name} {a}..{b}: no atom: {}", show(&x))); continue; };
            let end = x.line_range(n).end;
            if (x.anchor, x.focus) != (end, end) { fails.push(format!("{name} {a}..{b}: caret {:?} not at {end}: {}", (x.anchor, x.focus), show(&x))); }
            if x.paragraph(n).atom != Some(20.0) || x.blocks()[n] != Block::Paragraph { fails.push(format!("{name} {a}..{b}: not a Paragraph atom: {}", show(&x))); }
        }
    }
    assert!(fails.is_empty(), "{} failures, the first:\n{}", fails.len(), fails.iter().take(5).cloned().collect::<Vec<_>>().join("\n"));
}

fn atom_fragment(shape: &str) -> TextEdit {
    let (text, atoms): (Vec<&str>, Vec<bool>) = shape.chars().map(|c| (if c == 'A' { "T" } else { "x" }, c == 'A')).unzip();
    let mut f = TextEdit::from_parts(text.join("\n"), vec![], vec![Block::Paragraph; text.len()]);
    f.set_paragraph_attrs(atoms.iter().enumerate().map(|(i, &a)| if a { id(500 + i as u64).atom(20.0) } else { id(500 + i as u64) }).collect());
    f
}

/// Fragments of several atoms pasted over every selection leave every id unique.
#[test]
fn several_pasted_atoms_get_ids_of_their_own() {
    let mut fails: Vec<String> = Vec::new();
    for (name, e) in nondefault_docs().into_iter().chain(std::iter::once(("atom-next", atom_at_start_doc()))).filter(|(n, _)| *n != "plain-soft") {
        let len = e.text.len();
        for (a, b) in (0..=len).flat_map(|a| (a..=len).map(move |b| (a, b))) {
            for shape in ["AA", "AAA", "AxA", "xAA", "AAx"] {
                let mut x = e.clone();
                x.set_selection(a, b);
                if x.range() != (a, b) { continue; }
                x.paste_fragment(atom_fragment(shape), "");
                let ids: Vec<u64> = x.paragraph_attrs().iter().map(|p| p.id).collect();
                let mut u = ids.clone();
                u.sort();
                u.dedup();
                if u.len() != ids.len() || ids.contains(&0) { fails.push(format!("{name} {a}..{b} {shape}: {ids:?}: {}", show(&x))); }
            }
        }
    }
    assert!(fails.is_empty(), "{} failures, the first:\n{}", fails.len(), fails.iter().take(5).cloned().collect::<Vec<_>>().join("\n"));
}

/// `insert_atom` always inserts: over an identical atom it gives a fresh id, one undo step and a change report.
#[test]
fn insert_atom_over_an_identical_atom_still_inserts() {
    let mut e = atom_at_start_doc();
    e.set_selection(3, 6);
    e.take_changes();
    let (old, steps) = (e.paragraph(1).id, e.history_len().0);
    e.insert_atom("Tbl", 20.0);
    assert_ne!(e.paragraph(1).id, old, "a fresh id: {}", show(&e));
    assert_eq!(e.history_len().0, steps + 1, "one undo step");
    assert!(!e.take_changes().is_empty(), "a change report");
}

/// Turning ids on inside an undo group with no history clears the group's baseline too: undo brings back no id-less state.
#[test]
fn turning_ids_on_in_an_undo_group_leaves_no_id_less_baseline() {
    let p = Block::Paragraph;
    let mut e = TextEdit::from_parts("ab\ncd", vec![], vec![p, p]);
    e.set_paragraph_attrs(vec![ParagraphAttrs::default(); 2]);
    e.undo_group(|e| {
        e.set_paragraph_ids(100);
        e.set_selection(2, 2);
        e.insert("x");
    });
    e.undo();
    assert!((0..2).all(|i| e.paragraph(i).id != 0), "undo keeps the ids: {}", show(&e));
}
