//! Every path an edit can take (typed text, `insert`, `paste`, `paste_html` with and without a
//! mark, `paste_fragment` with default, attributed and marked fragments, and Return) over every
//! selection of fourteen documents (atoms of every block, pills, links, empty lines) gives one
//! result, and a pasted fragment (atoms of every block, marks, links, pills) follows the rules
//! of the README: atoms are never lost and never take text, ids stay unique and keep their
//! paragraphs, marks land on the fragment's bytes, and undo and redo are exact.

use std::collections::{BTreeMap, BTreeSet};

use super::fuzz_tests::{check, seen};
use super::*;

pub(super) const NEW: u64 = u64::MAX;
pub(super) type Row = (String, u64, u16, ParagraphAlign, (f32, f32), Option<f32>, Block, bool);

pub(super) fn rows(e: &TextEdit, orig: &[u64]) -> Vec<Row> {
    let blocks = e.blocks();
    e.text.split('\n').enumerate().map(|(i, t)| {
        let a = e.paragraph(i);
        let id = if orig.contains(&a.id) || a.id == 0 { a.id } else { NEW };
        (t.to_string(), id, a.style, a.align, (a.indent, a.first_line_indent), a.atom, blocks[i], a.page_break_before)
    }).collect()
}
pub(super) fn show(r: &[Row]) -> String {
    r.iter().map(|r| format!("{:?}#{}{}s{}i{}{:?}{:?}{}", r.0, if r.1 == NEW { "N".into() } else { r.1.to_string() }, if r.5.is_some() { "@" } else { "" }, r.2, r.4.0, r.6, r.3, if r.7 { "PB" } else { "" })).collect::<Vec<_>>().join(" | ")
}
pub(super) fn marked(marks: &[MarkRange], from: usize, to: usize) -> BTreeSet<(usize, String)> {
    marks.iter().flat_map(|m| (m.start.max(from)..m.end.min(to)).map(move |i| (i - from, format!("{:?}", m.mark)))).collect()
}
pub(super) fn mr(r: std::ops::Range<usize>, m: Mark) -> MarkRange { MarkRange::new(r, m) }
pub(super) fn pill(s: &str) -> Mark { Mark::Pill(s.into()) }
pub(super) fn link() -> Mark { Mark::Link("https://example.com".into()) }

pub(super) fn mk(text: &str, marks: Vec<MarkRange>, blocks: Vec<Block>, attrs: Vec<ParagraphAttrs>) -> TextEdit {
    let n = marks.len();
    let mut e = TextEdit::from_parts(text, marks, blocks.clone());
    e.set_paragraph_attrs(attrs.clone());
    e.set_paragraph_ids(100);
    e.set_history(false);
    e.take_changes();
    assert_eq!(e.marks().len(), n, "doc {text:?} lost marks: {:?}", e.marks());
    assert_eq!(e.blocks(), &blocks[..], "doc {text:?}");
    assert_eq!(e.paragraph_attrs(), attrs, "doc {text:?}");
    e
}

pub(super) fn docs() -> Vec<(&'static str, TextEdit)> {
    let d = ParagraphAttrs::default();
    let b = Block::Bullet(0);
    let p = Block::Paragraph;
    let q = Block::Quote;
    let c = Block::Code;
    let n = Block::Numbered(1);
    let mut soft = mk("one\ntwo\nthr", vec![], vec![b, b, p], vec![d.id(1).style(1), d.id(2).style(2), d.id(3).style(3)]);
    soft.set_soft_breaks(vec![1]);
    soft.take_changes();
    let soft_in = |text: &str, bl: Vec<Block>, at: Vec<ParagraphAttrs>, soft: Vec<u32>| { let mut e = mk(text, vec![], bl, at); e.set_soft_breaks(soft); e.take_changes(); e };
    vec![
        ("list", mk("top\nXYZ\nend", vec![], vec![p, b, p], vec![d.id(1), d.id(2).style(2).indent(10.0, 0.0), d.id(3).style(3)])),
        ("listend", mk("top\nXYZ\nend", vec![], vec![p, p, b], vec![d.id(1), d.id(2).style(2).align(ParagraphAlign::Right), d.id(3).style(3).indent(5.0, 2.0)])),
        ("atom", mk("AAA\nTab\nend", vec![], vec![p, p, b], vec![d.id(1), d.id(2).style(2).atom(40.0), d.id(3).style(3)])),
        ("atomfirst", mk("Tab\nXYZ\nend", vec![], vec![p, b, p], vec![d.id(1).style(2).atom(40.0), d.id(2).style(4), d.id(3).style(3)])),
        ("atomlast", mk("AAA\nXYZ\nTab", vec![], vec![b, p, p], vec![d.id(1).style(5), d.id(2).style(4), d.id(3).style(2).atom(40.0)])),
        ("twoatoms", mk("AAA\nTa\nTb\nend", vec![], vec![p, p, p, p], vec![d.id(1), d.id(2).atom(30.0), d.id(3).atom(30.0), d.id(4).style(3)])),
        ("emptyid", mk("top\n\nend", vec![], vec![p, p, p], vec![d.id(1), d.id(2), d.id(3).style(3)])),
        ("emptystyled", mk("top\n\nend", vec![], vec![p, b, p], vec![d.id(1), d.id(2).style(2), d.id(3).style(3)])),
        // "ab@pp cd" 0..8, "XYZ" 9..12, "end" 13..16
        ("pills", mk("ab@pp cd\nXYZ\nend", vec![mr(0..2, Mark::Italic), mr(2..5, pill("f1")), mr(9..11, Mark::Bold), mr(13..16, link())], vec![p, b, p], vec![d.id(1), d.id(2).style(2), d.id(3).style(3)])),
        ("quoteatom", mk("AAA\nTab\nend", vec![mr(0..3, Mark::Bold)], vec![b, q, p], vec![d.id(1), d.id(2).style(2).atom(40.0), d.id(3).style(3)])),
        ("atomonly", mk("Tab", vec![], vec![q], vec![d.id(1).style(2).atom(40.0)])),
        ("atomempty", mk("\nTab\n", vec![], vec![p, q, b], vec![d.id(1), d.id(2).atom(40.0), d.id(3).style(3)])),
        // "@pp" 0..3, "Tab" 4..7, "@qq" 8..11
        ("pillatom", mk("@pp\nTab\n@qq", vec![mr(0..3, pill("f1")), mr(8..11, pill("f2"))], vec![p, p, q], vec![d.id(1), d.id(2).atom(40.0), d.id(3).style(3)])),
        ("quoteatoms", mk("Ta\nTb\nxy", vec![], vec![q, b, q], vec![d.id(1).atom(30.0), d.id(2).atom(30.0).style(4), d.id(3).style(3)])),
        ("code", mk("top\nab\ncd\nend", vec![], vec![p, c, c, b], vec![d.id(1), d.id(2).style(2), d.id(3).style(3), d.id(4).style(4)])),
        ("codeatom", mk("ab\nTab\ncd", vec![], vec![c, c, c], vec![d.id(1), d.id(2).atom(40.0), d.id(3).style(3)])),
        ("codepatom", mk("ab\nTab\ncd", vec![], vec![c, p, c], vec![d.id(1).style(1), d.id(2).atom(40.0), d.id(3).style(3)])),
        ("uni", mk("äé\n日本\nend", vec![mr(0..2, Mark::Bold)], vec![p, n, q], vec![d.id(1), d.id(2).style(2).align(ParagraphAlign::Center), d.id(3).style(3)])),
        ("empties", mk("\n\n\n", vec![], vec![p, b, q, p], vec![d.id(1).style(1), d.id(2).style(2), d.id(3).style(3), d.id(4).style(4)])),
        ("soft", soft),
        ("pagebreak", mk("top\nXYZ\nend", vec![], vec![p, n, p], vec![d.id(1).page_break_before(true), d.id(2).style(2).page_break_before(true).indent(4.0, 1.0), d.id(3).style(3).page_break_before(true)])),
        ("atomgap", mk("Ta\n\nTb", vec![], vec![b, p, q], vec![d.id(1).atom(30.0).style(1), d.id(2).style(2), d.id(3).atom(30.0)])),
        ("trail", mk("abc\n", vec![mr(0..3, link())], vec![n, p], vec![d.id(1).style(1), d.id(2)])),
        ("atomtrail", mk("Tab\n", vec![], vec![p, b], vec![d.id(1).atom(30.0).page_break_before(true), d.id(2).style(2)])),
        ("threeatoms", mk("Ta\nTb\nTc", vec![], vec![p, b, c], vec![d.id(1).atom(30.0), d.id(2).atom(30.0).style(2), d.id(3).atom(30.0)])),
        ("pcode", mk("ab\ncd\nef", vec![mr(0..2, Mark::Bold)], vec![p, c, p], vec![d.id(1).style(1), d.id(2).style(2), d.id(3).style(3)])),
        ("codep", mk("ab\ncd\nef", vec![mr(3..5, Mark::Bold)], vec![c, p, c], vec![d.id(1).style(1), d.id(2).style(2), d.id(3).style(3)])),
        ("codeq", mk("ab\n\ncd\nef", vec![], vec![c, c, q, b], vec![d.id(1), d.id(2).style(2), d.id(3).style(3), d.id(4)])),
        ("atomcodeatom", mk("Ta\nab\nTb\ncd", vec![], vec![c, c, n, c], vec![d.id(1).atom(30.0), d.id(2).style(2), d.id(3).atom(30.0), d.id(4)])),
        ("soft-empty-last", soft_in("top\n", vec![p, p], vec![d.id(1), d.id(2)], vec![1])),
        ("soft-empty-mid", soft_in("top\n\nend", vec![p, p, p], vec![d.id(1), d.id(2), d.id(3)], vec![1])),
        ("soft-head-empty", soft_in("top\n\nend", vec![p, p, p], vec![d.id(1), d.id(2), d.id(3)], vec![2])),
        ("soft-text", soft_in("one\ntwo\nthr", vec![b, b, p], vec![d.id(1).style(1), d.id(2).style(1), d.id(3).style(3)], vec![1])),
        ("soft-quote", soft_in("one\n\nthr", vec![q, q, q], vec![d.id(1), d.id(2), d.id(3)], vec![1, 2])),
        ("soft-after-atom", soft_in("Tab\none\ntwo", vec![p, p, p], vec![d.id(1).atom(30.0), d.id(2), d.id(3)], vec![2])),
        ("soft-before-atom", soft_in("one\ntwo\nTab\nend", vec![p, p, p, p], vec![d.id(1), d.id(2), d.id(3).atom(30.0), d.id(4)], vec![1])),
        ("soft-around-atom", soft_in("one\nTab\nx\ntwo", vec![p, p, p, p], vec![d.id(1), d.id(2).atom(30.0), d.id(3), d.id(4)], vec![3])),
    ]
}

