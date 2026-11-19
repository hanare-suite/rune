//! What must hold after every edit of a field whose history the app owns, checked over
//! thousands of random keys, pastes, atoms and formatting commands:
//!
//! 1. The reported changes, applied in order to the text before, give the text now.
//! 2. No two paragraphs share an id, and none loses its id.
//! 3. Nothing outside the bytes the changes name is different: not a paragraph's attributes,
//!    not its block, not a mark.
//! 4. The field's own history stays empty.
//! 5. A paragraph that is not an atom never carries an id that was an atom's earlier: an app
//!    that keys an atom's content by id would take the text for the atom.
//! 6. A rich paste over selected atoms leaves the lines a plain paste of the same text does.

use std::collections::BTreeSet;

use super::*;

/// What a field holds that an edit can change.
pub(super) struct Seen {
    text: String,
    attrs: Vec<ParagraphAttrs>,
    blocks: Vec<Block>,
    marks: Vec<MarkRange>,
}

pub(super) fn seen(e: &TextEdit) -> Seen {
    Seen { text: e.text.clone(), attrs: e.paragraph_attrs(), blocks: e.blocks().to_vec(), marks: e.marks().to_vec() }
}

/// Each line's first byte and the byte of the line break that ends it.
fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut at = 0;
    text.split('\n').map(|l| { let r = (at, at + l.len()); at = r.1 + 1; r }).collect()
}

/// Every marked byte of `from..to`, counted from `from`, with its mark.
fn marked(marks: &[MarkRange], from: usize, to: usize) -> BTreeSet<(usize, String)> {
    marks.iter().flat_map(|m| (m.start.max(from)..m.end.min(to)).map(move |i| (i - from, format!("{:?}", m.mark)))).collect()
}

