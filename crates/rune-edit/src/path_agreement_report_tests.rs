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
