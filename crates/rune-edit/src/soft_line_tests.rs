//! Soft lines are part of their paragraph, the hard line they follow is its head: they mirror its block, style,
//! alignment and indents, are never a paragraph's start, and the paragraph's identity is the head's. The broad
//! probe runs every edit over every selection of documents with soft lines (in a document and in a field without
//! attributes, with atoms, code, lists and quotes); the others are the scenarios that found the rules.

use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[derive(Clone, PartialEq, Debug)]
struct St { text: String, attrs: Vec<ParagraphAttrs>, blocks: Vec<Block>, marks: Vec<(usize, usize, String)>, soft: Vec<u32> }

fn st(e: &TextEdit) -> St {
    let mut marks: Vec<(usize, usize, String)> = e.marks().iter().map(|m| (m.start, m.end, format!("{:?}", m.mark))).collect();
    marks.sort();
    St { text: e.text.clone(), attrs: e.paragraph_attrs(), blocks: e.blocks().to_vec(), marks, soft: e.soft_breaks().to_vec() }
}

fn noid(e: &TextEdit) -> St { let mut s = st(e); for a in &mut s.attrs { a.id = 0; } s }

fn nomarks(e: &TextEdit) -> St { let mut s = st(e); s.marks.clear(); s }

fn blk(b: Block) -> &'static str { match b { Block::Paragraph => "P", Block::Code => "C", Block::Quote => "Q", Block::Bullet(_) => "U", _ => "N" } }

fn show(e: &TextEdit) -> String {
    let lines: Vec<String> = e.text.split('\n').enumerate().map(|(i, t)| {
        let a = e.paragraph(i);
        format!("{}{t:?}#{}{}{}{}{}", if e.soft_breaks().contains(&(i as u32)) { "~" } else { "" }, a.id, if a.align != ParagraphAlign::Left { "^" } else { "" }, if a.indent != 0.0 { ">" } else { "" }, if a.atom.is_some() { "@" } else { "" }, blk(e.blocks()[i]))
    }).collect();
    format!("{} m{:?} c{:?}", lines.join(" | "), e.marks().iter().map(|m| (m.start, m.end)).collect::<Vec<_>>(), e.range())
}

fn doc(text: &str, blocks: Vec<Block>, atoms: &[usize], soft: &[u32], marks: Vec<MarkRange>, attributed: bool) -> TextEdit {
    let n = text.split('\n').count();
    let mut e = TextEdit::from_parts(text, marks, blocks);
    if attributed {
        let mut para = 0u16;
        // Paragraphs alternate between left and centred, and a soft line is its head's (a document keeps that).
        let attrs: Vec<ParagraphAttrs> = (0..n).map(|i| {
            if i > 0 && !soft.contains(&(i as u32)) { para += 1; }
            let a = ParagraphAttrs::default().id(1 + i as u64).align(if para % 2 == 1 { ParagraphAlign::Center } else { ParagraphAlign::Left });
            if atoms.contains(&i) { a.atom(20.0) } else { a }
        }).collect();
        e.set_paragraph_attrs(attrs);
        e.set_paragraph_ids(100);
    }
    if !soft.is_empty() { e.set_soft_breaks(soft.to_vec()); }
    e.take_changes();
    e
}

/// What is wrong with the state: a soft line that is not its head's in block, style, alignment or indents, a
/// soft line first in the text or beside an atom, marks in code.
fn inv(x: &TextEdit) -> String {
    let mut bad = String::new();
    let s = st(x);
    let mut off = 0;
    for (i, t) in x.text.split('\n').enumerate() {
        if s.blocks[i] == Block::Code && x.marks().iter().any(|m| m.start < off + t.len() && m.end > off) { bad.push_str(" MARK-IN-CODE"); }
        off += t.len() + 1;
        let soft = s.soft.contains(&(i as u32));
        if soft && i > 0 && s.blocks[i] != s.blocks[i - 1] { bad.push_str(" SOFT-BLOCK"); }
        if s.attrs[i].atom.is_some() && (soft || s.soft.contains(&(i as u32 + 1))) { bad.push_str(" ATOM-SOFT"); }
        if soft && i > 0 && !s.attrs.is_empty() {
            let (a, h) = (&s.attrs[i], &s.attrs[i - 1]);
            if (a.style, a.align, a.indent, a.first_line_indent) != (h.style, h.align, h.indent, h.first_line_indent) { bad.push_str(" SOFT-ATTRS"); }
        }
    }
    if s.soft.contains(&0) { bad.push_str(" SOFT0"); }
    bad
}