pub(super) type Frag = (&'static str, Vec<Block>, Vec<ParagraphAttrs>, Vec<MarkRange>);
pub(super) fn fragments() -> Vec<Frag> {
    let d = ParagraphAttrs::default();
    let qa = || d.id(50).atom(20.0);
    let p = Block::Paragraph;
    let q = Block::Quote;
    let c = ParagraphAlign::Center;
    vec![
        ("Q", vec![p], vec![qa()], vec![]),
        ("Q", vec![q], vec![qa().style(9)], vec![]),
        ("A\nQ", vec![p, p], vec![d.id(51).style(7), qa()], vec![]),
        ("Q\nA", vec![q, p], vec![qa(), d.id(51).style(7)], vec![]),
        ("Q\n", vec![p, p], vec![qa(), d.id(51)], vec![]),
        ("Q\nfg", vec![p, p], vec![qa(), d.id(51)], vec![mr(2..4, link())]),
        ("A\nQ\nB", vec![p, p, p], vec![d.id(51), qa(), d.id(52)], vec![mr(0..1, Mark::Bold)]),
        ("A\nQ\nB", vec![Block::Bullet(0), q, Block::Numbered(0)], vec![d.id(51).style(7), qa(), d.id(52).style(8).align(ParagraphAlign::Right)], vec![mr(0..1, Mark::Bold), mr(4..5, link())]),
        ("A\nQ\nQ\nB", vec![p, p, q, p], vec![d.id(51), qa(), d.id(53).atom(20.0), d.id(52)], vec![]),
        ("Q\nQ", vec![p, q], vec![qa(), d.id(53).atom(20.0)], vec![]),
        ("x@p\nQ", vec![p, p], vec![d.id(51), qa()], vec![mr(1..3, pill("p1")), mr(0..1, Mark::Italic)]),
        ("x@p\nQ\nfg", vec![p, q, p], vec![d.id(51), qa(), d.id(52)], vec![mr(0..1, Mark::Italic), mr(1..3, pill("p1")), mr(6..8, link())]),
        ("Q\n@p", vec![q, p], vec![qa(), d.id(51)], vec![mr(2..4, pill("p1"))]),
        ("@p\nQ", vec![p, q], vec![d.id(51), qa()], vec![mr(0..2, pill("p1"))]),
        ("@p", vec![p], vec![d.id(51)], vec![mr(0..2, pill("p1"))]),
        ("fg", vec![p], vec![d.id(51)], vec![mr(0..2, link())]),
        ("f\ng", vec![p, p], vec![d.id(51), d.id(52)], vec![mr(0..1, Mark::Bold), mr(2..3, link())]),
        ("fg\n", vec![p, p], vec![d.id(51), d.id(52)], vec![mr(0..2, link())]),
        ("A\nB\n", vec![p, p, p], vec![d.id(51).style(7).align(c), d.id(52).style(8), d.id(53)], vec![]),
        ("A\nB", vec![p, q], vec![d.id(51).style(7).align(c), d.id(52).style(8)], vec![mr(0..1, Mark::Bold)]),
        ("A\nM\nB", vec![p, Block::Bullet(1), q], vec![d.id(51).style(7).align(c), d.id(52).style(8).align(c), d.id(53)], vec![mr(2..3, Mark::Bold)]),
        ("A", vec![p], vec![d.id(51).style(7).align(c)], vec![]),
        ("Q", vec![p], vec![qa()], vec![]),
        ("A\nQ\nB", vec![Block::Bullet(0), q, Block::Numbered(0)], vec![d.id(51).style(7), qa(), d.id(52).style(8).align(ParagraphAlign::Right)], vec![mr(0..1, Mark::Bold)]),
        ("Q\nQ", vec![p, q], vec![qa(), d.id(53).atom(20.0)], vec![]),
        ("A\nB\n", vec![q, Block::Bullet(1), p], vec![d.id(51).style(7).align(c), d.id(52).style(8), d.id(53)], vec![]),
        ("A\nB", vec![Block::Numbered(0), q], vec![d.id(51).style(7).align(c), d.id(52).style(8).align(c)], vec![mr(0..1, Mark::Bold)]),
        ("A\nM\nB", vec![p, Block::Bullet(1), q], vec![d.id(51).style(7).align(c), d.id(52).style(8).align(c), d.id(53).style(9)], vec![mr(2..3, Mark::Bold)]),
        ("A", vec![q], vec![d.id(51).style(7).align(c)], vec![]),
        ("\nA", vec![q, Block::Bullet(0)], vec![d.id(51).style(7), d.id(52).style(8)], vec![]),
        ("A\n\nB", vec![q, Block::Bullet(0), Block::Code], vec![d.id(51).style(7), d.id(52).style(8), d.id(53).style(9)], vec![]),
        ("é\nQ\n", vec![q, p, Block::Bullet(0)], vec![d.id(51).style(7).indent(3.0, 1.0).page_break_before(true), qa(), d.id(52).style(8)], vec![]),
    ]
}

pub(super) fn mkfrag(f: &Frag) -> TextEdit {
    let mut x = TextEdit::from_parts(f.0, f.3.clone(), f.1.clone());
    x.set_paragraph_attrs(f.2.clone());
    assert_eq!(x.marks().len(), f.3.len(), "frag {:?} marks {:?}", f.0, x.marks());
    assert_eq!(x.blocks(), &f.1[..]);
    x
}

fn la_line(e: &TextEdit, a: usize) -> usize { e.line_of_byte(a) }

/// Whether every line of text `ft` pasted over `a..b` lands on a line of code: the first joins the paragraph
/// before it (or goes before the line it is pasted at), the last joins the one after it, and a line the paste
/// makes is code where the line it splits from is. Position decides, and code wins.
pub(super) fn joins_code_obs(e: &TextEdit, a: usize, b: usize, ft: &str) -> bool {
    let ol = line_ranges(&e.text);
    let (la, lb) = (e.line_of_byte(a), e.line_of_byte(b));
    let code = |l: usize| e.blocks()[l] == Block::Code && e.paragraph(l).atom.is_none();
    let nl = ft.matches('\n').count();
    let (kept_head, kept_tail) = (e.paragraph(la).atom.is_some() && a == ol[la].1, e.paragraph(lb).atom.is_some() && b == ol[lb].0);
    let whole = la < lb && a == ol[la].0 && (b == ol[lb].0 || (b == e.text.len() && b == ol[lb].1) || e.paragraph(la).atom.is_some());
    // (An atom left whole after the paste keeps its own line: the line before it stays the first.)
    let kept = if whole && !kept_tail { lb } else { la };
    let (front, tail) = (a == ol[la].0 && nl > 0 && !kept_head && !kept_tail, la < lb && a > ol[la].0 && nl > 0);
    let behind_atom = la < lb && e.paragraph(la).atom.is_some() && a == ol[la].1;
    let last_line = if tail || behind_atom { lb } else { kept };
    let rest = (nl == 0 || front || tail || b < ol[lb].1) && !(kept_tail && nl > 0);
    (0..=nl).all(|k| {
        let joined = if k == 0 { !front } else if k == nl { rest } else { false };
        if !joined { code(kept) } else if k == nl && nl > 0 { code(last_line) } else { code(kept) }
    })
}

/// Whether the line the lines a paste makes split from is code: they are code lines then, as typed text's are.
pub(super) fn makes_code_lines(e: &TextEdit, a: usize, b: usize, ft: &str) -> bool {
    let ol = line_ranges(&e.text);
    let (la, lb) = (e.line_of_byte(a), e.line_of_byte(b));
    let kept_tail = e.paragraph(lb).atom.is_some() && b == ol[lb].0 && ft.contains('\n');
    let whole = la < lb && a == ol[la].0 && (b == ol[lb].0 || (b == e.text.len() && b == ol[lb].1) || e.paragraph(la).atom.is_some());
    let kept = if whole && !kept_tail { lb } else { la };
    e.blocks()[kept] == Block::Code
}

/// An atom is never a soft line and never has one.
pub(super) fn soft_ok(e: &TextEdit) -> bool {
    e.soft_breaks().iter().all(|&l| { let l = l as usize; e.paragraph(l).atom.is_none() && (l == 0 || e.paragraph(l - 1).atom.is_none()) })
}

pub(super) fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut at = 0;
    text.split('\n').map(|l| { let r = (at, at + l.len()); at = r.1 + 1; r }).collect()
}

