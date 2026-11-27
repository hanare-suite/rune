//! The change report in fields without ids and without attributes, empty fragments, and the shape
//! of a copy (where it begins and ends, in a single-line field, in code). The documents and
//! helpers are `path_agreement_tests`'.

use super::fuzz_tests::{check, seen};
use super::path_agreement_tests::{docs, fragments, link, mkfrag, mr, rows, show, soft_ok, PATHS};
use super::*;

/// Documents without ids, and fields without paragraph attributes, with marks, lists, code and soft lines.
fn plain_docs() -> Vec<(String, TextEdit)> {
    let (p, b, q, c, n) = (Block::Paragraph, Block::Bullet(0), Block::Quote, Block::Code, Block::Numbered(1));
    let mut out: Vec<(String, TextEdit)> = Vec::new();
    for (name, mut e) in docs() {
        let attrs: Vec<ParagraphAttrs> = e.paragraph_attrs().into_iter().map(|a| a.id(0)).collect();
        e.set_paragraph_ids(0);
        e.set_paragraph_attrs(attrs);
        e.take_changes();
        out.push((format!("idsoff-{name}"), e));
    }
    let field = |text: &str, marks: Vec<MarkRange>, bl: Vec<Block>, soft: Vec<u32>| {
        let mut e = TextEdit::from_parts(text, marks, bl);
        e.set_soft_breaks(soft);
        e.set_history(false);
        e.take_changes();
        e
    };
    out.push(("rich-list".into(), field("top\nXYZ\nend", vec![mr(0..2, Mark::Bold), mr(5..10, Mark::Italic)], vec![p, b, p], vec![])));
    out.push(("rich-mix".into(), field("ab\ncd\n\nef", vec![mr(1..4, Mark::Bold), mr(7..9, link())], vec![q, n, b, p], vec![])));
    out.push(("rich-code".into(), field("ab\ncd\nef\ngh", vec![mr(0..2, Mark::Bold), mr(9..11, Mark::Italic)], vec![p, c, c, b], vec![])));
    out.push(("rich-soft".into(), field("one\ntwo\nthr\nfou", vec![mr(2..6, Mark::Bold)], vec![b, b, p, p], vec![1, 3])));
    out.push(("rich-softq".into(), field("one\n\nthr", vec![], vec![q, q, q], vec![1, 2])));
    out.push(("rich-para".into(), field("one\ntwo\n", vec![mr(0..3, link())], vec![p, p, p], vec![])));
    let mut plain = TextEdit::new("one\ntwo\nthr");
    plain.set_history(false);
    plain.take_changes();
    out.push(("plain".into(), plain));
    out
}