/// The change report and the history of an edit.
fn hist(base: &TextEdit, x: &mut TextEdit, attributed: bool) -> String {
    let mut bad = String::new();
    let (before, after) = (st(base), st(x));
    let ch = x.take_changes();
    if attributed {
        let ids: Vec<u64> = after.attrs.iter().map(|p| p.id).collect();
        let set: BTreeSet<u64> = ids.iter().copied().collect();
        if set.len() != ids.len() || set.contains(&0) { bad.push_str(" IDS"); }
        let old_atoms: BTreeSet<u64> = before.attrs.iter().filter(|p| p.atom.is_some()).map(|p| p.id).collect();
        if after.attrs.iter().any(|p| p.atom.is_none() && old_atoms.contains(&p.id)) { bad.push_str(" ATOM-ID-ON-TEXT"); }
    }
    let mut un = x.clone();
    un.undo();
    if st(&un) != before { bad.push_str(" UNDO"); }
    let mut re = un.clone();
    re.redo();
    if st(&re) != after { bad.push_str(" REDO"); }
    if after != before && ch.is_empty() { bad.push_str(" NOCHANGE-REPORT"); }
    if after == before && !ch.is_empty() { bad.push_str(" SPURIOUS-REPORT"); }
    bad
}

fn key(k: Key, shift: bool) -> KeyEvent { KeyEvent { key: k, mods: Mods { shift, ..Mods::default() } } }

fn id(i: u64) -> ParagraphAttrs { ParagraphAttrs::default().id(i) }