#[test]
fn a_pasted_fragment_follows_the_rules_over_every_selection() {
    let mut fails: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut cases = 0usize;
    let frags: Vec<Frag> = fragments();
    for (doc, e) in docs() {
        let n = e.text.len();
        let olines = line_ranges(&e.text);
        let oattrs = e.paragraph_attrs();
        let oblocks = e.blocks().to_vec();
        let orig: Vec<u64> = oattrs.iter().map(|a| a.id).collect();
        for a0 in 0..=n { for b0 in a0..=n {
            let mut probe = e.clone();
            probe.set_selection(a0, b0);
            let (a, b) = probe.range();
            if (a, b) != (a0, b0) { continue; }
            for (fi, fr) in frags.iter().enumerate() {
                cases += 1;
                let f = mkfrag(fr);
                let (ft, fb, fa, _) = fr;
                let in_code = joins_code_obs(&e, a, b, ft);
                let fmarks = f.marks().to_vec();
                let mut x = e.clone();
                x.set_selection(a, b);
                let before = seen(&x);
                x.paste_fragment(f.clone(), "");
                let ch = x.take_changes();
                let now = rows(&x, &orig);
                let why = |m: String| format!("{doc} {a}..{b} {:?} frag#{fi} {ft:?}: {m}\n      => {}", &e.text[a..b], show(&now));
                let mut fail = |k: &'static str, m: String| fails.entry(k).or_default().push(why(m));
                if let Err(w) = check(&before, &x, &ch) { fail("report", w); }
                if !soft_ok(&x) { fail("soft-atom", format!("{:?}", x.soft_breaks())); }
                // text shape
                if !(x.text.starts_with(&e.text[..a]) && x.text.ends_with(&e.text[b..]) && x.text.len() >= a + (n - b)) { fail("text-shape", "edges moved".into()); continue; }
                let mid = &x.text[a..x.text.len() - (n - b)];
                let off = mid.len() - mid.trim_start_matches('\n').len();
                let off = if ft.starts_with('\n') { off.saturating_sub(1) } else { off };
                if !mid[off..].starts_with(ft) || !mid[off + ft.len()..].chars().all(|c| c == '\n') || off > 1 || mid.len() - off - ft.len() > 1 { fail("text-shape", format!("mid {mid:?}")); continue; }
                // marks land on exactly the fragment's bytes
                // (A code line holds no marks, wherever the text of the paste ends up.)
                let plain_line = |i: &usize| x.blocks()[x.line_of_byte(a + *i)] != Block::Code;
                // (Text that joins code goes in as plain text: its marks are what typed text gets.)
                let mut t = e.clone();
                t.set_selection(a, b);
                t.insert(ft);
                let want: BTreeSet<(usize, String)> = if in_code && fa.iter().any(|y| y.atom.is_some()) { BTreeSet::new() } else if in_code { marked(t.marks(), a, a + mid.len()) } else { fmarks.iter().flat_map(|m| (m.start..m.end).map(move |i| (off + i, format!("{:?}", m.mark)))).filter(|(i, _)| plain_line(i)).collect() };
                let got: BTreeSet<(usize, String)> = marked(x.marks(), a, a + mid.len()).into_iter().filter(|(i, _)| in_code || plain_line(i)).collect();
                if want != got { fail("marks", format!("want {want:?} got {got:?}")); }
                // pill ranges exact (not merged/split)
                for m in fmarks.iter().filter(|m| m.mark.is_pill() && !in_code && x.blocks()[x.line_of_byte(a + off + m.start)] != Block::Code) {
                    if !x.marks().iter().any(|g| g.mark == m.mark && g.start == a + off + m.start && g.end == a + off + m.end) { fail("pill-range", format!("{:?}", x.marks())); }
                }
                // field pills untouched by the selection survive whole
                for m in e.marks().iter().filter(|m| m.mark.is_pill()) {
                    let hit = (a < b && a < m.end && b > m.start) || (a == b && a > m.start && a < m.end);
                    let st = if m.start >= b { m.start - b + a + mid.len() } else { m.start };
                    let there = x.marks().iter().any(|g| g.mark == m.mark && g.start == st && g.end - g.start == m.end - m.start && &x.text[g.start..g.end] == &e.text[m.start..m.end]);
                    if !hit && !there { fail("field-pill", format!("{:?} gone: {:?}", m, x.marks())); }
                    if hit && x.marks().iter().any(|g| g.mark == m.mark) { fail("field-pill-remains", format!("{:?}", x.marks())); }
                }
                // ids
                let ids: Vec<u64> = x.paragraph_attrs().iter().map(|a| a.id).collect();
                let mut u = ids.clone(); u.sort(); u.dedup();
                if u.len() != ids.len() || ids.contains(&0) { fail("ids", format!("{ids:?}")); }
                let xl = line_ranges(&x.text);
                let lines: Vec<(&str, ParagraphAttrs, Block)> = x.text.split('\n').enumerate().map(|(i, t)| (t, x.paragraph(i), x.blocks()[i])).collect();
                // fragment lines
                let l0 = xl.iter().position(|r| r.0 <= a + off && a + off <= r.1).unwrap();
                let flines: Vec<&str> = ft.split('\n').collect();
                let last = flines.len() - 1;
                let (la, lb) = (olines.iter().position(|r| r.0 <= a && a <= r.1).unwrap(), olines.iter().position(|r| r.0 <= b && b <= r.1).unwrap());
                let (ra, rb) = (olines[la], olines[lb]);
                let mut wo = oattrs[if la < lb && a == ra.0 && (b == rb.0 || (b == n && b == rb.1) || oattrs[la].atom.is_some()) { lb } else { la }]; wo.id = 0;
                let recv_block = oblocks[if la < lb && a == ra.0 && (b == rb.0 || (b == n && b == rb.1) || oattrs[la].atom.is_some()) { lb } else { la }];
                let plain_single = flines.len() == 1 && a == ra.0 && b == rb.1 && wo == ParagraphAttrs::default() && recv_block == Block::Paragraph;
                for k in 0..flines.len() {
                    let (t, at, bl) = lines[l0 + k];
                    let from = fa[k];
                    if from.atom.is_some() {
                        if t != flines[k] || at.atom != Some(20.0) { fail("ATOM-LOST", format!("frag line {k} is {t:?} atom {:?}", at.atom)); continue; }
                        if at.id < 100 { fail("atom-id", format!("pasted atom id {}", at.id)); }
                        if bl != fb[k] { fail(if off == 1 { "atom-block-lead" } else { "atom-block" }, format!("pasted atom block {bl:?} want {:?}", fb[k])); }
                        if (at.style, at.align) != (from.style, from.align) { fail("atom-style", format!("pasted atom style {} want {}", at.style, from.style)); }
                        if (at.indent, at.first_line_indent) != (0.0, 0.0) { fail("atom-indent", format!("{at:?}")); }
                    } else if !(k == last && flines[k].is_empty()) {
                        if at.atom.is_some() && !flines[k].is_empty() { fail("text-is-atom", format!("frag line {k} {t:?} is an atom")); }
                        if k > 0 && k < last && !in_code && !makes_code_lines(&e, a, b, ft) {
                            if t != flines[k] { fail("mid-line-text", format!("{t:?}")); }
                            if at.id < 100 || (at.style, at.align) != (from.style, from.align) || bl != fb[k] || at.indent != 0.0 || at.first_line_indent != 0.0 { fail("new-para", format!("frag line {k}: {at:?} {bl:?}")); }
                        }
                    }
                }
                // atoms: survivors and counts
                let survivors: Vec<usize> = (0..olines.len()).filter(|&i| oattrs[i].atom.is_some() && !(a < b && a < olines[i].1 && b > olines[i].0)).collect();
                let fatoms = fa.iter().filter(|a| a.atom.is_some()).count();
                let nowatoms = lines.iter().filter(|l| l.1.atom.is_some()).count();
                if nowatoms != survivors.len() + fatoms { fail("ATOM-COUNT", format!("{nowatoms} atoms want {}+{fatoms}", survivors.len())); }
                for &i in &survivors {
                    let label = &e.text[olines[i].0..olines[i].1];
                    if !lines.iter().any(|l| l.0 == label && l.1 == oattrs[i] && l.2 == oblocks[i]) { fail("survivor", format!("atom {} changed", oattrs[i].id)); }
                }
                for (i, l) in lines.iter().enumerate() {
                    if l.1.atom.is_none() && l.1.id != 0 && oattrs.iter().any(|o| o.atom.is_some() && o.id == l.1.id) { fail("atom-id-on-text", format!("line {i}")); }
                    if l.1.atom.is_some() && l.1.atom != Some(20.0) && !oattrs.iter().any(|o| o.id == l.1.id && o.atom.is_some()) { fail("field-atom-id", format!("line {i}")); }
                }
                // Rules, independent of the engine.
                let added = mid.matches('\n').count();
                let atom_touch = oattrs[la].atom.is_some() || oattrs[lb].atom.is_some();
                let end_line = la + added;
                let takes_block = a == ra.0 && b == rb.1 && recv_block == Block::Paragraph && flines.len() > 1 && !ft.ends_with('\n');
                let blk = |orig: Block, got: Block| if takes_block { got == orig || got == fb[last] } else { got == orig };
                if a == b && a == ra.0 && !plain_single {
                    // line start: the receiving line keeps id, attrs, block, atom
                    let (t, at, bl) = lines[end_line];
                    if at != oattrs[la] || !blk(oblocks[la], bl) || !t.ends_with(&e.text[ra.0..ra.1]) { fail("rule-line-start", format!("line {la} was {:?} {:?}", oattrs[la], oblocks[la])); }
                }
                if a > ra.0 && added == 0 && !atom_touch {
                    let (_, at, bl) = lines[la];
                    if at != oattrs[la] || bl != oblocks[la] { fail("rule-join", format!("line {la}")); }
                }
                if la < lb && a == ra.0 && (b == rb.0 || (b == n && b == rb.1 && oattrs[lb].atom.is_none())) && !plain_single {
                    let (t, at, bl) = lines[end_line];
                    // (A paragraph taken to the end of the text stays as the hard line its soft lines belong to; a soft line
                    // the edit starts on keeps the block of the paragraph above.)
                    let own = if b == n && b != rb.0 { (la..=lb).rev().find(|&l| !e.soft_breaks().contains(&(l as u32))).unwrap_or(la) } else { lb };
                    let want_block = if added == 0 && e.soft_breaks().contains(&(la as u32)) { oblocks[la] } else { oblocks[own] };
                    if at != oattrs[own] || !blk(want_block, bl) || (b == rb.0 && !t.ends_with(&e.text[rb.0..rb.1])) { fail("rule-whole", format!("line {lb} was {:?} {:?}", oattrs[own], oblocks[own])); }
                }
                if la < lb && a > ra.0 && b > rb.0 && added > 0 && !atom_touch {
                    let (_, at, bl) = lines[la];
                    if at != oattrs[la] || bl != oblocks[la] { fail("rule-midmid-first", String::new()); }
                    let (_, at, bl) = lines[end_line];
                    if at != oattrs[lb] || bl != oblocks[lb] { fail("rule-midmid-last", format!("want {:?} {:?}", oattrs[lb], oblocks[lb])); }
                }
                if la == lb && a > ra.0 && b < ra.1 && added > 0 && !atom_touch {
                    let (_, at, bl) = lines[la];
                    if at != oattrs[la] || bl != oblocks[la] { fail("rule-split-first", String::new()); }
                    let (_, at, bl) = lines[end_line];
                    let mut want = oattrs[la]; want.id = at.id; want.page_break_before = false;
                    if bl != oblocks[la] || at.id < 100 || at != want { fail("rule-split-tail", format!("tail {at:?} {bl:?}")); }
                }
                // typed text of the same bytes: same rows outside the fragment's new paragraphs
                let mut t = e.clone();
                t.set_selection(a, b);
                t.insert(mid);
                let tr = rows(&t, &orig);
                if tr.len() != now.len() { fail("agree-len", show(&tr)); } else {
                    for i in 0..now.len() {
                        let infrag = i >= l0 && i <= l0 + last;
                        if infrag && now[i].1 == NEW && tr[i].1 == NEW { continue; }
                        // A fragment's atom is an atom of its own where typed text is text.
                        if infrag && now[i].5 == Some(20.0) { continue; }
                        if infrag && plain_single { continue; }
                        if takes_block && i == l0 + last { let mut n2 = now[i].clone(); n2.6 = tr[i].6; if n2 == tr[i] { continue; } }
                        if now[i] != tr[i] { fail("agree", format!("line {i}; typed gives {}", show(&tr))); break; }
                    }
                }
                // undo and redo
                let mut y = e.clone();
                y.set_history(true);
                y.set_selection(a, b);
                let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()));
                let st = snap(&y);
                y.paste_fragment(f.clone(), "");
                let done = snap(&y);
                if done != snap(&x) { fail("history-differs", String::new()); }
                y.undo();
                if snap(&y) != st { fail("undo", format!("{:?}", snap(&y))); }
                y.redo();
                if snap(&y) != done { fail("redo", format!("{:?}", snap(&y))); }
            }
        }}
    }
    assert!(fails.is_empty(), "{}", summary(cases, &fails));
}