/// Checks `changes`, which took the field from `before` to `e`, as an app that follows the
/// field through them reads them: applied in order, each range in the text the one before it
/// left, the inserted bytes read from the text now.
pub(super) fn check(before: &Seen, e: &TextEdit, changes: &[EditChange]) -> Result<(), String> {
    let now = seen(e);
    // Each byte as the changes leave it: one of the old text's, or one a change put there.
    let mut bytes: Vec<Option<usize>> = (0..before.text.len()).map(Some).collect();
    // The changes' union: `lo..old_hi` of the old text became `lo..new_hi` of the new.
    let mut span = None::<(usize, usize, usize)>;
    for c in changes {
        if c.range.end > bytes.len() || c.range.start > c.range.end {
            return Err(format!("{c:?} is outside a text of {} bytes", bytes.len()));
        }
        if matches!(c.kind, EditChangeKind::Marks | EditChangeKind::Paragraphs) {
            if c.range.len() != c.len { return Err(format!("{c:?} names no text, yet changes its length")); }
        } else {
            bytes.splice(c.range.clone(), std::iter::repeat_n(None, c.len));
        }
        let (lo, mut old_hi, mut new_hi) = span.unwrap_or((c.range.start, c.range.start, c.range.start));
        if c.range.end > new_hi {
            old_hi += c.range.end - new_hi;
            new_hi = c.range.end;
        }
        let lo_now = lo.min(c.range.start);
        // A change that starts before the union widens it backwards in both texts.
        span = Some((lo_now, old_hi, new_hi + c.len - c.range.len()));
    }
    if bytes.len() != now.text.len() {
        return Err(format!("the changes give {} bytes, the text has {}", bytes.len(), now.text.len()));
    }
    for (at, from) in bytes.iter().enumerate() {
        if let Some(from) = *from && before.text.as_bytes()[from] != now.text.as_bytes()[at] {
            return Err(format!("byte {at} should be the old text's byte {from}"));
        }
    }

    let lines = now.text.split('\n').count();
    if (now.attrs.len(), now.blocks.len()) != (lines, lines) {
        return Err(format!("{} attributes and {} blocks for {lines} lines", now.attrs.len(), now.blocks.len()));
    }
    let mut ids: Vec<u64> = now.attrs.iter().map(|a| a.id).collect();
    ids.sort_unstable();
    if ids.contains(&0) || ids.windows(2).any(|w| w[0] == w[1]) {
        return Err(format!("ids {:?}", now.attrs.iter().map(|a| a.id).collect::<Vec<_>>()));
    }
    if e.history_len() != (0, 0) {
        return Err(format!("the field recorded {:?}", e.history_len()));
    }

    let (lo, old_hi, new_hi) = span.unwrap_or((before.text.len() + 1, before.text.len() + 1, now.text.len() + 1));
    let (old_lines, new_lines) = (line_ranges(&before.text), line_ranges(&now.text));
    // Paragraphs that end before the first changed byte, and ones that start after the last.
    let head = old_lines.iter().take_while(|l| l.1 < lo).count();
    let tail = old_lines.iter().rev().take_while(|l| l.0 > old_hi).count();
    if head + tail > new_lines.len() {
        return Err(format!("{head} paragraphs before the change and {tail} after it, but only {} now", new_lines.len()));
    }
    for i in 0..head {
        if (before.attrs[i], before.blocks[i]) != (now.attrs[i], now.blocks[i]) {
            return Err(format!("paragraph {i}, before the change, went from {:?} {:?} to {:?} {:?}", before.attrs[i], before.blocks[i], now.attrs[i], now.blocks[i]));
        }
    }
    for k in 1..=tail {
        let (i, j) = (old_lines.len() - k, new_lines.len() - k);
        if (before.attrs[i], before.blocks[i]) != (now.attrs[j], now.blocks[j]) {
            return Err(format!("paragraph {i}, after the change, went from {:?} {:?} to {:?} {:?} (now {j})", before.attrs[i], before.blocks[i], now.attrs[j], now.blocks[j]));
        }
    }
    let (lo, old_hi, new_hi) = (lo.min(before.text.len()), old_hi.min(before.text.len()), new_hi.min(now.text.len()));
    if marked(&before.marks, 0, lo) != marked(&now.marks, 0, lo) {
        return Err(format!("marks before byte {lo} went from {:?} to {:?}", before.marks, now.marks));
    }
    if marked(&before.marks, old_hi, before.text.len()) != marked(&now.marks, new_hi, now.text.len()) {
        return Err(format!("marks after byte {old_hi} (now {new_hi}) went from {:?} to {:?}", before.marks, now.marks));
    }
    Ok(())
}

/// Checks that no paragraph of text carries an id in `atoms`, every id an atom had so far,
/// and adds the ids of the atoms `e` holds now.
pub(super) fn atom_ids_stay_atoms(atoms: &mut BTreeSet<u64>, e: &TextEdit) -> Result<(), String> {
    let attrs = e.paragraph_attrs();
    if let Some(a) = attrs.iter().find(|a| a.atom.is_none() && atoms.contains(&a.id)) {
        return Err(format!("id {} was an atom's and is now on text: {:?}", a.id, attrs.iter().map(|a| (a.id, a.atom.is_some())).collect::<Vec<_>>()));
    }
    atoms.extend(attrs.iter().filter(|a| a.atom.is_some()).map(|a| a.id));
    Ok(())
}

/// Whether the selection is one or more atoms whole: from the start of one to the end of one.
fn selects_atoms(e: &TextEdit) -> bool {
    let (a, b) = e.range();
    a < b && e.atom_at(a).is_some_and(|(start, _)| start == a) && e.atom_at(b).is_some_and(|(_, end)| end == b)
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
        of[self.below(of.len())]
    }

    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