#[test]
fn every_edit_over_every_selection_of_documents_with_soft_lines_keeps_the_invariant() {
    let (p, c, q, u) = (Block::Paragraph, Block::Code, Block::Quote, Block::Bullet(0));
    let bold = |s, e| MarkRange { start: s, end: e, mark: Mark::Bold };
    type Fixture = (&'static str, Vec<Block>, Vec<usize>, Vec<u32>, Vec<MarkRange>);
    let fixtures: Vec<Fixture> = vec![
        ("ab\ncd\nef", vec![p, p, p], vec![], vec![1], vec![]),
        ("ab\ncd\nef", vec![p, p, p], vec![], vec![2], vec![]),
        ("ab\ncd\nef", vec![p, p, p], vec![], vec![1, 2], vec![bold(3, 5)]),
        ("ab\ncd\nef", vec![u, u, p], vec![], vec![1], vec![]),
        ("ab\ncd\nef", vec![p, q, q], vec![], vec![2], vec![]),
        ("ab\ncd\nef\ngh", vec![p, p, u, u], vec![], vec![1, 3], vec![]),
        ("ab\ncd\nef\ngh", vec![q, p, p, p], vec![], vec![2], vec![bold(6, 8)]),
        ("ab\ncd\nef\ngh", vec![p, p, p, q], vec![], vec![1, 2], vec![]),
        ("ab\n\nef", vec![p, p, p], vec![], vec![1], vec![]),
        ("ab\ncd\n", vec![p, p, p], vec![], vec![2], vec![]),
        ("ab\n\n", vec![u, u, u], vec![], vec![1, 2], vec![]),
        ("ab\ncd\nT\nef", vec![p, p, p, p], vec![2], vec![1], vec![]),
        ("T\nab\ncd", vec![p, q, q], vec![0], vec![2], vec![]),
        ("ab\nT\ncd\nef", vec![p, p, p, p], vec![1], vec![3], vec![]),
        ("ab\ncd\nef\ngh", vec![p, p, c, c], vec![], vec![1], vec![]),
        ("ab\ncd\nef\ngh", vec![c, p, p, p], vec![], vec![2], vec![]),
        ("ab\ncd\nef\ngh", vec![c, c, p, p], vec![], vec![3], vec![]),
    ];
    let ops: Vec<(&str, &str, &str)> = vec![
        ("bksp", "", ""), ("del", "", ""), ("ret", "", ""), ("sret", "", ""),
        ("type", "", "x"), ("insert", "", "x"), ("paste", "", "x"), ("html", "<p>x</p>", "x"), ("html", "<p><b>x</b></p>", "x"),
        ("insert", "", "x\ny"), ("paste", "", "x\ny"), ("html", "<p>x</p><p>y</p>", "x\ny"), ("html", "<p><i>x</i></p><p>y</p>", "x\ny"), ("frag", "<p><i>x</i></p><p>y</p>", "x\ny"),
        ("insert", "", "x\n"), ("paste", "", "x\n"), ("html", "<p>x</p><p></p>", "x\n"), ("html", "<p><i>x</i></p><p></p>", "x\n"),
        ("insert", "", "\n"), ("paste", "", "\n"),
        ("html", "<p>x<br>y</p>", "x\ny"), ("html", "<p><b>x</b><br>y</p>", "x\ny"), ("frag", "<p>x<br>y</p>", "x\ny"),
        ("html", "<p>x<br>y</p><p>z</p>", "x\ny\nz"), ("html", "<p>x<br>y</p><p></p>", "x\ny\n"), ("html", "<p>x<br><br></p>", "x\n"),
        ("html", "<ul><li>x<br>y</li></ul>", "x\ny"), ("html", "<blockquote>x<br>y</blockquote><p>z</p>", "x\ny\nz"), ("html", "<ul><li>x</li><li>y</li></ul>", "x\ny"),
        ("html-own", "", ""), ("frag-own", "", ""), ("cut-own", "", ""),
    ];
    let mut fails: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cases = 0usize;
    for attributed in [true, false] {
        for (text, blocks, atoms, soft, marks) in &fixtures {
            if !attributed && !atoms.is_empty() { continue; }
            let f = doc(text, blocks.clone(), atoms, soft, marks.clone(), attributed);
            let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
            let hard_starts: Vec<usize> = starts.iter().enumerate().filter(|(i, _)| !soft.contains(&(*i as u32))).map(|(_, s)| *s).collect();
            for a in 0..=text.len() { for b in a..=text.len() {
                let mut group: Vec<(String, String, St)> = Vec::new();
                for (op, html, plain) in &ops {
                    let mut x = f.clone();
                    x.set_selection(a, b);
                    let mut base = f.clone();
                    base.set_selection(a, b);
                    let mut flags: Vec<String> = Vec::new();
                    match *op {
                        "bksp" => { x.key(&key(Key::Backspace, false), None); }
                        "del" => { x.key(&key(Key::Delete, false), None); }
                        "ret" => { x.key(&key(Key::Enter, false), None); }
                        "sret" => { x.key(&key(Key::Enter, true), None); }
                        "type" => { x.key(&key(Key::Text((*plain).into()), false), None); }
                        "insert" => { x.insert(plain); }
                        "paste" => { x.paste(plain); }
                        "html" => { x.paste_html(html, plain); }
                        "frag" => { let fr = x.html_fragment(html); x.paste_fragment(fr, plain); }
                        "html-own" | "frag-own" => {
                            if a == b { continue; }
                            let h = x.selected_html();
                            let pl = text[a..b].to_string();
                            if *op == "html-own" { x.paste_html(&h, &pl); } else { let fr = x.html_fragment(&h); x.paste_fragment(fr, &pl); }
                            let sel_atom = (0..starts.len()).any(|i| atoms.contains(&i) && { let s = starts[i]; let e = text[s..].find('\n').map_or(text.len(), |n| s + n); s < b && e > a || (s >= a && e <= b) });
                            // Code decides by position, and HTML cannot say a code line's alignment: a copy pasted over code, or
                            // into the line before or after it, is not itself (the edge probe leaves code out the same way).
                            let (la, lb) = (text[..a].matches('\n').count(), text[..b].matches('\n').count());
                            let lossy_code = (la.saturating_sub(1)..=(lb + 1).min(blocks.len() - 1)).any(|l| blocks[l] == c);
                            if !(sel_atom && *op == "html-own") && !lossy_code {
                                // (A paragraph's identity is its head's id: a soft line's id is its own and no one's.)
                                let hard_ids = |e: &TextEdit| (0..e.text.split('\n').count()).filter(|i| !e.soft_breaks().contains(&(*i as u32))).map(|i| e.paragraph(i).id).collect::<Vec<_>>();
                                if noid(&x) != noid(&f) { flags.push("OWN-CHANGED".into()); } else if hard_ids(&x) != hard_ids(&f) { flags.push("OWN-IDS".into()); }
                            }
                            flags.push(format!("[{h}]"));
                        }
                        _ => {
                            if a == b { continue; }
                            let h = x.selected_html();
                            let pl = text[a..b].to_string();
                            x.key(&key(Key::Backspace, false), None);
                            x.take_changes();
                            base = x.clone();
                            let fr = x.html_fragment(&h);
                            x.paste_fragment(fr, &pl);
                            let (la, lb) = (text[..a].matches('\n').count(), text[..b].matches('\n').count());
                            let whole = hard_starts.contains(&a) && (hard_starts.contains(&b) || b == text.len()) && la < lb.max(if b == text.len() { lb + 1 } else { lb });
                            let has_atom = (la..=lb).any(|i| atoms.contains(&i));
                            let near_code = (la.saturating_sub(1)..=(lb + 1).min(blocks.len() - 1)).any(|i| blocks[i] == c);
                            if noid(&x) != noid(&f) && whole && !has_atom && !near_code { flags.push("NOT-RESTORED-WHOLE".into()); }
                            flags.push(format!("[{h}] cut={}", show(&base)));
                        }
                    }
                    cases += 1;
                    let mut bad = inv(&x);
                    bad.push_str(&hist(&base, &mut x, attributed));
                    for fl in flags.iter().filter(|f| !f.starts_with('[')) { bad.push(' '); bad.push_str(fl); }
                    if matches!(*op, "type" | "insert" | "paste" | "html" | "frag") { group.push((plain.to_string(), format!("{op} {html}"), nomarks(&x))); }
                    if !bad.is_empty() {
                        let note = flags.iter().find(|f| f.starts_with('[')).cloned().unwrap_or_default();
                        for w in bad.split_whitespace() {
                            fails.entry(w.to_string()).or_default().push(format!("attributed {attributed} {text:?}{}@{atoms:?}s{soft:?} {a}..{b} {op} {html:?} {plain:?}\n   from {}\n   => {} {note}", blocks.iter().map(|b| blk(*b)).collect::<String>(), show(&f), show(&x)));
                        }
                    }
                }
                // Pastes of one plain text through the paths agree (fragments with soft lines excluded).
                for i in 0..group.len() { for j in i + 1..group.len() {
                    let soft_or_list = |s: &str| s.contains("<br>") || s.contains("<li>");
                    if group[i].0 == group[j].0 && !soft_or_list(&group[i].1) && !soft_or_list(&group[j].1) && group[i].2 != group[j].2 {
                        fails.entry("DISAGREE".into()).or_default().push(format!("attributed {attributed} {text:?}{}@{atoms:?}s{soft:?} {a}..{b} [{}] vs [{}]", blocks.iter().map(|b| blk(*b)).collect::<String>(), group[i].1, group[j].1));
                    }
                } }
            } }
        }
    }
    let summary: Vec<String> = fails.iter().map(|(k, v)| format!("== {k}: {}\n{}", v.len(), v.iter().take(3).cloned().collect::<Vec<_>>().join("\n"))).collect();
    assert!(fails.is_empty(), "{cases} cases\n{}", summary.join("\n"));
}

/// Inserting text with a break at the start of a soft line is a mid-paragraph insertion: the head keeps its id,
/// attributes and block, and every soft line is its head's. A paste of the same text lands the same way.
#[test]
fn text_with_a_break_at_a_soft_lines_start_goes_into_the_paragraph() {
    let (p, u) = (Block::Paragraph, Block::Bullet(0));
    let f = doc("ab\ncd\nef", vec![u, u, p], &[], &[1], vec![], true);
    for how in ["insert", "paste", "html"] {
        let mut x = f.clone();
        x.set_selection(3, 3);
        match how { "insert" => x.insert("x\ny"), "paste" => { x.paste("x\ny"); } _ => { x.paste_html("<p>x</p><p>y</p>", "x\ny"); } }
        assert_eq!(x.text, "ab\nx\nycd\nef", "{how}");
        assert_eq!((x.paragraph(0).id, x.blocks()[0]), (1, u), "{how}: the head keeps its id and block: {}", show(&x));
        assert_eq!(x.soft_breaks(), &[1], "{how}: {}", show(&x));
        assert_eq!(inv(&x), "", "{how}: {}", show(&x));
        assert_eq!(x.blocks()[3], p, "{how}: ef is untouched: {}", show(&x));
    }
}

/// A fragment whose soft line lands on a kept line: the receiving paragraph is the head, the soft line mirrors it
/// with an id of its own, as typing the text with Shift+Return gives.
#[test]
fn a_fragments_soft_line_on_a_kept_paragraph_makes_it_the_head() {
    let (p, u) = (Block::Paragraph, Block::Bullet(0));
    let f = doc("ab\nef", vec![p, u], &[], &[], vec![], true);
    let mut f = f;
    f.set_paragraph_attrs(vec![id(1), id(2).align(ParagraphAlign::Center)]);
    let mut x = f.clone();
    x.set_selection(3, 3);
    x.paste_html("<p>x<br>y</p>", "x\ny");
    let mut t = f.clone();
    t.set_selection(3, 3);
    t.key(&key(Key::Text("x".into()), false), None);
    t.key(&key(Key::Enter, true), None);
    t.key(&key(Key::Text("y".into()), false), None);
    assert_eq!(noid(&x), noid(&t), "pasted {} / typed {}", show(&x), show(&t));
    assert_eq!((x.text.as_str(), x.paragraph(1).id, x.soft_breaks()), ("ab\nx\nyef", 2, &[2][..]), "{}", show(&x));
    assert_ne!(x.paragraph(2).id, 2, "the soft line has an id of its own: {}", show(&x));
    assert_eq!(inv(&x), "");
}

/// The last paragraph, with a soft line and an indent, cut and pasted back: `cd` keeps its id and indent and `ef`
/// mirrors it.
#[test]
fn a_cut_paragraph_with_a_soft_line_and_an_indent_pastes_back_whole() {
    let p = Block::Paragraph;
    let mut f = doc("ab\ncd\nef", vec![p, p, p], &[], &[2], vec![], true);
    f.set_paragraph_attrs(vec![id(1), id(2).indent(20.0, 0.0), id(3).indent(20.0, 0.0)]);
    let mut x = f.clone();
    x.set_selection(3, 8);
    let html = x.selected_html();
    x.key(&key(Key::Backspace, false), None);
    let fr = x.html_fragment(&html);
    x.paste_fragment(fr, "cd\nef");
    assert_eq!(noid(&x), noid(&f), "{} / {}", show(&x), show(&f));
    assert_eq!(x.paragraph(1).id, 2, "the paragraph keeps its id: {}", show(&x));
    let mut y = f.clone();
    y.set_selection(0, 8);
    let html = y.selected_html();
    y.key(&key(Key::Backspace, false), None);
    let fr = y.html_fragment(&html);
    y.paste_fragment(fr, "ab\ncd\nef");
    assert_eq!(noid(&y), noid(&f), "{} / {}", show(&y), show(&f));
}

/// A paste that changes only the softness of the lines it replaces is a change, and a paste of what is there is not.
#[test]
fn a_paste_over_the_selection_compares_its_soft_lines() {
    let p = Block::Paragraph;
    let plain_doc = |soft: &[u32]| {
        let mut e = TextEdit::from_parts("ab\ncd\nef", vec![], vec![p, p, p]);
        e.set_paragraph_attrs(vec![id(1), id(2), id(3)]);
        e.set_soft_breaks(soft.to_vec());
        e.take_changes();
        e
    };
    let hard = plain_doc(&[]);
    let mut x = hard.clone();
    x.set_selection(3, 8);
    x.paste_html("<p>cd<br>ef</p>", "cd\nef");
    assert_eq!(x.soft_breaks(), &[2], "{}", show(&x));
    assert!(!x.take_changes().is_empty(), "the softness changed: {}", show(&x));
    let soft = plain_doc(&[2]);
    let mut y = soft.clone();
    y.set_selection(3, 8);
    y.paste_html("<p>cd</p><p>ef</p>", "cd\nef");
    assert_eq!(y.soft_breaks(), &[] as &[u32], "{}", show(&y));
    assert!(!y.take_changes().is_empty());
    let mut z = soft.clone();
    z.set_selection(3, 8);
    z.paste_html("<p>cd<br>ef</p>", "cd\nef");
    assert_eq!(st(&z), st(&soft));
    assert!(z.take_changes().is_empty(), "nothing changed");
}

/// A Quote with soft lines joined into a paragraph by Backspace: the soft lines are the paragraph's now.
#[test]
fn soft_lines_of_a_quote_joined_into_a_paragraph_mirror_the_paragraph() {
    let (p, q) = (Block::Paragraph, Block::Quote);
    let f = doc("ab\ncd\nef", vec![p, q, q], &[], &[2], vec![], true);
    let mut x = f.clone();
    x.set_caret(3);
    x.key(&key(Key::Backspace, false), None);
    assert_eq!((x.text.as_str(), x.blocks().to_vec()), ("abcd\nef", vec![p, p]), "{}", show(&x));
    assert_eq!(inv(&x), "");
}