pub(super) type Path = (&'static str, fn(&mut TextEdit, &str));
fn html_of(s: &str, bold: bool) -> String {
    s.split('\n').map(|l| if bold && !l.is_empty() { format!("<p><b>{l}</b></p>") } else { format!("<p>{l}</p>") }).collect()
}
fn richfrag(s: &str, mark: bool) -> TextEdit {
    let lines = s.split('\n').count();
    let first = s.split('\n').next().unwrap().len();
    let marks = if mark && first > 0 { vec![mr(0..first, link())] } else { vec![] };
    let mut f = TextEdit::from_parts(s, marks, vec![]);
    f.set_paragraph_attrs((0..lines).map(|i| ParagraphAttrs::default().id(60 + i as u64)).collect());
    f
}
pub(super) const PATHS: [Path; 9] = [
    ("typed", |e, s| { e.key(&KeyEvent { key: Key::Text(s.into()), mods: Mods::default() }, None); }),
    ("insert", |e, s| e.insert(s)),
    ("paste", |e, s| { e.paste(s); }),
    ("html", |e, s| { e.paste_html(&html_of(s, false), s); }),
    ("htmlb", |e, s| { e.paste_html(&html_of(s, true), s); }),
    ("frag", |e, s| { e.paste_fragment(TextEdit::from_parts(s, vec![], vec![]), ""); }),
    ("fragattrs", |e, s| { e.paste_fragment(richfrag(s, false), ""); }),
    ("fragmark", |e, s| { e.paste_fragment(richfrag(s, true), ""); }),
    ("fragplain", |e, s| { e.paste_fragment(richfrag(s, true), s); }),
];

#[test]
fn every_path_agrees_over_every_selection_of_every_document() {
    let inputs = ["x", "é", "\n", "A\n", "A\nB", "A\n\nB", "A\nB\nC\nD\n", "\nA", "\n\n", "\nA\n"];
    let mut fails: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cases = 0;
    let mut all = docs();
    // Paragraph ids off: nothing gets an id, and nothing needs one.
    for (_, mut e) in docs().into_iter().take(4) {
        let attrs: Vec<ParagraphAttrs> = e.paragraph_attrs().into_iter().map(|a| a.id(0)).collect();
        e.set_paragraph_ids(0);
        e.set_paragraph_attrs(attrs);
        e.take_changes();
        all.push(("idsoff", e));
    }
    for (doc, e) in all {
        let n = e.text.len();
        let orig: Vec<u64> = e.paragraph_attrs().iter().map(|a| a.id).collect();
        let idsoff = doc == "idsoff";
        for a0 in 0..=n { for b0 in a0..=n {
            let mut pr = e.clone(); pr.set_selection(a0, b0);
            if pr.range() != (a0, b0) { continue; }
            for s in inputs {
                cases += 1;
                let mut res: Vec<(&str, Vec<Row>, Vec<u32>)> = Vec::new();
                for p in PATHS.iter() {
                    let mut x = e.clone();
                    x.set_selection(a0, b0);
                    let before = seen(&x);
                    (p.1)(&mut x, s);
                    let ch = x.take_changes();
                    let r = rows(&x, &orig);
                    if let Err(w) = check(&before, &x, &ch) { fails.entry(format!("report/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}: {w}")); }
                    if !soft_ok(&x) { fails.entry(format!("soft-atom/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}: {:?}", x.soft_breaks())); }
                    let ids: Vec<u64> = x.paragraph_attrs().iter().map(|a| a.id).collect();
                    let mut u = ids.clone(); u.sort(); u.dedup();
                    if !idsoff && u.len() != ids.len() { fails.entry(format!("dup/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}: {ids:?}")); }
                    if idsoff && ids.iter().any(|&i| i != 0) { fails.entry(format!("idsoff-id/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}: {ids:?}")); }
                    if !idsoff { for (i, a) in x.paragraph_attrs().iter().enumerate() {
                        if a.atom.is_none() && e.paragraph_attrs().iter().any(|o| o.atom.is_some() && o.id == a.id) { fails.entry(format!("atomid/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}: line {i}")); }
                    } }
                    let ol = line_ranges(&e.text);
                    for (i, o) in e.paragraph_attrs().iter().enumerate() {
                        if o.atom.is_none() || (a0 < b0 && a0 < ol[i].1 && b0 > ol[i].0) { continue; }
                        let label = &e.text[ol[i].0..ol[i].1];
                        if !x.text.split('\n').enumerate().any(|(j, t)| t == label && x.paragraph(j) == *o && x.blocks()[j] == e.blocks()[i]) { fails.entry(format!("survivor/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}: {}", show(&r))); }
                    }
                    let mut y = e.clone();
                    y.set_history(true);
                    y.set_selection(a0, b0);
                    let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()), y.soft_breaks().to_vec());
                    let st = snap(&y);
                    (p.1)(&mut y, s);
                    let done = snap(&y);
                    y.undo();
                    if snap(&y) != st { fails.entry(format!("undo/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}")); }
                    y.redo();
                    if snap(&y) != done { fails.entry(format!("redo/{}", p.0)).or_default().push(format!("{doc} {a0}..{b0} {s:?}")); }
                    res.push((p.0, r, x.soft_breaks().to_vec()));
                }
                for (name, r, sb) in &res[1..] {
                    let same = a0 < b0 && &e.text[a0..b0] == s;
                    // A fragment with attributes that holds what is selected is a no-op only when its
                    // attributes are the selection's; otherwise it restyles, as the rules say.
                    if same && name.starts_with("frag") && *name != "frag" { continue; }
                    let key = format!("disagree/{name}{}", if same { "/SAME" } else { "" });
                    if *r != res[0].1 { fails.entry(key).or_default().push(format!("{doc} {a0}..{b0} {:?} {s:?}\n      typed: {}\n      {name}: {}", &e.text[a0..b0], show(&res[0].1), show(r))); }
                    else if *sb != res[0].2 { fails.entry(format!("soft/{name}")).or_default().push(format!("{doc} {a0}..{b0} {:?} {s:?} typed {:?} got {:?}", &e.text[a0..b0], res[0].2, sb)); }
                }
            }
        }}
    }
    let fails: BTreeMap<&str, Vec<String>> = fails.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    assert!(fails.is_empty(), "{}", summary(cases, &fails));
}