/// Paragraphs, bullets and three atoms, two of them next to each other, with ids 1 to 8.
fn document() -> TextEdit {
    let mut e = TextEdit::from_parts(
        "Eins zwei\nTabelle\n\nBild\nGrafik\ndrei vier fünf\nPunkt\nEnde",
        vec![MarkRange::new(0..4, Mark::Bold), MarkRange::new(37..41, Mark::Italic)],
        vec![Block::Paragraph, Block::Paragraph, Block::Paragraph, Block::Paragraph, Block::Paragraph, Block::Paragraph, Block::Bullet(0), Block::Paragraph],
    );
    let d = ParagraphAttrs::default();
    e.set_paragraph_attrs(vec![
        d.id(1).style(1), d.id(2).atom(40.0), d.id(3), d.id(4).atom(30.0), d.id(5).atom(20.0),
        d.id(6).align(ParagraphAlign::Justified).indent(12.0, 6.0), d.id(7), d.id(8).page_break_before(true),
    ]);
    e.set_paragraph_ids(9);
    e.set_html_styles([HtmlStyle { style: 1, tag: "h1", class: None }, HtmlStyle { style: 2, tag: "p", class: Some("title") }]);
    e.set_history(false);
    e.take_changes();
    e
}

/// A byte at the start or the end of a paragraph: where atoms meet their neighbours.
fn edge(e: &TextEdit, rng: &mut Rng) -> usize {
    let lines = line_ranges(&e.text);
    let line = lines[rng.below(lines.len())];
    if rng.one_in(2) { line.0 } else { line.1 }
}

/// One random edit, and what it was.
fn step(e: &mut TextEdit, rng: &mut Rng) -> String {
    let len = e.text.len();
    let key = |e: &mut TextEdit, key: Key, mods: Mods| {
        let ev = KeyEvent { key, mods };
        e.key(&ev, None);
        format!("{ev:?}")
    };
    let none = Mods::default();
    // A long text shrinks, so the rounds stay near the atoms.
    if len > 320 {
        let (a, b) = (rng.below(len + 1), rng.below(len + 1));
        e.set_selection(a, b);
        return format!("select {a}..{b}, {}", key(e, Key::Backspace, none));
    }
    match rng.below(26) {
        0 => { let at = rng.below(len + 1); e.set_caret(at); format!("caret {at}") }
        1 | 2 => { let at = edge(e, rng); e.set_caret(at); format!("caret {at}") }
        3 => { let (a, b) = (rng.below(len + 1), rng.below(len + 1)); e.set_selection(a, b); format!("select {a}..{b}") }
        4 => { let (a, b) = (edge(e, rng), edge(e, rng)); e.set_selection(a, b); format!("select {a}..{b}") }
        5 | 6 => {
            let k = [Key::Left, Key::Right, Key::Up, Key::Down][rng.below(4)].clone();
            key(e, k, Mods { shift: rng.one_in(2), alt: rng.one_in(5), ..none })
        }
        7..=10 => { let s = rng.pick(&["a", "ä", "x y", " ", "Q"]); key(e, Key::Text(s.into()), none) }
        11 => key(e, Key::Enter, none),
        12 => key(e, Key::Enter, Mods { shift: true, ..none }),
        13..=15 => key(e, Key::Backspace, Mods { alt: rng.one_in(6), cmd: rng.one_in(8), ..none }),
        16 | 17 => key(e, Key::Delete, Mods { alt: rng.one_in(6), ..none }),
        18 => { let s = rng.pick(&["eingefügt", "zwei\nZeilen", "\nvorn", "hinten\n", "a\n\nb", "\n"]); e.paste(s); format!("paste {s:?}") }
        19 => {
            let html = rng.pick(&[
                "<b>fett</b> und <i>schräg</i>",
                "<p><b>X</b>y</p><ul><li>L1</li><li>L2</li></ul>",
                "<ol><li>eins</li><li>zwei</li></ol><p>danach</p>",
                "<blockquote>Zitat</blockquote>",
                "<p>erste</p><p><u>zweite</u></p>",
                "<h1 style=\"text-align:center\">Titel</h1><p style=\"text-align:right\"><span style=\"color:#c00000;font-size:14pt\">rot</span></p>",
                "<p class=\"title\">Kopf</p>",
                "<p align=\"center\">Mitte <sup>hoch</sup></p><p>unten</p>",
            ]);
            let twin = selects_atoms(e).then(|| {
                let mut twin = e.clone();
                twin.paste(&html::parse_html(html, e.html_styles()).rich().text);
                twin
            });
            e.paste_html(html, "");
            if let Some(twin) = twin {
                assert_eq!(e.text, twin.text, "a rich paste of {html:?} over selected atoms leaves other lines than a plain paste");
            }
            format!("paste_html {html:?}")
        }
        20 => { e.insert_atom("Block", 24.0); "insert_atom".into() }
        21 => {
            let mark = [Mark::Bold, Mark::Italic, Mark::Baseline(Baseline::Sub), Mark::family("Carlito")][rng.below(4)].clone();
            e.toggle_mark(mark.clone());
            format!("toggle_mark {mark:?}")
        }
        22 => {
            let align = [ParagraphAlign::Left, ParagraphAlign::Center, ParagraphAlign::Right][rng.below(3)];
            let indent = rng.below(3) as f32 * 12.0;
            e.format_paragraphs(|a| (a.align, a.indent) = (align, indent));
            format!("format_paragraphs {align:?} {indent}")
        }
        23 => {
            let block = [Block::Bullet(0), Block::Numbered(0), Block::Quote][rng.below(3)];
            e.toggle_block(block);
            format!("toggle_block {block:?}")
        }
        24 => key(e, Key::Tab, Mods { shift: rng.one_in(2), ..none }),
        _ => key(e, Key::Command(if rng.one_in(6) { 'a' } else { 'x' }), Mods { cmd: true, ..none }),
    }
}

