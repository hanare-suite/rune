//! Every path an edit can take (typed text, `insert`, `paste`, `paste_html` with and without a
//! mark, `paste_fragment`) over every selection of ten documents and six inputs gives one
//! result, a sound change report, unique ids, no atom id on text, and exact undo and redo.

use super::fuzz_tests::{check, seen};
use super::*;

type Row = (String, u64, u16, ParagraphAlign, (f32, f32), Option<f32>, Block);
const NEW: u64 = u64::MAX;

fn rows(e: &TextEdit, orig: &[u64]) -> Vec<Row> {
    let blocks = e.blocks();
    e.text.split('\n').enumerate().map(|(i, t)| {
        let a = e.paragraph(i);
        let id = if orig.contains(&a.id) || a.id == 0 { a.id } else { NEW };
        (t.to_string(), id, a.style, a.align, (a.indent, a.first_line_indent), a.atom, blocks[i])
    }).collect()
}

fn mk(text: &str, blocks: Vec<Block>, attrs: Vec<ParagraphAttrs>) -> TextEdit {
    let mut e = TextEdit::from_parts(text, vec![], blocks);
    if !attrs.is_empty() { e.set_paragraph_attrs(attrs); e.set_paragraph_ids(10); }
    e.set_history(false);
    e.take_changes();
    e
}

fn docs() -> Vec<(&'static str, TextEdit)> {
    let d = ParagraphAttrs::default();
    let b = Block::Bullet(0);
    let p = Block::Paragraph;
    vec![
        ("list", mk("top\nXYZ\nend", vec![p, b, p], vec![d.id(1), d.id(2).style(2).indent(10.0, 0.0), d.id(3).style(3)])),
        ("listend", mk("top\nXYZ\nend", vec![p, p, b], vec![d.id(1), d.id(2).style(2).align(ParagraphAlign::Right), d.id(3).style(3).indent(5.0, 2.0)])),
        ("atom", mk("AAA\nTab\nend", vec![p, p, b], vec![d.id(1), d.id(2).style(2).atom(40.0), d.id(3).style(3)])),
        ("atomfirst", mk("Tab\nXYZ\nend", vec![p, b, p], vec![d.id(1).style(2).atom(40.0), d.id(2).style(4), d.id(3).style(3)])),
        ("atomlast", mk("AAA\nXYZ\nTab", vec![b, p, p], vec![d.id(1).style(5), d.id(2).style(4), d.id(3).style(2).atom(40.0)])),
        ("twoatoms", mk("AAA\nTa\nTb\nend", vec![p, p, p, p], vec![d.id(1), d.id(2).atom(30.0), d.id(3).atom(30.0), d.id(4).style(3)])),
        ("emptyid", mk("top\n\nend", vec![p, p, p], vec![d.id(1), d.id(2), d.id(3).style(3)])),
        ("emptystyled", mk("top\n\nend", vec![p, b, p], vec![d.id(1), d.id(2).style(2), d.id(3).style(3)])),
        ("noattrs", mk("top\nXYZ\nend", vec![p, b, p], vec![])),
        ("noattrs2", mk("top\nXYZ\nend", vec![b, p, Block::Bullet(1)], vec![])),
    ]
}