#[test]
fn return_gives_the_result_of_a_typed_break_over_every_selection() {
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (doc, e) in docs() {
        let len = e.text.len();
        let orig: Vec<u64> = e.paragraph_attrs().iter().map(|a| a.id).collect();
        let ol = line_ranges(&e.text);
        for a in 0..=len { for b in a..=len {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            // Return in an empty list item, quote or code line ends the block.
            let line = e.line_of_byte(a);
            if a == b && e.line_range(line).is_empty() && e.blocks()[line] != Block::Paragraph { continue; }
            cases += 1;
            let mut r = e.clone();
            r.set_selection(a, b);
            let before = seen(&r);
            r.key(&KeyEvent { key: Key::Enter, mods: Mods::default() }, None);
            let ch = r.take_changes();
            let mut t = e.clone();
            t.set_selection(a, b);
            t.insert("\n");
            let key = |x: &TextEdit| x.text.split('\n').enumerate().map(|(i, s)| (s.to_string(), { let id = x.paragraph(i).id; if id >= 100 { NEW } else { id } }, x.paragraph(i).atom)).collect::<Vec<_>>();
            let (rr, tr) = (rows(&r, &orig), rows(&t, &orig));
            let msg = format!("{doc} {a}..{b} {:?}\n   return: {}\n   break:  {}", &e.text[a..b], show(&rr), show(&tr));
            if let Err(w) = check(&before, &r, &ch) { fails.push(format!("{msg}\n   report {w}")); }
            if !soft_ok(&r) { fails.push(format!("{msg}\n   an atom is a soft line or has one: {:?}", r.soft_breaks())); }
            if key(&r) != key(&t) { fails.push(format!("{msg}\n   differs")); }
            let ids: Vec<u64> = r.paragraph_attrs().iter().map(|a| a.id).collect();
            let mut u = ids.clone();
            u.sort();
            u.dedup();
            if u.len() != ids.len() { fails.push(format!("{msg}\n   duplicate ids")); }
            for (i, o) in e.paragraph_attrs().iter().enumerate() {
                if o.atom.is_none() || (a < b && a < ol[i].1 && b > ol[i].0) { continue; }
                let label = &e.text[ol[i].0..ol[i].1];
                if !r.text.split('\n').enumerate().any(|(j, s)| s == label && r.paragraph(j) == *o && r.blocks()[j] == e.blocks()[i]) { fails.push(format!("{msg}\n   atom {} lost", o.id)); }
            }
            // The new paragraph is a copy of the line the selection starts on: its style, alignment,
            // indents and block, never a mix of two lines'.
            let want = |l: usize| { let o = e.paragraph(l); (o.style, o.align, (o.indent, o.first_line_indent), e.blocks()[l]) };
            // Only one shape is no copy of that line: an atom's line that became text (under a new id, in
            // the atom's attributes).
            let as_text = |x: &Row| (la_line(&e, a)..=e.line_of_byte(b)).any(|l| { let o = e.paragraph(l); o.atom.is_some() && (x.2, x.3, x.4, x.6) == (o.style, o.align, (o.indent, o.first_line_indent), e.blocks()[l]) });
            for (i, x) in rr.iter().enumerate() {
                if x.1 == NEW && x.5.is_none() && (x.2, x.3, x.4, x.6) != want(line) && !as_text(x) { fails.push(format!("{msg}\n   new paragraph {i} is not a copy of line {line}")); }
            }
            // undo and redo
            let mut y = e.clone();
            y.set_history(true);
            y.set_selection(a, b);
            let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()), y.soft_breaks().to_vec());
            let st = snap(&y);
            y.key(&KeyEvent { key: Key::Enter, mods: Mods::default() }, None);
            let done = snap(&y);
            y.undo();
            if snap(&y) != st { fails.push(format!("{msg}\n   undo")); }
            y.redo();
            if snap(&y) != done { fails.push(format!("{msg}\n   redo")); }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn a_pasted_fragment_follows_the_rules_in_code_and_around_every_document() {
    let mut fails: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cases = 0usize;
    let frags: Vec<Frag> = fragments().into_iter().skip(22).collect();
    for (doc, e) in docs() {
        let n = e.text.len();
        let olines = line_ranges(&e.text);
        let oattrs = e.paragraph_attrs();
        let oblocks = e.blocks().to_vec();
        let orig: Vec<u64> = oattrs.iter().map(|a| a.id).collect();
        for a0 in 0..=n { for b0 in a0..=n {
            let mut probe = e.clone();
            probe.set_selection(a0, b0);
            let (a, b) = probe.range();
            if (a, b) != (a0, b0) { continue; }
            let (la, lb) = (olines.iter().position(|r| r.0 <= a && a <= r.1).unwrap(), olines.iter().position(|r| r.0 <= b && b <= r.1).unwrap());
            let landing = if la < lb && a == olines[la].0 && (b == olines[lb].0 || (b == n && b == olines[lb].1) || oattrs[la].atom.is_some()) { lb } else { la };
            let touches_code = (la..=lb).any(|l| oblocks[l] == Block::Code);
            for (fi, fr) in frags.iter().enumerate() {
                cases += 1;
                let f = mkfrag(fr);
                let (ft, fb, fa, _) = fr;
                let in_code = joins_code_obs(&e, a, b, ft);
                let code_landing = oblocks[landing] == Block::Code && oattrs[landing].atom.is_none();
                let fmarks = f.marks().to_vec();
                let mut x = e.clone();
                x.set_selection(a, b);
                let before = seen(&x);
                x.paste_fragment(f.clone(), "");
                let ch = x.take_changes();
                let now = rows(&x, &orig);
                let why = |m: String| format!("{doc} {a}..{b} {:?} frag#{fi} {ft:?}: {m}\n      => {} marks {:?}", &e.text[a..b], show(&now), x.marks());
                let mut fail = |k: &str, m: String| fails.entry(format!("{k}{}{}", if in_code { "/code" } else { "" }, if touches_code && !in_code { "/touchcode" } else { "" })).or_default().push(why(m));
                if let Err(w) = check(&before, &x, &ch) { fail("report", w); }
                if !soft_ok(&x) { fail("soft-atom", format!("{:?}", x.soft_breaks())); }
                if !(x.text.starts_with(&e.text[..a]) && x.text.ends_with(&e.text[b..]) && x.text.len() >= a + (n - b)) { fail("text-shape", "edges moved".into()); continue; }
                let mid = &x.text[a..x.text.len() - (n - b)];
                let off = mid.len() - mid.trim_start_matches('\n').len();
                let off = if ft.starts_with('\n') { off.saturating_sub(1) } else { off };
                if !mid[off..].starts_with(ft) || !mid[off + ft.len()..].chars().all(|c| c == '\n') || off > 1 || mid.len() - off - ft.len() > 1 { fail("text-shape", format!("mid {mid:?}")); continue; }
                // strict: marks on code lines anywhere
                let xl = line_ranges(&x.text);
                for m in x.marks() {
                    for (i, r) in xl.iter().enumerate() {
                        if x.blocks()[i] == Block::Code && x.paragraph(i).atom.is_none() && m.start < r.1 && m.end > r.0 { fail("MARK-ON-CODE-LINE", format!("line {i} {m:?}")); }
                    }
                }
                if !in_code {
                    let plain_line = |i: &usize| x.blocks()[x.line_of_byte(a + *i)] != Block::Code;
                    let want: BTreeSet<(usize, String)> = fmarks.iter().flat_map(|m| (m.start..m.end).map(move |i| (off + i, format!("{:?}", m.mark)))).collect();
                    let got: BTreeSet<(usize, String)> = marked(x.marks(), a, a + mid.len());
                    if want != got {
                        let w2: BTreeSet<_> = want.iter().filter(|(i, _)| plain_line(i)).cloned().collect();
                        let g2: BTreeSet<_> = got.iter().filter(|(i, _)| plain_line(i)).cloned().collect();
                        // (A code line holds no marks, wherever the text of the paste ends up.)
                        if w2 != g2 { fail("marks", format!("want {want:?} got {got:?}")); }
                    }
                } else {
                    // Text that joins code goes in as plain text: its marks are what typed text gets.
                    let mut t = e.clone();
                    t.set_selection(a, b);
                    t.insert(ft);
                    let want = if fa.iter().any(|y| y.atom.is_some()) { BTreeSet::new() } else { marked(t.marks(), a, a + mid.len()) };
                    if marked(x.marks(), a, a + mid.len()) != want { fail("marks-in-code", String::new()); }
                }
                let ids: Vec<u64> = x.paragraph_attrs().iter().map(|a| a.id).collect();
                let mut u = ids.clone(); u.sort(); u.dedup();
                if u.len() != ids.len() || ids.contains(&0) { fail("ids", format!("{ids:?}")); }
                let lines: Vec<(&str, ParagraphAttrs, Block)> = x.text.split('\n').enumerate().map(|(i, t)| (t, x.paragraph(i), x.blocks()[i])).collect();
                let l0 = xl.iter().position(|r| r.0 <= a + off && a + off <= r.1).unwrap();
                let flines: Vec<&str> = ft.split('\n').collect();
                let last = flines.len() - 1;
                let atom_touch = oattrs[la].atom.is_some() || oattrs[lb].atom.is_some();
                for k in 0..flines.len() {
                    let (t, at, bl) = lines[l0 + k];
                    let from = fa[k];
                    if from.atom.is_some() {
                        if t != flines[k] || at.atom != Some(20.0) { fail("ATOM-LOST", format!("frag line {k} is {t:?} atom {:?}", at.atom)); continue; }
                        if at.id < 100 { fail("atom-id", format!("pasted atom id {}", at.id)); }
                        if bl != fb[k] { fail("atom-block", format!("pasted atom block {bl:?} want {:?}", fb[k])); }
                        if (at.style, at.align) != (from.style, from.align) { fail("atom-style", format!("pasted atom style {}", at.style)); }
                        if (at.indent, at.first_line_indent, at.page_break_before) != (0.0, 0.0, false) { fail("atom-indent", format!("{at:?}")); }
                    } else {
                        if at.atom.is_some() && !flines[k].is_empty() { fail("text-is-atom", format!("frag line {k} {t:?} is an atom")); }
                        if at.id >= 100 && t == flines[k] && at.atom.is_none() && !in_code {
                            let code_line = bl == Block::Code;
                            let as_text = |o: ParagraphAttrs| { let mut o = o; o.atom = None; o.id = at.id; o };
                            let kept_atom = atom_touch && (la..=lb).any(|l| oattrs[l].atom.is_some() && (as_text(oattrs[l]) == at || { let mut o = as_text(oattrs[l]); o.page_break_before = false; o == at }));
                            if kept_atom || makes_code_lines(&e, a, b, ft) { continue; }
                            if (at.style, at.align) != (from.style, from.align) || bl != fb[k] || at.indent != 0.0 || at.first_line_indent != 0.0 || at.page_break_before {
                                fail(&format!("new-para-k{}{}{}", if k == 0 { "first" } else if k == last { "last" } else { "mid" }, if atom_touch { "/atomtouch" } else { "" }, if code_line { "/codeline" } else { "" }), format!("frag line {k}: {at:?} {bl:?}"));
                            }
                        }
                        if in_code && code_landing && at.id >= 100 && t == flines[k] && k > 0 && k < last && (bl != Block::Code) { fail("code-mid-block", format!("{bl:?}")); }
                        // (The last line of a paste that runs to a later line is that paragraph, in its block.)
                        if in_code && code_landing && bl != Block::Code && !flines[k].is_empty() && !(k == last && la < lb) { fail("code-text-not-code", format!("frag line {k} {bl:?}")); }
                    }
                }
                let survivors: Vec<usize> = (0..olines.len()).filter(|&i| oattrs[i].atom.is_some() && !(a < b && a < olines[i].1 && b > olines[i].0)).collect();
                let fatoms = fa.iter().filter(|a| a.atom.is_some()).count();
                let nowatoms = lines.iter().filter(|l| l.1.atom.is_some()).count();
                if nowatoms != survivors.len() + fatoms { fail("ATOM-COUNT", format!("{nowatoms} atoms want {}+{fatoms}", survivors.len())); }
                for &i in &survivors {
                    let label = &e.text[olines[i].0..olines[i].1];
                    if !lines.iter().any(|l| l.0 == label && l.1 == oattrs[i] && l.2 == oblocks[i]) { fail("survivor", format!("atom {} changed", oattrs[i].id)); }
                }
                for (i, l) in lines.iter().enumerate() {
                    if l.1.atom.is_none() && l.1.id != 0 && oattrs.iter().any(|o| o.atom.is_some() && o.id == l.1.id) { fail("atom-id-on-text", format!("line {i}")); }
                    if l.1.atom == Some(20.0) && l.0 != "Q" { fail("atom-holds-text", format!("line {i} {:?}", l.0)); }
                }
                // untouched lines outside the selection keep everything
                for i in 0..la { if now[i] != rows(&e, &orig)[i] && !(i + 1 == la && false) { fail("line-before-changed", format!("line {i}")); } }
                let on = rows(&e, &orig);
                for i in lb + 1..on.len() { let j = now.len() - (on.len() - i); if now[j] != on[i] { fail("line-after-changed", format!("line {i}")); } }
                // rules independent of engine
                let (ra, rb) = (olines[la], olines[lb]);
                let added = mid.matches('\n').count();
                let end_line = la + added;
                let mut wo = oattrs[landing]; wo.id = 0;
                let plain_single = flines.len() == 1 && a == ra.0 && b == rb.1 && wo == ParagraphAttrs::default() && oblocks[landing] == Block::Paragraph;
                let takes_block = a == ra.0 && b == rb.1 && oblocks[landing] == Block::Paragraph && flines.len() > 1 && !ft.ends_with('\n');
                let blk = |orig: Block, got: Block| if takes_block { got == orig || got == fb[last] } else { got == orig };
                if a == b && a == ra.0 && !plain_single {
                    let (t, at, bl) = lines[end_line];
                    if at != oattrs[la] || !blk(oblocks[la], bl) || !t.ends_with(&e.text[ra.0..ra.1]) { fail("rule-line-start", format!("line {la} was {:?} {:?}", oattrs[la], oblocks[la])); }
                }
                if a > ra.0 && added == 0 && !atom_touch {
                    let (_, at, bl) = lines[la];
                    if at != oattrs[la] || bl != oblocks[la] { fail("rule-join", format!("line {la}")); }
                }
                if la < lb && a == ra.0 && (b == rb.0 || (b == n && b == rb.1 && oattrs[lb].atom.is_none())) && !plain_single {
                    let (t, at, bl) = lines[end_line];
                    // (A paragraph taken to the end of the text stays as the hard line its soft lines belong to; a soft line
                    // the edit starts on keeps the block of the paragraph above.)
                    let own = if b == n && b != rb.0 { (la..=lb).rev().find(|&l| !e.soft_breaks().contains(&(l as u32))).unwrap_or(la) } else { lb };
                    let want_block = if added == 0 && e.soft_breaks().contains(&(la as u32)) { oblocks[la] } else { oblocks[own] };
                    if at != oattrs[own] || !blk(want_block, bl) || (b == rb.0 && !t.ends_with(&e.text[rb.0..rb.1])) { fail("rule-whole", format!("line {lb} was {:?} {:?}", oattrs[own], oblocks[own])); }
                }
                if la < lb && a > ra.0 && b > rb.0 && added > 0 && !atom_touch {
                    let (_, at, bl) = lines[la];
                    if at != oattrs[la] || bl != oblocks[la] { fail("rule-midmid-first", String::new()); }
                    let (_, at, bl) = lines[end_line];
                    if at != oattrs[lb] || bl != oblocks[lb] { fail("rule-midmid-last", format!("want {:?} {:?}", oattrs[lb], oblocks[lb])); }
                }
                if la == lb && a > ra.0 && b < ra.1 && added > 0 && !atom_touch {
                    let (_, at, bl) = lines[la];
                    if at != oattrs[la] || bl != oblocks[la] { fail("rule-split-first", String::new()); }
                    let (_, at, bl) = lines[end_line];
                    let mut want = oattrs[la]; want.id = at.id; want.page_break_before = false;
                    if bl != oblocks[la] || at.id < 100 || at != want { fail("rule-split-tail", format!("tail {at:?} {bl:?}")); }
                }
                let mut y = e.clone();
                y.set_history(true);
                y.set_selection(a, b);
                let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()), y.soft_breaks().to_vec());
                let st = snap(&y);
                y.paste_fragment(f.clone(), "");
                let done = snap(&y);
                if done != snap(&x) { fail("history-differs", String::new()); }
                y.undo();
                if snap(&y) != st { fail("undo", format!("{:?}", snap(&y))); }
                y.redo();
                if snap(&y) != done { fail("redo", format!("{:?}", snap(&y))); }
            }
        }}
    }
    let fails: BTreeMap<&str, Vec<String>> = fails.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    assert!(fails.is_empty(), "{}", summary(cases, &fails));
}