/// The report of every path over every selection, in documents without ids and in fields without
/// attributes: the same checks as with ids, but the ones about ids.
#[test]
fn the_change_report_holds_without_ids_and_without_attributes() {
    let inputs = ["x", "\n", "A\n", "A\nB", "A\n\nB", "\nA", "\n\n"];
    let key = |k: Key, shift: bool| KeyEvent { key: k, mods: Mods { shift, ..Mods::default() } };
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (doc, e) in plain_docs() {
        let n = e.text.len();
        let attributed = doc.starts_with("idsoff");
        for a in 0..=n { for b in a..=n {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            let mut run = |name: &str, s: &str, f: &dyn Fn(&mut TextEdit)| {
                cases += 1;
                let mut x = e.clone();
                x.set_selection(a, b);
                let before = seen(&x);
                f(&mut x);
                let ch = x.take_changes();
                if let Err(w) = check(&before, &x, &ch) { fails.push(format!("{doc} {a}..{b} {:?} {name} {s:?}: {w}", &e.text[a..b])); }
                if !attributed && x.paragraph_attrs().iter().any(|p| *p != ParagraphAttrs::default()) { fails.push(format!("{doc} {a}..{b} {name} {s:?}: the field gained paragraph attributes")); }
                if x.paragraph_attrs().iter().any(|p| p.id != 0) { fails.push(format!("{doc} {a}..{b} {name} {s:?}: the field gained an id")); }
                if !soft_ok(&x) { fails.push(format!("{doc} {a}..{b} {name} {s:?}: an atom is a soft line")); }
            };
            for (pn, pf) in PATHS.iter() { for s in inputs { run(pn, s, &|x| pf(x, s)); } }
            run("return", "", &|x| { x.key(&key(Key::Enter, false), None); });
            run("shift", "", &|x| { x.key(&key(Key::Enter, true), None); });
            run("backspace", "", &|x| { x.key(&key(Key::Backspace, false), None); });
            for (fi, fr) in fragments().iter().enumerate() { run(&format!("frag#{fi}"), fr.0, &|x| { x.paste_fragment(mkfrag(fr), ""); }); }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

/// A fragment with no text but formatting (attributes only, a bullet, a quote, a centred paragraph,
/// an image the parser drops) pasted at a caret changes nothing, except a single line pasted into an
/// empty plain line, which takes its attributes; over a selection it is the plain deletion.
#[test]
fn an_empty_fragment_is_a_no_op_at_a_caret_and_a_deletion_over_a_selection() {
    let d = ParagraphAttrs::default();
    let frags: Vec<(&str, TextEdit)> = vec![
        ("attrs", { let mut f = TextEdit::from_parts("", vec![], vec![]); f.set_paragraph_attrs(vec![d.id(60)]); f }),
        ("bullet", TextEdit::from_parts("", vec![], vec![Block::Bullet(0)])),
        ("quote", TextEdit::from_parts("", vec![], vec![Block::Quote])),
        ("center", { let mut f = TextEdit::from_parts("", vec![], vec![]); f.set_paragraph_attrs(vec![d.align(ParagraphAlign::Center)]); f }),
    ];
    let htmls = ["<p style=\"text-align:center\"><img src=\"a.png\"></p>", "<ul><li></li></ul>", "<blockquote></blockquote>", "<p style=\"text-align:center\"></p>"];
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (doc, e) in docs() {
        let n = e.text.len();
        let orig: Vec<u64> = e.paragraph_attrs().iter().map(|a| a.id).collect();
        for a in 0..=n { for b in a..=n {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            let line = e.line_of_byte(a);
            let mut plain = e.paragraph(line);
            plain.id = 0;
            // the single-line paste into an empty plain line takes the line's attributes
            let takes = a == b && e.line_range(line).is_empty() && plain == ParagraphAttrs::default() && e.blocks()[line] == Block::Paragraph;
            let mut want = e.clone();
            want.set_selection(a, b);
            if a < b { want.insert(""); }
            let mut paste = |name: String, f: &dyn Fn(&mut TextEdit)| {
                cases += 1;
                let mut x = e.clone();
                x.set_selection(a, b);
                let before = seen(&x);
                f(&mut x);
                let ch = x.take_changes();
                if let Err(w) = check(&before, &x, &ch) { fails.push(format!("{doc} {a}..{b} {name}: report {w}")); }
                if !takes && (rows(&x, &orig), x.soft_breaks().to_vec()) != (rows(&want, &orig), want.soft_breaks().to_vec()) {
                    fails.push(format!("{doc} {a}..{b} {:?} {name}\n   now  {} soft {:?}\n   want {} soft {:?}", &e.text[a..b], show(&rows(&x, &orig)), x.soft_breaks(), show(&rows(&want, &orig)), want.soft_breaks()));
                }
            };
            for (fname, f) in &frags { paste(format!("fragment {fname}"), &|x| { x.paste_fragment(f.clone(), ""); }); }
            for h in htmls { paste(format!("html {h}"), &|x| { x.paste_html(h, ""); }); }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
}

/// A copy that begins at a line's end starts with a plain empty paragraph, never a list item, quote or
/// code block; one that ends behind a break ends in one, in code too.
#[test]
fn a_copy_that_begins_or_ends_at_a_break_says_so_in_plain_paragraphs() {
    let (p, b, q, c) = (Block::Paragraph, Block::Bullet(0), Block::Quote, Block::Code);
    for blocks in [vec![b, b, p], vec![q, q, p], vec![c, c, c], vec![p, c, c], vec![c, p, p]] {
        let mut e = TextEdit::from_parts("one\ntwo\nthr", vec![], blocks.clone());
        e.take_changes();
        e.set_selection(3, 7);
        let html = e.selected_html();
        assert!(html.starts_with("<p></p>"), "{blocks:?} {:?}: {html}", &e.text[3..7]);
        let f = e.html_fragment(&html);
        assert_eq!((f.text.as_str(), f.blocks()[0]), ("\ntwo", p), "{blocks:?}: {html}");
        e.set_selection(0, 4);
        let html = e.selected_html();
        let f = e.html_fragment(&html);
        assert_eq!(f.text, "one\n", "{blocks:?}: {html}");
    }
}

/// A formatted copy that ends behind a break, pasted into a single-line rich field, joins as the
/// lines do, without a trailing space.
#[test]
fn a_formatted_copy_ending_behind_a_break_pastes_into_a_single_line_field_without_a_trailing_space() {
    let mut src = TextEdit::from_parts("one\ntwo\nthr", vec![mr(0..3, Mark::Bold)], vec![Block::Paragraph; 3]);
    src.take_changes();
    src.set_selection(0, 8);
    let html = src.selected_html();
    let mut dst = TextEdit::new("xyzw").rich();
    dst.set_single_line(true);
    dst.set_caret(2);
    dst.paste_html(&html, "one\ntwo\n");
    assert_eq!(dst.text, "xyone twozw", "{html}");
}

/// Undo and redo of a paste are exact in every kind of field, and for the fragments with no text.
#[test]
fn undo_and_redo_of_a_paste_are_exact_in_every_kind_of_field() {
    let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), y.marks().to_vec(), y.soft_breaks().to_vec());
    let mut all: Vec<(String, TextEdit)> = docs().into_iter().map(|(n, e)| (n.to_string(), e)).collect();
    all.extend(plain_docs());
    let mut frags: Vec<TextEdit> = fragments().iter().map(mkfrag).collect();
    frags.push({ let mut f = TextEdit::from_parts("", vec![], vec![]); f.set_paragraph_attrs(vec![ParagraphAttrs::default().id(60)]); f });
    frags.push(TextEdit::from_parts("", vec![], vec![Block::Bullet(0)]));
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (doc, e) in all {
        let n = e.text.len();
        for a in 0..=n { for b in a..=n {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            for (fi, f) in frags.iter().enumerate() {
                cases += 1;
                let mut y = e.clone();
                y.set_history(true);
                y.set_selection(a, b);
                let st = snap(&y);
                y.paste_fragment(f.clone(), "");
                let done = snap(&y);
                y.undo();
                if snap(&y) != st { fails.push(format!("{doc} {a}..{b} frag#{fi}: undo")); }
                y.redo();
                if snap(&y) != done { fails.push(format!("{doc} {a}..{b} frag#{fi}: redo")); }
            }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
}

fn composer(text: &str, blocks: Vec<Block>) -> TextEdit {
    let mut e = TextEdit::from_parts(text, vec![], blocks);
    e.take_changes();
    e
}

fn doc(text: &str, blocks: Vec<Block>) -> TextEdit {
    let mut e = TextEdit::from_parts(text, vec![], blocks);
    e.set_paragraph_attrs((0..text.split('\n').count()).map(|i| ParagraphAttrs::default().id(1 + i as u64)).collect());
    e.set_paragraph_ids(100);
    e.take_changes();
    e
}

fn lines_of(e: &TextEdit) -> Vec<(String, Block)> {
    e.text.split('\n').enumerate().map(|(i, t)| (t.to_string(), e.blocks()[i])).collect()
}

/// A paste into a code block is dev's: unformatted HTML is the plain text, with its whitespace, in code
/// lines, whether it lands in an empty field, on an empty code line, or at a line's start or end.
#[test]
fn a_paste_into_a_code_block_is_the_plain_text_in_code_lines() {
    let c = Block::Code;
    let fields: Vec<(&str, TextEdit, usize)> = vec![
        ("fresh", composer("", vec![c]), 0),
        ("empty line in a run", composer("fn a() {\n\n}", vec![c, c, c]), 9),
        ("start of a line in a run", composer("fn a() {\n}", vec![c, c]), 9),
        ("end of a line in a run", composer("fn a() {\n}", vec![c, c]), 8),
        ("doc, empty line in a run", doc("fn a() {\n\n}", vec![c, c, c]), 9),
    ];
    let pastes = [
        ("<div>let  x = 1;</div><div><br></div>", "let  x = 1;\n"),
        ("<p>let  x = 1;</p><p></p>", "let  x = 1;\n"),
        ("<p></p><p>let  x = 1;</p>", "\nlet  x = 1;"),
        ("<p>let  x = 1;</p>", "let  x = 1;"),
    ];
    for (name, f, at) in &fields {
        for (html, plain) in pastes {
            let (mut x, mut y) = (f.clone(), f.clone());
            x.set_selection(*at, *at);
            y.set_selection(*at, *at);
            x.paste_html(html, plain);
            y.paste(plain);
            assert_eq!((lines_of(&x), x.range()), (lines_of(&y), y.range()), "{name}: {html}");
            assert!(lines_of(&x).iter().all(|l| l.1 == c), "{name}: {html}: {:?}", lines_of(&x));
        }
    }
    // the same text over a selection of code is no change
    let mut e = doc("ab\n\ncd\nef", vec![c, c, Block::Quote, Block::Paragraph]);
    e.set_selection(2, 4);
    e.paste_html("<p></p><p></p><p></p>", "\n\n");
    assert_eq!(lines_of(&e), vec![("ab".into(), c), ("".into(), c), ("cd".into(), Block::Quote), ("ef".into(), Block::Paragraph)]);
}

/// Position decides: a line in a code block is code, plain, and one that ends up in a paragraph keeps the
/// fragment's formatting.
#[test]
fn a_paste_across_code_and_paragraphs_formats_the_lines_that_are_paragraphs() {
    let (p, c) = (Block::Paragraph, Block::Code);
    for (a, b, want) in [
        (2, 7, vec![("abX", p), ("Yde", c), ("end", p)]),
        (2, 5, vec![("abX", p), ("Ycode", c), ("end", p)]),
        (7, 11, vec![("abcd", p), ("coX", c), ("Ynd", p)]),
    ] {
        let mut e = doc("abcd\ncode\nend", vec![p, c, p]);
        e.set_selection(a, b);
        e.paste_html("<p><b>X</b></p><p><b>Y</b></p>", "X\nY");
        let got: Vec<(String, Block)> = lines_of(&e);
        assert_eq!(got, want.iter().map(|(t, b)| (t.to_string(), *b)).collect::<Vec<_>>(), "{a}..{b}");
        let bold: Vec<char> = e.marks().iter().filter(|m| m.mark == Mark::Bold).flat_map(|m| e.text[m.range()].chars()).collect();
        let want_bold: Vec<char> = match a { 7 => vec!['Y'], _ => vec!['X'] };
        assert_eq!(bold, want_bold, "{a}..{b}: {:?}", e.marks());
    }
}

/// A copy that ends behind a break before an empty code line ends in a paragraph, so a paste keeps the
/// lines apart.
#[test]
fn a_copy_that_ends_before_an_empty_code_line_pastes_its_break() {
    let c = Block::Code;
    for src in [doc("ab\n\ncd", vec![c, c, c]), composer("ab\n\ncd", vec![c, c, c])] {
        let mut s = src.clone();
        s.set_selection(0, 3);
        let html = s.selected_html();
        assert!(html.ends_with("<p></p>"), "{html}");
        let f = s.html_fragment(&html);
        assert_eq!(f.text, "ab\n", "{html}");
        let mut dst = composer("xyz\nend", vec![Block::Paragraph; 2]);
        dst.set_selection(3, 3);
        dst.paste_html(&html, "ab\n");
        assert_eq!(dst.text, "xyzab\n\nend", "{html}");
    }
}

/// An element that says it is an atom but holds nothing is no atom.
#[test]
fn an_empty_atom_element_pastes_as_no_atom() {
    let d = ParagraphAttrs::default();
    for html in ["<p data-atom=\"40\"></p>", "<p data-atom=\"40\"><br></p>", "<p data-atom=\"40\">T</p><p data-atom=\"40\"></p>"] {
        let mut e = TextEdit::from_parts("top\n\nend", vec![], vec![Block::Paragraph; 3]);
        e.set_paragraph_attrs(vec![d.id(1), d.id(2), d.id(3)]);
        e.set_paragraph_ids(100);
        e.set_selection(4, 4);
        let f = e.html_fragment(html);
        for (i, t) in f.text.split('\n').enumerate() {
            assert!(!(t.is_empty() && f.paragraph(i).atom.is_some()), "{html}: empty atom in {:?}", f.text);
        }
        e.paste_fragment(f, "");
        for (i, t) in e.text.split('\n').enumerate() {
            assert!(!(t.is_empty() && e.paragraph(i).atom.is_some()), "{html}: {:?}", e.text);
        }
    }
}

/// A formatted copy that ends in two breaks joins a single-line field as the lines do, with no trailing space.
#[test]
fn a_fragment_ending_in_two_breaks_adds_no_space_to_a_single_line_field() {
    for (html, plain, want) in [
        ("<p><b>one</b></p><p></p><p></p>", "one\n\n", "xyonezw"),
        ("<p><b>one</b></p><p></p>", "one\n", "xyonezw"),
        ("<p><b>one</b></p><p>two</p><p></p>", "one\ntwo\n", "xyone twozw"),
        ("<p></p><p><b>one</b></p>", "\none", "xyonezw"),
    ] {
        let mut s = TextEdit::new("xyzw").rich();
        s.set_single_line(true);
        s.set_selection(2, 2);
        s.paste_html(html, plain);
        assert_eq!(s.text, want, "{html}");
    }
}

/// The plain fallbacks a browser or a word processor puts on the clipboard, as lines and as breaks.
const FALLBACKS: [(&str, &str); 21] = [
    ("<p><b>X</b></p>", "X"),
    ("<p><b>a  b</b></p><p>c  d</p>", "a  b\nc  d"),
    ("<p><b>a  b</b></p><p>c  d</p>", "a  b\n\nc  d"),
    ("<p><b>a  b</b></p><p>c  d</p>", "a  b\n\nc  d\n"),
    ("<p><b>a  b</b></p><p>c  d</p><p></p>", "a  b\nc  d\n"),
    ("<ul><li>a  b</li><li>c</li></ul>", "a  b\nc"),
    ("<pre><code>a  b\nc</code></pre>", "a  b\nc"),
    ("<blockquote>q  q</blockquote>", "q  q"),
    ("<p></p><p><b>x  y</b></p>", "\nx  y"),
    ("<p><b>a</b><br>b  c</p>", "a\nb  c"),
    ("<p><b>a  b</b></p><p>c  d</p>", ""),
    ("<p><i>x  y</i></p>", "x  y\n"),
    ("<p><b>x</b></p><p><b>y</b></p><p><b>z</b></p>", "x\ny\nz"),
    ("<div>let  x = 1;</div><div><br></div>", "let  x = 1;\n"),
    ("<p><b>a  b</b></p><p>c  d</p>", "a  b\r\nc  d"),
    ("<p><b>a  b</b></p><p>c  d</p>", "a  b\r\n\r\nc  d\r\n"),
    ("<p>a  <b>b</b></p><ul><li>c  d</li></ul>", "a  b\nc  d"),
    ("<p><b>a  b</b></p><p></p>", "a  b\n"),
    ("<ul><li></li></ul>", ""),
    ("<p><b>a\tb</b></p><p><b>c</b></p>", "a\tb\nc"),
    ("<p><b>one</b></p><p>two</p>", "one\r\ntwo\r\n"),
];

/// A paste into code, whatever the plain fallback's breaks and lines: no carriage return in the text, no marks on
/// code lines, a report that follows, an exact undo and redo, and where the first line lands in code and there is
/// no atom, dev's: the plain fallback pasted whole when it has other lines than the fragment.
#[test]
fn a_paste_into_code_is_clean_for_every_plain_fallback() {
    let (p, c, q, u) = (Block::Paragraph, Block::Code, Block::Quote, Block::Bullet(0));
    let fixtures: Vec<(&str, Vec<Block>)> = vec![
        ("", vec![c]), ("ab", vec![c]), ("ab\ncd", vec![c, c]), ("ab\n\ncd", vec![c, c, c]), ("\nab\n", vec![c, c, c]),
        ("ab\ncd\nef", vec![p, c, p]), ("ab\ncd\nef", vec![c, p, c]), ("ab\ncd\nef", vec![c, c, p]), ("ab\ncd\nef", vec![p, c, c]),
        ("ab\n\nef", vec![p, c, p]), ("ab\n\nef", vec![c, p, c]), ("ab\ncd\nef", vec![q, c, u]), ("ab\ncd\nef", vec![c, u, c]), ("\n\n", vec![c, p, c]), ("\n\n", vec![p, c, p]),
    ];
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (text, blocks) in &fixtures {
        let mut f = composer(text, blocks.clone());
        f.set_history(false);
        for a in 0..=text.len() { for b in a..=text.len() {
            for (html, plain) in FALLBACKS {
                cases += 1;
                let mut x = f.clone();
                x.set_selection(a, b);
                let before = seen(&x);
                x.paste_html(html, plain);
                let ch = x.take_changes();
                let msg = format!("{text:?}{blocks:?} {a}..{b} {html} {plain:?}: {:?}", lines_of(&x));
                if x.text.contains('\r') { fails.push(format!("{msg}: a carriage return in the text")); }
                let mut off = 0;
                for (i, t) in x.text.split('\n').enumerate() {
                    if x.blocks()[i] == Block::Code && x.marks().iter().any(|m| m.start < off + t.len() && m.end > off) { fails.push(format!("{msg}: a mark on code line {i}")); }
                    off += t.len() + 1;
                }
                if let Err(w) = check(&before, &x, &ch) { fails.push(format!("{msg}: report {w}")); }
                // dev: a fallback with other lines than the fragment, pasted where its first line is code, is pasted whole
                let f_lines = { let h = f.html_fragment(html); h.text.split('\n').count() };
                let normal = plain.replace("\r\n", "\n").replace('\r', "\n");
                if f.blocks()[f.line_of_byte(a)] == c && !normal.is_empty() && normal.split('\n').count() != f_lines {
                    let mut y = f.clone();
                    y.set_selection(a, b);
                    y.paste(plain);
                    if (lines_of(&x), x.range()) != (lines_of(&y), y.range()) { fails.push(format!("{msg}: dev's {:?}", lines_of(&y))); }
                }
                let mut h = f.clone();
                h.set_history(true);
                h.set_selection(a, b);
                h.paste_html(html, plain);
                h.undo();
                if (h.text.clone(), h.blocks().to_vec(), h.marks().to_vec()) != (f.text.clone(), f.blocks().to_vec(), f.marks().to_vec()) { fails.push(format!("{msg}: undo")); }
            }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(10).cloned().collect::<Vec<_>>().join("\n"));
}

/// Atoms among code lines take the plain fallback's text for the code lines, whatever its breaks and blank lines.
#[test]
fn atoms_among_code_lines_take_the_plain_fallbacks_text() {
    let d = ParagraphAttrs::default();
    let atom_frag = |text: &str| { let mut f = TextEdit::from_parts(text, vec![mr(0..1, Mark::Bold)], vec![Block::Paragraph; text.split('\n').count()]); f.set_paragraph_attrs(vec![d, d.atom(20.0).id(50), d]); f };
    for (plain, want) in [
        ("a  a\nQ\nb  b", ["a  a", "Q", "b  bxy"]),
        ("a  a\r\nQ\r\nb  b", ["a  a", "Q", "b  bxy"]),
        ("a  a\n\nQ\n\nb  b\n", ["a  a", "Q", "b  bxy"]),
        ("a  a\r\n\r\nQ\r\nb  b\r\n", ["a  a", "Q", "b  bxy"]),
    ] {
        let mut e = doc("xy", vec![Block::Code]);
        e.set_caret(0);
        e.paste_fragment(atom_frag("a a\nQ\nb b"), plain);
        assert_eq!(e.text.split('\n').collect::<Vec<_>>(), want, "{plain:?}");
        assert!(!e.text.contains('\r') && e.marks().is_empty(), "{plain:?}: {:?} {:?}", e.text, e.marks());
        assert_eq!(e.paragraph(1).atom, Some(20.0), "{plain:?}");
    }
    // a fallback that cannot be matched leaves the fragment's text
    let mut e = doc("xy", vec![Block::Code]);
    e.set_caret(0);
    e.paste_fragment(atom_frag("a a\nQ\nb b"), "something else entirely\nand more\nand more\nand more");
    assert_eq!(e.text.split('\n').collect::<Vec<_>>(), ["a a", "Q", "b bxy"]);
}

/// A fragment's soft break is no soft break of a code line.
#[test]
fn a_fragment_soft_break_in_code_is_dropped() {
    let mut e = composer("xy", vec![Block::Code]);
    e.set_caret(0);
    e.paste_html("<p>one<br>two</p>", "one\ntwo");
    assert!(e.soft_breaks().is_empty(), "{:?} {:?}", e.soft_breaks(), lines_of(&e));
}