type Path = (&'static str, fn(&mut TextEdit, &str));

fn html_of(s: &str, bold: bool) -> String {
    s.split('\n').map(|l| if bold && !l.is_empty() { format!("<p><b>{l}</b></p>") } else { format!("<p>{l}</p>") }).collect()
}

const PATHS: [Path; 6] = [
    ("typed", |e, s| { e.key(&KeyEvent { key: Key::Text(s.into()), mods: Mods::default() }, None); }),
    ("insert", |e, s| e.insert(s)),
    ("paste", |e, s| { e.paste(s); }),
    ("html", |e, s| { e.paste_html(&html_of(s, false), s); }),
    ("htmlb", |e, s| { e.paste_html(&html_of(s, true), s); }),
    ("frag", |e, s| { e.paste_fragment(TextEdit::from_parts(s, vec![], vec![]), ""); }),
];

fn run(e: &TextEdit, sel: (usize, usize), s: &str, path: &Path) -> Result<(Vec<Row>, Vec<u64>), String> {
    let orig: Vec<u64> = e.paragraph_attrs().iter().map(|a| a.id).collect();
    let mut x = e.clone();
    x.set_selection(sel.0, sel.1);
    let before = seen(&x);
    (path.1)(&mut x, s);
    let changes = x.take_changes();
    if let Err(w) = check(&before, &x, &changes) { if !(w.starts_with("ids [0") && e.paragraph_attrs().iter().all(|a| a.id == 0)) { return Err(format!("change report: {w} changes {changes:?}")); } }
    let ids: Vec<u64> = x.paragraph_attrs().iter().map(|a| a.id).filter(|&i| i != 0).collect();
    let mut u = ids.clone(); u.sort(); u.dedup();
    if u.len() != ids.len() { return Err(format!("duplicate ids {:?}", x.paragraph_attrs())); }
    // An atom's id never names text.
    for (i, a) in x.paragraph_attrs().iter().enumerate() {
        if a.atom.is_none() && e.paragraph_attrs().iter().any(|o| o.atom.is_some() && o.id == a.id && a.id != 0) {
            return Err(format!("text line {i} carries an atom's id"));
        }
    }
    // undo/redo
    let mut y = e.clone();
    y.set_history(true);
    y.set_selection(sel.0, sel.1);
    let st = (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec());
    (path.1)(&mut y, s);
    let done = (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec());
    y.undo();
    if (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec()) != st { return Err("undo".into()); }
    y.redo();
    if (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec()) != done { return Err("redo".into()); }
    Ok((rows(&x, &orig), orig))
}

#[test]
fn every_path_agrees_over_every_selection_of_every_document() {
    let inputs = ["x", "\n", "A\n", "A\nB", "A\nB\nC", "A\nB\nC\nD\n"];
    let mut fails = Vec::new();
    for (doc, e) in docs() {
        let n = e.text.len();
        for a in 0..=n { for b in a..=n {
            if !e.text.is_char_boundary(a) || !e.text.is_char_boundary(b) { continue; }
            for s in inputs {
                let results: Vec<_> = PATHS.iter().map(|p| (p.0, run(&e, (a, b), s, p))).collect();
                for (name, r) in &results {
                    if let Err(w) = r { fails.push(format!("{doc} {a}..{b} {s:?} {name}: {w}")); }
                }
                let oks: Vec<_> = results.iter().filter_map(|(n, r)| r.as_ref().ok().map(|r| (*n, &r.0))).collect();
                let mut groups: Vec<(Vec<&str>, &Vec<Row>)> = Vec::new();
                for (n, r) in &oks {
                    match groups.iter_mut().find(|g| g.1 == *r) { Some(g) => g.0.push(n), None => groups.push((vec![n], r)) }
                }
                if groups.len() > 1 {
                    let mut msg = format!("{doc} {a}..{b} {:?} {s:?}", &e.text[a..b]);
                    for (ns, r) in &groups { msg += &format!("\n    {ns:?}: {:?}", r.iter().map(|r| format!("{}#{}{}s{}i{}{}{}", r.0, if r.1==NEW {"N".into()} else {r.1.to_string()}, if r.5.is_some(){"@"}else{""}, r.2, r.4.0, match r.6 {Block::Paragraph=>"".to_string(), b=>format!("{b:?}")}, if r.3!=ParagraphAlign::Left{format!("{:?}",r.3)}else{"".into()})).collect::<Vec<_>>()); }
                    fails.push(msg);
                }
            }
        }}
    }
    assert!(fails.is_empty(), "{} failures:\n{}", fails.len(), fails.join("\n"));
}

#[test]
fn a_pasted_fragment_with_atoms_keeps_atoms_whole_and_ids_unique() {
    let d = ParagraphAttrs::default();
    let frags: Vec<(&str, Vec<ParagraphAttrs>)> = vec![
        ("Q", vec![d.id(50).atom(20.0)]),
        ("A\nQ", vec![d.id(51).style(7), d.id(50).atom(20.0)]),
        ("Q\nA", vec![d.id(50).atom(20.0), d.id(51).style(7)]),
        ("Q\n", vec![d.id(50).atom(20.0), d.id(51)]),
        ("A\nQ\nB", vec![d.id(51), d.id(50).atom(20.0), d.id(52)]),
        ("A\nB\n", vec![d.id(51).style(7).align(ParagraphAlign::Center), d.id(52).style(8), d.id(53)]),
        ("A\nB", vec![d.id(51).style(7).align(ParagraphAlign::Center), d.id(52).style(8)]),
        ("A", vec![d.id(51).style(7).align(ParagraphAlign::Center)]),
    ];
    let mut fails = Vec::new();
    for (doc, e) in docs() {
        if doc.starts_with("noattrs") { continue; }
        let n = e.text.len();
        let labels: Vec<String> = e.text.split('\n').enumerate().filter(|(i, _)| e.paragraph(*i).atom.is_some()).map(|(_, t)| t.to_string()).chain(["Q".to_string()]).collect();
        for a in 0..=n { for b in a..=n {
            for (ft, fa) in &frags {
                let mut f = TextEdit::from_parts(*ft, vec![], vec![]);
                f.set_paragraph_attrs(fa.clone());
                let p: Path = ("frag", |_, _| {});
                let _ = p;
                let orig: Vec<u64> = e.paragraph_attrs().iter().map(|a| a.id).collect();
                let mut x = e.clone();
                x.set_selection(a, b);
                let before = seen(&x);
                x.paste_fragment(f.clone(), "");
                let ch = x.take_changes();
                if let Err(w) = check(&before, &x, &ch) { fails.push(format!("{doc} {a}..{b} {ft:?}: report {w}")); }
                let ids: Vec<u64> = x.paragraph_attrs().iter().map(|a| a.id).collect();
                let mut u = ids.clone(); u.sort(); u.dedup();
                if u.len() != ids.len() { fails.push(format!("{doc} {a}..{b} {ft:?}: dup ids {ids:?}")); }
                for (i, t) in x.text.split('\n').enumerate() {
                    let at = x.paragraph(i);
                    if at.atom.is_some() && !labels.iter().any(|l| l == t) { fails.push(format!("{doc} {a}..{b} {ft:?}: atom line {i} holds {t:?} {:?}", rows(&x, &orig))); }
                    if at.atom.is_none() && at.id != 0 && e.paragraph_attrs().iter().any(|o| o.atom.is_some() && o.id == at.id) { fails.push(format!("{doc} {a}..{b} {ft:?}: text line {i} has atom id")); }
                    if at.atom.is_some() && at.id == 50 { fails.push(format!("{doc} {a}..{b} {ft:?}: pasted atom kept fragment id")); }
                }
                let mut y = e.clone();
                y.set_history(true);
                y.set_selection(a, b);
                let st = (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec());
                y.paste_fragment(f, "");
                let done = (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec());
                y.undo();
                if (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec()) != st { fails.push(format!("{doc} {a}..{b} {ft:?}: undo")); }
                y.redo();
                if (y.text.clone(), y.paragraph_attrs().to_vec(), y.blocks().to_vec()) != done { fails.push(format!("{doc} {a}..{b} {ft:?}: redo")); }
            }
        }}
    }
    assert!(fails.is_empty(), "{} failures:\n{}", fails.len(), fails.join("\n"));
}