/// Shift+Return puts a soft break in the text: over a selection it is the deletion of the
/// selection, by the rules for text, and then a soft line in the paragraph that is left, in that
/// paragraph's block.
#[test]
fn shift_return_over_a_selection_deletes_it_and_puts_a_soft_line_in_the_paragraph_left() {
    let shift = KeyEvent { key: Key::Enter, mods: Mods { shift: true, ..Mods::default() } };
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (doc, e) in docs() {
        let len = e.text.len();
        for a in 0..=len { for b in a + 1..=len {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            cases += 1;
            let mut s = e.clone();
            s.set_selection(a, b);
            let before = seen(&s);
            s.key(&shift, None);
            let ch = s.take_changes();
            let mut d = e.clone();
            d.set_selection(a, b);
            d.key(&KeyEvent { key: Key::Backspace, mods: Mods::default() }, None);
            d.key(&shift, None);
            let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs().iter().map(|a| a.id.min(99)).collect::<Vec<_>>(), y.blocks().to_vec(), y.soft_breaks().to_vec());
            let msg = format!("{doc} {a}..{b} {:?}", &e.text[a..b]);
            if let Err(w) = check(&before, &s, &ch) { fails.push(format!("{msg}: report {w}")); }
            if !soft_ok(&s) { fails.push(format!("{msg}: an atom is a soft line or has one: {:?}", s.soft_breaks())); }
            if snap(&s) != snap(&d) { fails.push(format!("{msg}: {:?} but delete and soft break give {:?}", snap(&s), snap(&d))); }
            // the soft line has its paragraph's block
            for &line in s.soft_breaks() {
                let line = line as usize;
                if line > 0 && s.blocks()[line] != s.blocks()[line - 1] { fails.push(format!("{msg}: soft line {line} is {:?} in {:?}", s.blocks()[line], s.blocks()[line - 1])); }
            }
            let mut y = e.clone();
            y.set_history(true);
            y.set_selection(a, b);
            let st = (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), y.soft_breaks().to_vec());
            y.key(&shift, None);
            let done = (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), y.soft_breaks().to_vec());
            y.undo();
            if (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), y.soft_breaks().to_vec()) != st { fails.push(format!("{msg}: undo")); }
            y.redo();
            if (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), y.soft_breaks().to_vec()) != done { fails.push(format!("{msg}: redo")); }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