const SEEDS: u64 = 40;
const ROUNDS: usize = 400;

#[test]
fn random_edits_report_what_they_changed_and_keep_ids_unique() {
    for seed in 1..=SEEDS {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let mut e = document();
        let mut atoms = BTreeSet::new();
        for round in 0..ROUNDS {
            let before = seen(&e);
            let (anchor, focus) = (e.anchor, e.focus);
            let what = step(&mut e, &mut rng);
            let changes = e.take_changes();
            if let Err(why) = check(&before, &e, &changes).and_then(|()| atom_ids_stay_atoms(&mut atoms, &e)) {
                panic!(
                    "seed {seed}, round {round}: {why}\n after {what} with the selection {anchor}..{focus}\n text before {:?}\n text now    {:?}\n ids before {:?}\n changes {changes:?}",
                    before.text, e.text, before.attrs.iter().map(|a| (a.id, a.atom.is_some())).collect::<Vec<_>>(),
                );
            }
        }
    }
}

/// Changes of several edits taken at once still apply in order.
#[test]
fn changes_left_untaken_across_edits_still_apply_in_order() {
    for seed in 1..=SEEDS {
        let mut rng = Rng(seed.wrapping_mul(0xd1b5_4a32_d192_ed03));
        let mut e = document();
        let mut atoms = BTreeSet::new();
        for round in 0..ROUNDS / 4 {
            let before = seen(&e);
            let mut ids = Ok(());
            let what: Vec<String> = (0..1 + rng.below(4)).map(|_| {
                let what = step(&mut e, &mut rng);
                ids = std::mem::replace(&mut ids, Ok(())).and_then(|()| atom_ids_stay_atoms(&mut atoms, &e));
                what
            }).collect();
            let changes = e.take_changes();
            if let Err(why) = check(&before, &e, &changes).and(ids) {
                panic!("seed {seed}, round {round}: {why}\n after {what:?}\n text before {:?}\n text now    {:?}\n changes {changes:?}", before.text, e.text);
            }
        }
    }
}
