//! The rest of the folded probes: atoms that are soft lines or have them, atoms through copy,
//! paste and cut, pasting what is selected, and the edges of code. The documents and helpers are
//! `path_agreement_tests`'.

use std::collections::BTreeMap;

use super::fuzz_tests::{check, seen};
use super::path_agreement_tests::{docs, fragments, line_ranges, marked, mk, mkfrag, mr, rows, show, summary, Row, NEW};
use super::*;

/// A fragment that is the lines `a..b` select: their text, marks, blocks and attributes, under
/// ids of their own.
fn fragment_of(e: &TextEdit, a: usize, b: usize) -> TextEdit {
    let (la, lb) = (e.line_of_byte(a), e.line_of_byte(b));
    let marks: Vec<MarkRange> = e.marks().iter().filter(|m| m.start < b && a < m.end).map(|m| MarkRange::new(m.start.max(a) - a..m.end.min(b) - a, m.mark.clone())).collect();
    let mut f = TextEdit::from_parts(&e.text[a..b], marks, (la..=lb).map(|i| e.blocks()[i]).collect());
    f.set_paragraph_attrs((la..=lb).map(|i| { let mut x = e.paragraph(i); x.id = 500 + i as u64; x }).collect());
    f
}

#[test]
fn pasting_what_the_selection_holds_changes_nothing() {
    let mut cases = 0;
    let mut fails = Vec::new();
    for (doc, e) in docs() {
        let n = e.text.len();
        let ol = line_ranges(&e.text);
        for a in 0..=n { for b in a + 1..=n {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            // An atom's label cut in two is a different fragment.
            let cut = (0..ol.len()).any(|i| e.paragraph(i).atom.is_some() && ((a > ol[i].0 && a < ol[i].1) || (b > ol[i].0 && b < ol[i].1)));
            if cut { continue; }
            cases += 1;
            let mut x = e.clone();
            x.set_selection(a, b);
            x.paste_fragment(fragment_of(&e, a, b), "");
            let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()));
            if snap(&x) != snap(&e) { fails.push(format!("{doc} {a}..{b} {:?}\n   was {} soft {:?}\n   now {} soft {:?}", &e.text[a..b], show(&rows(&e, &[1, 2, 3, 4])), e.soft_breaks(), show(&rows(&x, &[1, 2, 3, 4])), x.soft_breaks())); }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(10).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn a_fragment_atom_replacing_a_whole_paragraph_before_an_atom_leaves_no_empty_line() {
    let d = ParagraphAttrs::default();
    for (doc, a, b, frag) in [("atom", 0, 4, 0), ("atom", 0, 4, 1), ("list", 0, 4, 0), ("list", 0, 4, 1)] {
        let (_, e) = docs().into_iter().find(|x| x.0 == doc).unwrap();
        let mut x = e.clone();
        x.set_selection(a, b);
        x.paste_fragment(mkfrag(&fragments()[frag]), "");
        let atom = fragments()[frag].2[0];
        let block = fragments()[frag].1[0];
        let got = rows(&x, &[1, 2, 3, 4]);
        let mut want_rest: Vec<Row> = rows(&e, &[1, 2, 3, 4])[1..].to_vec();
        want_rest.iter_mut().for_each(|_| ());
        assert_eq!(got.len(), want_rest.len() + 1, "{doc} frag {frag}: {}", show(&got));
        assert_eq!((got[0].0.as_str(), got[0].1, got[0].2, got[0].5, got[0].6), ("Q", NEW, atom.style, Some(20.0), block), "{doc} frag {frag}: {}", show(&got));
        assert_eq!(&got[1..], &want_rest[..], "{doc} frag {frag}");
        let _ = d;
    }
}

#[test]
fn a_fragment_atom_stays_an_atom_in_a_code_block_and_the_text_around_it_stays_code() {
    let d = ParagraphAttrs::default();
    let mut e = mk("abc\nxyz", vec![], vec![Block::Code, Block::Paragraph], vec![d.id(1), d.id(2)]);
    e.set_selection(1, 1);
    let mut f = TextEdit::from_parts("x\nQ\ny", vec![mr(0..1, Mark::Bold)], vec![Block::Paragraph; 3]);
    f.set_paragraph_attrs(vec![d.id(51), d.id(50).atom(20.0), d.id(52)]);
    e.paste_fragment(f, "");
    let got = rows(&e, &[1, 2]);
    let kinds: Vec<(&str, bool, Block)> = got.iter().map(|r| (r.0.as_str(), r.5.is_some(), r.6)).collect();
    assert_eq!(kinds, vec![("ax", false, Block::Code), ("Q", true, Block::Paragraph), ("ybc", false, Block::Code), ("xyz", false, Block::Paragraph)], "{}", show(&got));
    assert!(e.marks().is_empty(), "text goes in as plain text: {:?}", e.marks());
    assert_eq!(got[0].1, 1);
}

#[test]
fn an_atom_in_every_block_survives_copy_paste_and_cut_and_paste() {
    let d = ParagraphAttrs::default();
    let blocks = [Block::Paragraph, Block::Bullet(0), Block::Bullet(1), Block::Numbered(0), Block::Numbered(2), Block::Quote, Block::Code];
    let mut fails: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cases = 0;
    // sources: before / atom / after with every block combination; also two atoms adjacent
    for &ab in &blocks { for &nb in &blocks { for shape in 0..5 {
        let (text, bl, at): (&str, Vec<Block>, Vec<ParagraphAttrs>) = match shape {
            0 => ("aa\nTab\nbb", vec![nb, ab, nb], vec![d.id(1), d.id(2).atom(40.0), d.id(3)]),
            1 => ("Tab\nbb", vec![ab, nb], vec![d.id(1).atom(40.5), d.id(2)]),
            2 => ("aa\nTab", vec![nb, ab], vec![d.id(1), d.id(2).atom(40.0)]),
            3 => ("Ta\nTb\nbb", vec![ab, nb, nb], vec![d.id(1).atom(40.0), d.id(2).atom(41.0), d.id(3)]),
            _ => ("Tab", vec![ab], vec![d.id(1).atom(40.0).align(ParagraphAlign::Center)]),
        };
        if shape == 4 && nb != blocks[0] { continue; }
        let src = mk(text, vec![], bl.clone(), at.clone());
        let n = src.text.len();
        let ol = line_ranges(&src.text);
        for a in 0..=n { for b in a + 1..=n {
            let mut s = src.clone();
            s.set_selection(a, b);
            if s.range() != (a, b) { continue; }
            cases += 1;
            let (la, lb) = (src.line_of_byte(a), src.line_of_byte(b));
            let html = s.selected_html();
            let f = s.html_fragment(&html);
            let name = format!("{text:?} {bl:?} sel {a}..{b} {:?}", &src.text[a..b]);
            let msg = format!("{name}\n     html {html}\n     frag {:?} blocks {:?} attrs {:?}", f.text, f.blocks(), f.paragraph_attrs().iter().map(|x| (x.atom, x.id, x.align)).collect::<Vec<_>>());
            let tag = format!("/atom{ab:?}");
            // expected fragment: text == selection text (a trailing break gives an empty last line)
            let sel = &src.text[a..b];
            if f.text != sel { fails.entry(format!("frag-text{tag}/nb{nb:?}")).or_default().push(msg.clone()); continue; }
            let fl = f.text.split('\n').count();
            if f.blocks().len() != fl { fails.entry(format!("frag-lines{tag}")).or_default().push(msg.clone()); continue; }
            for k in 0..fl {
                let line = la + k;
                let whole = a <= ol[line].0 && b >= ol[line].1;
                let src_atom = at[line].atom.filter(|_| whole);
                let got = f.paragraph(k);
                if got.atom != src_atom { fails.entry(format!("frag-atom{tag}")).or_default().push(format!("{msg}\n     line {k} atom {:?} want {:?}", got.atom, src_atom)); }
                if src_atom.is_some() && got.id != at[line].id { fails.entry(format!("frag-atom-id{tag}")).or_default().push(msg.clone()); }
                let empty_tail = k == fl - 1 && k > 0 && b == ol[line].0;
                // (A copy that begins at a line's end begins with a plain empty paragraph.)
                let empty_lead = k == 0 && a == ol[line].1 && b > a;
                if !empty_tail && !empty_lead && f.blocks()[k] != bl[line] { fails.entry(format!("frag-block{}{tag}/line{:?}", if src_atom.is_some() { "-ATOM" } else { "" }, bl[line])).or_default().push(format!("{msg}\n     line {k} block {:?} want {:?}", f.blocks()[k], bl[line])); }
                if src_atom.is_some() && got.align != at[line].align { fails.entry(format!("frag-atom-align{tag}")).or_default().push(msg.clone()); }
            }
            // pasting it back over the selection changes nothing
            let mut x = src.clone();
            x.set_selection(a, b);
            x.paste_fragment(f.clone(), &src.text[a..b]);
            let snap = |y: &TextEdit| (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec());
            let cut_label = (la..=lb).any(|l| at[l].atom.is_some() && !(a <= ol[l].0 && b >= ol[l].1) && a < ol[l].1 && b > ol[l].0);
            if snap(&x) != snap(&src) { fails.entry(format!("self-paste{}{tag}", if cut_label { "/cutlabel" } else { "" })).or_default().push(format!("{msg}\n     now {}", show(&rows(&x, &[1, 2, 3])))); }
            // cut, then paste at the caret, here and into other documents: atoms stay atoms, under new ids
            let has_atom = (la..=lb).any(|l| at[l].atom.is_some() && a <= ol[l].0 && b >= ol[l].1);
            if has_atom && !cut_label {
                let mut x = src.clone();
                x.set_selection(a, b);
                x.insert("");
                x.paste_fragment(f.clone(), &src.text[a..b]);
                let ids: Vec<u64> = x.paragraph_attrs().iter().map(|a| a.id).collect();
                let mut u = ids.clone(); u.sort(); u.dedup();
                if u.len() != ids.len() { fails.entry(format!("cut-paste-dup{tag}")).or_default().push(msg.clone()); }
                // into other documents, at every caret and a few selections
                for (dn, dst) in [
                    ("empty", mk("", vec![], vec![Block::Paragraph], vec![d.id(1)])),
                    ("text", mk("xy\nzw", vec![], vec![Block::Paragraph, Block::Bullet(0)], vec![d.id(1), d.id(2).style(2)])),
                    ("code", mk("xy\nzw", vec![], vec![Block::Code, Block::Code], vec![d.id(1), d.id(2)])),
                    ("quote", mk("xy\n\nzw", vec![], vec![Block::Quote, Block::Numbered(0), Block::Quote], vec![d.id(1), d.id(2), d.id(3)])),
                    ("atomdoc", mk("Tq\nzw", vec![], vec![Block::Quote, Block::Paragraph], vec![d.id(1).atom(9.0), d.id(2)])),
                ] {
                    let m = dst.text.len();
                    for c in 0..=m { for e2 in c..=m {
                        let mut x = dst.clone();
                        x.set_selection(c, e2);
                        if x.range() != (c, e2) { continue; }
                        let before = seen(&x);
                        x.paste_fragment(f.clone(), &src.text[a..b]);
                        let chg = x.take_changes();
                        let m2 = format!("{msg}\n     into {dn} {c}..{e2}: {}", show(&rows(&x, &[1, 2, 3])));
                        if let Err(w) = check(&before, &x, &chg) { fails.entry(format!("dst-report/{dn}")).or_default().push(format!("{m2} {w}")); }
                        // each atom of the fragment is there, an atom on its own line, new id, its block
                        let want: Vec<(String, Option<f32>, Block)> = (0..fl).filter(|&k| f.paragraph(k).atom.is_some()).map(|k| (f.text.split('\n').nth(k).unwrap().to_string(), f.paragraph(k).atom, f.blocks()[k])).collect();
                        let got: Vec<(String, Option<f32>, Block)> = x.text.split('\n').enumerate().filter(|(i, _)| x.paragraph(*i).atom.is_some() && x.paragraph(*i).id >= 100).map(|(i, t)| (t.to_string(), x.paragraph(i).atom, x.blocks()[i])).collect();
                        if want != got { fails.entry(format!("dst-atoms{tag}/{dn}")).or_default().push(m2.clone()); }
                        let ids: Vec<u64> = x.paragraph_attrs().iter().map(|a| a.id).collect();
                        let mut u = ids.clone(); u.sort(); u.dedup();
                        if u.len() != ids.len() || ids.contains(&0) { fails.entry(format!("dst-ids/{dn}")).or_default().push(m2.clone()); }
                        if !x.text.contains(&f.text) { fails.entry(format!("dst-text/{dn}")).or_default().push(m2.clone()); }
                    }}
                }
            }
        }}
    }}}
    let fails: BTreeMap<&str, Vec<String>> = fails.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    assert!(fails.is_empty(), "{}", summary(cases, &fails));
}

/// Shift+Return beside an atom, and text typed after it, copy and paste with the atom whole:
/// an atom is never a soft line and never has one.
#[test]
fn shift_return_beside_an_atom_keeps_the_atom_through_copy_and_paste() {
    let shift = KeyEvent { key: Key::Enter, mods: Mods { shift: true, ..Mods::default() } };
    let d = ParagraphAttrs::default();
    for (a, b, typed) in [(4, 4, ""), (0, 4, ""), (7, 7, "xyz"), (7, 11, "xyz"), (7, 7, ""), (4, 7, "")] {
        let mut e = mk("AAA\nTab\nend", vec![], vec![Block::Paragraph, Block::Paragraph, Block::Bullet(0)], vec![d.id(1), d.id(2).style(2).atom(40.0), d.id(3).style(3)]);
        e.set_selection(a, b);
        e.key(&shift, None);
        if !typed.is_empty() { e.key(&KeyEvent { key: Key::Text(typed.into()), mods: Mods::default() }, None); }
        let at: Vec<usize> = (0..e.blocks().len()).filter(|&i| e.paragraph(i).atom.is_some()).collect();
        for &l in e.soft_breaks() {
            let l = l as usize;
            assert!(e.paragraph(l).atom.is_none() && (l == 0 || e.paragraph(l - 1).atom.is_none()), "{a}..{b} {typed:?}: soft line {l} beside an atom: {:?} {:?}", e.text, e.soft_breaks());
        }
        let n = e.text.len();
        e.set_selection(0, n);
        let f = e.html_fragment(&e.selected_html());
        let label: Vec<&str> = f.text.split('\n').enumerate().filter(|(i, _)| f.paragraph(*i).atom.is_some()).map(|(_, t)| t).collect();
        // an atom the selection took away (4..7 above) is gone; any other is whole in the copy
        let kept = at.len();
        assert_eq!((label.len(), label.iter().all(|t| *t == "Tab")), (kept, true), "{a}..{b} {typed:?}: {:?} {:?}", e.text, f.text);
        let mut t = mk("", vec![], vec![Block::Paragraph], vec![d.id(1)]);
        t.paste_fragment(f, "");
        let pasted = (0..t.blocks().len()).filter(|&i| t.paragraph(i).atom.is_some()).count();
        assert_eq!(pasted, kept, "{a}..{b} {typed:?}: pasted {:?} from {:?}", t.text, e.text);
    }
}

fn snap(y: &TextEdit) -> (String, Vec<ParagraphAttrs>, Vec<Block>, std::collections::BTreeSet<(usize, String)>, Vec<u32>) {
    (y.text.clone(), y.paragraph_attrs(), y.blocks().to_vec(), marked(y.marks(), 0, y.text.len()), y.soft_breaks().to_vec())
}

/// `e` with a paragraph style for each of 1 to 9, so a copy carries them.
fn styled(mut e: TextEdit) -> TextEdit {
    const C: [&str; 9] = ["s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9"];
    e.set_html_styles((1..=9u16).map(|i| HtmlStyle { style: i, tag: "p", class: Some(C[i as usize - 1]) }).collect::<Vec<_>>());
    e
}

fn atom_free_frag(text: &str, blocks: Vec<Block>, attrs: Vec<ParagraphAttrs>) -> TextEdit {
    let mut x = TextEdit::from_parts(text, vec![], blocks);
    x.set_paragraph_attrs(attrs);
    x
}

/// An atom is never a soft line and never has one, whichever edit leaves it beside a soft line.
#[test]
fn an_atom_is_never_a_soft_line_whatever_edits_or_pastes_beside_it() {
    let soft_docs: Vec<(&str, TextEdit)> = docs().into_iter().filter(|d| d.0.starts_with("soft")).collect();
    type P = (&'static str, fn(&mut TextEdit));
    let paths: [P; 7] = [
        ("delete", |e| e.insert("")),
        ("backspace", |e| { e.key(&KeyEvent { key: Key::Backspace, mods: Mods::default() }, None); }),
        ("typed", |e| { e.key(&KeyEvent { key: Key::Text("x".into()), mods: Mods::default() }, None); }),
        ("insert", |e| e.insert("A\nB")),
        ("return", |e| { e.key(&KeyEvent { key: Key::Enter, mods: Mods::default() }, None); }),
        ("shift", |e| { e.key(&KeyEvent { key: Key::Enter, mods: Mods { shift: true, ..Mods::default() } }, None); }),
        ("paste atoms", |e| { e.paste_fragment(mkfrag(&fragments()[0]), ""); }),
    ];
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (doc, e) in soft_docs {
        let n = e.text.len();
        for a in 0..=n { for b in a..=n {
            let mut pr = e.clone();
            pr.set_selection(a, b);
            if pr.range() != (a, b) { continue; }
            for (name, f) in paths.iter() {
                cases += 1;
                let mut x = e.clone();
                x.set_selection(a, b);
                f(&mut x);
                if !super::path_agreement_tests::soft_ok(&x) { fails.push(format!("{doc} {a}..{b} {name}: soft {:?} text {:?}", x.soft_breaks(), x.text)); }
            }
            // atom fragments of every kind, pasted over the selection
            for (fi, fr) in fragments().iter().enumerate() {
                if !fr.2.iter().any(|y| y.atom.is_some()) { continue; }
                cases += 1;
                let mut x = e.clone();
                x.set_selection(a, b);
                x.paste_fragment(mkfrag(fr), "");
                if !super::path_agreement_tests::soft_ok(&x) { fails.push(format!("{doc} {a}..{b} frag#{fi}: soft {:?} text {:?}", x.soft_breaks(), x.text)); }
            }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

/// An empty line a fragment that ends in an atom lands at stays whole, with its id, attributes and
/// page break; only a single line pasted into a plain Paragraph line takes the line.
#[test]
fn an_empty_line_a_fragment_ending_in_an_atom_lands_at_stays_whole() {
    let d = ParagraphAttrs::default();
    let (p, b, q, c, n) = (Block::Paragraph, Block::Bullet(0), Block::Quote, Block::Code, Block::Numbered(0));
    let qa = || d.id(50).atom(20.0);
    let recvs: Vec<(&str, Block, ParagraphAttrs)> = vec![
        ("idonly", p, d.id(2)), ("idonly-bullet", b, d.id(2)), ("idonly-quote", q, d.id(2)), ("idonly-code", c, d.id(2)), ("idonly-numbered", n, d.id(2)),
        ("pagebreak", p, d.id(2).page_break_before(true)), ("style2", p, d.id(2).style(2)), ("style9-pb", p, d.id(2).style(9).page_break_before(true)),
        ("indent", p, d.id(2).indent(4.0, 1.0)), ("center", p, d.id(2).align(ParagraphAlign::Center)),
    ];
    let frags: Vec<(&str, Vec<Block>, Vec<ParagraphAttrs>)> = vec![
        ("Q", vec![p], vec![qa()]), ("Q", vec![q], vec![qa().style(9)]), ("A\nQ", vec![p, p], vec![d.id(51).style(7), qa()]),
        ("A\nQ", vec![p, q], vec![d.id(51), qa().style(9)]), ("Q\nQ", vec![p, q], vec![qa(), d.id(53).atom(20.0)]),
    ];
    let mut fails: Vec<String> = Vec::new();
    for (name, blk, at) in &recvs {
        for (ft, fb, fa) in &frags {
            for ids in [true, false] {
                let mut e = mk("top\n\nend", vec![], vec![p, *blk, p], vec![d.id(1), *at, d.id(3).style(3)]);
                if !ids {
                    let attrs: Vec<ParagraphAttrs> = e.paragraph_attrs().into_iter().map(|a| a.id(0)).collect();
                    e.set_paragraph_ids(0);
                    e.set_paragraph_attrs(attrs);
                }
                e.take_changes();
                e.set_selection(4, 4);
                let (before, mut y) = (seen(&e), e.clone());
                let mut recv = e.paragraph(1);
                let f = atom_free_frag(ft, fb.clone(), fa.clone());
                e.paste_fragment(f.clone(), "");
                let ch = e.take_changes();
                let msg = format!("{name} ids={ids} frag {ft:?}: {}", show(&rows(&e, &[1, 2, 3])));
                // a single line into a plain Paragraph line takes the line, as before
                let mut plain = recv;
                plain.id = 0;
                let takes = fa.len() == 1 && plain == ParagraphAttrs::default() && *blk == p;
                recv.id = if ids { 2 } else { 0 };
                let kept = (0..e.blocks().len()).any(|i| e.text.split('\n').nth(i) == Some("") && e.paragraph(i) == recv && e.blocks()[i] == *blk);
                if !takes && !kept { fails.push(format!("{msg}: the empty line did not stay whole")); }
                if let Err(w) = check(&before, &e, &ch) { fails.push(format!("{msg}: report {w}")); }
                y.set_history(true);
                let st = snap(&y);
                y.paste_fragment(f, "");
                let done = snap(&y);
                y.undo();
                if snap(&y) != st { fails.push(format!("{msg}: undo")); }
                y.redo();
                if snap(&y) != done { fails.push(format!("{msg}: redo")); }
            }
        }
    }
    assert!(fails.is_empty(), "{} failures, the first:\n{}", fails.len(), fails.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

/// Paragraphs whole, from a line's start to another's, with no atom in them, cut and pasted back at
/// the caret, are back, ids aside. (Where the copy carries the selection exactly.)
#[test]
fn cut_and_paste_in_place_restores_whole_paragraphs_without_atoms() {
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    // An unmarked paragraph above a marked or linked one: what it pastes back as must not take their marks.
    let mut all = docs();
    let next_marked = |m: Mark| { let mut e = TextEdit::from_parts("body\nTerm rest", vec![MarkRange::new(5..9, m)], vec![Block::Paragraph; 2]); e.set_paragraph_attrs(vec![ParagraphAttrs::default().id(1), ParagraphAttrs::default().id(2)]); e.set_paragraph_ids(100); e.take_changes(); e };
    all.push(("next-bold", next_marked(Mark::Bold)));
    all.push(("next-link", next_marked(Mark::Link("https://example.com".into()))));
    for (doc, e) in all {
        let e = styled(e);
        let len = e.text.len();
        let ol = line_ranges(&e.text);
        for a in 0..=len { for b in a + 1..=len {
            let mut s = e.clone();
            s.set_selection(a, b);
            if s.range() != (a, b) { continue; }
            let (la, lb) = (e.line_of_byte(a), e.line_of_byte(b));
            let whole = a == ol[la].0 && la < lb && (b == ol[lb].0 || (b == len && b == ol[lb].1));
            let atoms = (la..=lb).any(|l| e.paragraph(l).atom.is_some());
            // (The line the selection ends before is not in the copy: only the lines cut count.)
            let lossy = (la..lb).any(|l| {
                let o = e.paragraph(l);
                (o.style != 0 && e.blocks()[l] != Block::Paragraph) || o.page_break_before || o.indent != 0.0 || o.first_line_indent != 0.0 || e.blocks()[l] == Block::Code
            }) || e.marks().iter().any(|m| m.mark.is_pill() && m.start < b && m.end > a) ;
            // A paragraph right next to a code block pastes back by position: a line made beside code is code.
            let beside_code = e.blocks()[lb] == Block::Code || (la > 0 && e.blocks()[la - 1] == Block::Code);
            if !whole || atoms || lossy || beside_code || e.text[a..b] == *"\n" { continue; }
            cases += 1;
            let html = s.selected_html();
            let f = s.html_fragment(&html);
            let mut x = e.clone();
            x.set_selection(a, b);
            x.insert("");
            x.paste_fragment(f, &e.text[a..b]);
            let key = |y: &TextEdit| y.text.split('\n').enumerate().map(|(i, t)| { let mut at = y.paragraph(i); at.id = 0; (t.to_string(), at, y.blocks()[i]) }).collect::<Vec<_>>();
            if key(&x) != key(&e) || snap(&x).3 != snap(&e).3 || x.soft_breaks() != e.soft_breaks() { fails.push(format!("{doc} {a}..{b} {:?}\n   was {} soft {:?}\n   now {} soft {:?}", &e.text[a..b], show(&rows(&e, &[1, 2, 3, 4])), e.soft_breaks(), show(&rows(&x, &[1, 2, 3, 4])), x.soft_breaks())); }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

/// A copy through `selected_html` pasted over the same selection is a no-op where the HTML carries
/// the selection exactly: no indent, page break, styled list item or quote, code or pill, which the HTML cannot say.
#[test]
fn pasting_a_copy_of_the_selection_back_is_a_no_op_where_the_html_carries_it() {
    let mut fails: Vec<String> = Vec::new();
    let mut cases = 0;
    for (doc, e) in docs() {
        let e = styled(e);
        let len = e.text.len();
        let ol = line_ranges(&e.text);
        for a in 0..=len { for b in a + 1..=len {
            let mut s = e.clone();
            s.set_selection(a, b);
            if s.range() != (a, b) { continue; }
            let (la, lb) = (e.line_of_byte(a), e.line_of_byte(b));
            let lossy = (la..=lb).any(|l| {
                let o = e.paragraph(l);
                (o.style != 0 && e.blocks()[l] != Block::Paragraph) || (o.page_break_before && a <= ol[l].0) || o.indent != 0.0 || o.first_line_indent != 0.0 || e.blocks()[l] == Block::Code
            }) || e.marks().iter().any(|m| m.mark.is_pill() && m.start < b && m.end > a);
            let html = s.selected_html();
            let f = s.html_fragment(&html);
            let sel = &e.text[a..b];
            if lossy { continue; }
            cases += 1;
            let atoms = (la..=lb).any(|l| e.paragraph(l).atom.is_some());
            for (path, hook) in [("paste_fragment", true), ("paste_html", false)] {
                if !hook && atoms { continue; }
                let mut x = e.clone();
                x.set_selection(a, b);
                if hook { x.paste_fragment(f.clone(), sel); } else { x.paste_html(&html, sel); }
                if snap(&x) != snap(&e) { fails.push(format!("{path} {doc} {a}..{b} {sel:?}\n   html {html}\n   now {}", show(&rows(&x, &[1, 2, 3, 4])))); }
            }
        }}
    }
    assert!(fails.is_empty(), "{cases} cases, {} failures, the first:\n{}", fails.len(), fails.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}