pub(super) fn summary(cases: usize, fails: &BTreeMap<&str, Vec<String>>) -> String {
    let mut out = format!("{cases} cases\n");
    for (k, v) in fails {
        out += &format!("== {k}: {}\n", v.len());
        for s in v.iter().take(6) { out += &format!("  {s}\n"); }
    }
    out
}









#[test]
fn shift_return_is_the_text_of_a_break_with_a_soft_line_and_never_beside_an_atom() {
    let shift = KeyEvent { key: Key::Enter, mods: Mods { shift: true, ..Mods::default() } };
    let mut fails: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cases = 0;
    for (doc, e) in docs() {
        let len = e.text.len();
        let orig: Vec<u64> = e.paragraph_attrs().iter().map(|a| a.id).collect();
        for a in 0..=len { for b in a..=len {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            cases += 1;
            let mut s = e.clone();
            s.set_selection(a, b);
            let before = seen(&s);
            s.key(&shift, None);
            let ch = s.take_changes();
            let mut t = e.clone();
            t.set_selection(a, b);
            t.insert("\u{1}");
            let (sr, tr) = (rows(&s, &orig), rows(&t, &orig));
            let atom = (e.line_of_byte(a)..=e.line_of_byte(b)).any(|l| e.paragraph(l).atom.is_some());
            let tag = format!("{}{}", if a == b { "/caret" } else { "" }, if atom { "/atom" } else { "" });
            let msg = format!("{doc} {a}..{b} {:?}\n   shift: {} soft {:?} caret {:?}\n   typed: {} soft {:?}", &e.text[a..b], show(&sr), s.soft_breaks(), s.range(), show(&tr), t.soft_breaks());
            if let Err(w) = check(&before, &s, &ch) { fails.entry(format!("report{tag}")).or_default().push(format!("{msg}\n   {w}")); }
            if !soft_ok(&s) { fails.entry(format!("soft-atom{tag}")).or_default().push(msg.clone()); }
            let ids: Vec<u64> = s.paragraph_attrs().iter().map(|a| a.id).collect();
            let mut u = ids.clone(); u.sort(); u.dedup();
            if u.len() != ids.len() { fails.entry(format!("dup{tag}")).or_default().push(msg.clone()); }
            for (i, x) in s.paragraph_attrs().iter().enumerate() {
                if x.atom.is_none() && e.paragraph_attrs().iter().any(|o| o.atom.is_some() && o.id == x.id) { fails.entry(format!("atomid{tag}")).or_default().push(format!("{msg} line {i}")); }
            }
            for &l in s.soft_breaks() {
                let l = l as usize;
                if s.paragraph(l).atom.is_some() { fails.entry(format!("soft-atom{tag}")).or_default().push(msg.clone()); }
                if l > 0 && s.paragraph(l - 1).atom.is_some() { fails.entry(format!("soft-after-atom{tag}")).or_default().push(msg.clone()); }
                if l > 0 && s.blocks()[l] != s.blocks()[l - 1] { fails.entry(format!("soft-block{tag}")).or_default().push(msg.clone()); }
            }
            // An atom is never a soft line: beside one the break is a hard break, so a caret gives Return's
            // result, and a selection gives its deletion and then Shift+Return.
            if atom {
                let mut other = e.clone();
                other.set_selection(a, b);
                if a == b { other.key(&KeyEvent { key: Key::Enter, mods: Mods::default() }, None); } else { other.insert(""); other.key(&shift, None); }
                if (rows(&s, &orig), s.soft_breaks().to_vec()) != (rows(&other, &orig), other.soft_breaks().to_vec()) { fails.entry(format!("atom-shift{tag}")).or_default().push(format!("{msg}\n   expected {} soft {:?}", show(&rows(&other, &orig)), other.soft_breaks())); }
                let mut y = e.clone();
                y.set_history(true);
                y.set_selection(a, b);
                let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()), y.soft_breaks().to_vec());
                let st = snap(&y);
                y.key(&shift, None);
                let done = snap(&y);
                y.undo();
                if snap(&y) != st { fails.entry(format!("undo{tag}")).or_default().push(msg.clone()); }
                y.redo();
                if snap(&y) != done { fails.entry(format!("redo{tag}")).or_default().push(msg.clone()); }
                continue;
            }
            let i = tr.iter().position(|r| r.0.contains('\u{1}'));
            let Some(i) = i else { fails.entry(format!("no-x{tag}")).or_default().push(msg.clone()); continue; };
            if sr.len() != tr.len() + 1 { fails.entry(format!("len{tag}")).or_default().push(msg.clone()); continue; }
            let (pre, suf) = tr[i].0.split_once('\u{1}').unwrap();
            let mut ok = true;
            for j in 0..i { ok &= sr[j] == tr[j]; }
            for j in i + 1..tr.len() { ok &= sr[j + 1] == tr[j]; }
            let mut first = tr[i].clone(); first.0 = pre.into();
            if !ok { fails.entry(format!("others{tag}")).or_default().push(msg.clone()); }
            else if sr[i] != first { fails.entry(format!("first-part{tag}")).or_default().push(msg.clone()); }
            else if sr[i + 1].0 != suf || sr[i + 1].6 != tr[i].6 || !s.soft_breaks().contains(&((i + 1) as u32)) { fails.entry(format!("soft-part{tag}")).or_default().push(msg.clone()); }
            else if (sr[i + 1].2, sr[i + 1].3, sr[i + 1].4) != (tr[i].2, tr[i].3, tr[i].4) { fails.entry(format!("soft-attrs{tag}")).or_default().push(msg.clone()); }
            // softs otherwise the same
            let want: BTreeSet<u32> = t.soft_breaks().iter().map(|&l| if l as usize > i { l + 1 } else { l }).chain([(i + 1) as u32]).collect();
            let got: BTreeSet<u32> = s.soft_breaks().iter().copied().collect();
            if ok && want != got { fails.entry(format!("softset{tag}")).or_default().push(msg.clone()); }
            if s.range() != (a + 1 + (s.text.len() - (len - (b - a)) - 1), a + 1 + (s.text.len() - (len - (b - a)) - 1)) && false { }
            let mut y = e.clone();
            y.set_history(true);
            y.set_selection(a, b);
            let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()), y.soft_breaks().to_vec());
            let st = snap(&y);
            y.key(&shift, None);
            let done = snap(&y);
            y.undo();
            if snap(&y) != st { fails.entry(format!("undo{tag}")).or_default().push(msg.clone()); }
            y.redo();
            if snap(&y) != done { fails.entry(format!("redo{tag}")).or_default().push(msg.clone()); }
        }}
    }
    let fails: BTreeMap<&str, Vec<String>> = fails.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    assert!(fails.is_empty(), "{}", summary(cases, &fails));
}




