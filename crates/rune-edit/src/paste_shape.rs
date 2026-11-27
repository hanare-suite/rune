//! What a paste over the selection joins, lands in, and holds: the shape the fragment paste goes by.

use super::paragraph::{Edge, new_lines};
use super::*;

impl TextEdit {
    /// The line a paste over the selection lands in, the one what is left of the selection's lines
    /// joins: the later line when the selection takes the earlier one whole, from its start into a
    /// later line (an atom, or paragraphs up to a line's start), the first otherwise.
    pub(super) fn landing_line(&self) -> usize {
        let (a, b) = self.range();
        let (la, lb) = (self.line_of_byte(a), self.line_of_byte(b));
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        if la < lb && a == ra.start && (b == rb.start || self.paragraph(la).atom.is_some()) { lb } else { la }
    }

    /// Which lines of `frag` land on a line of code, as the paste puts them: one that joins a line joins
    /// the line before or after it (its first the line the text goes into, its last the one that
    /// follows), and one that is new is code where the line it splits from is, as typed text is.
    /// An atom's label is never code.
    pub(super) fn code_lines(&self, frag: &TextEdit) -> Vec<bool> {
        let (a, b) = self.range();
        let f = self.fence(a, b, &frag.text);
        let p = self.edit_plan((f.a, f.b), f.text.as_deref().unwrap_or(&frag.text), None, false);
        let new = new_lines(p.added_lines, p.kept, p.edge, p.line_a == p.line_b);
        let code = |l: usize| self.blocks.get(l) == Some(&Block::Code) && self.paragraph(l).atom.is_none();
        let kept = if p.take_b { p.line_b } else { p.line_a };
        let last_line = if p.edge == Edge::Tail || (p.kept.head && p.line_a < p.line_b) { p.line_b } else { kept };
        let n = frag.text.split('\n').count();
        (0..n).map(|k| {
            if new.get(f.lead + k).copied().unwrap_or(false) { code(kept) } else if k + 1 == n && n > 1 { code(last_line) } else { code(kept) }
        }).collect()
    }

    /// `frag` with the text of the lines `code` flags taken from `plain`, line for line, where `plain` has
    /// the same lines: a code line keeps the whitespace the HTML collapsed. Marks of those lines go.
    pub(super) fn with_plain_code_lines(frag: &TextEdit, plain: &str, code: &[bool]) -> Option<TextEdit> {
        let (old, pl): (Vec<&str>, Vec<&str>) = (frag.text.split('\n').collect(), plain.split('\n').collect());
        if old.len() != pl.len() || old.len() != code.len() || !code.iter().any(|&c| c) { return None; }
        // Where each byte of the fragment's text goes: a plain line's own bytes are no one's.
        let mut map: Vec<Option<usize>> = vec![None; frag.text.len() + 1];
        let (mut text, mut at) = (String::new(), 0usize);
        for (k, line) in old.iter().enumerate() {
            let from = if code[k] { pl[k] } else { line };
            if !code[k] {
                for i in 0..line.len() { map[at + i] = Some(text.len() + i); }
            }
            // the break after a line stays between two lines that keep their text
            if k + 1 < old.len() && !code[k] && !code[k + 1] { map[at + line.len()] = Some(text.len() + from.len()); }
            text.push_str(from);
            if k + 1 < old.len() { text.push('\n'); }
            at += line.len() + 1;
        }
        let mut out = frag.clone();
        out.text = text;
        let mut marks: Vec<MarkRange> = Vec::new();
        for m in &frag.marks {
            let mut run: Option<(usize, usize)> = None;
            let mut pieces: Vec<(usize, usize)> = Vec::new();
            for i in m.start..m.end {
                match map.get(i).copied().flatten() {
                    Some(n) if run.is_some_and(|(_, e)| e == n) => run = run.map(|(s, _)| (s, n + 1)),
                    Some(n) => { pieces.extend(run); run = Some((n, n + 1)); }
                    None => { pieces.extend(run.take()); }
                }
            }
            pieces.extend(run);
            // A pill is whole or gone.
            if m.mark.is_pill() && !(pieces.len() == 1 && pieces[0].1 - pieces[0].0 == m.end - m.start) { continue; }
            marks.extend(pieces.into_iter().map(|(s, e)| MarkRange { start: s, end: e, mark: m.mark.clone() }));
        }
        out.marks = marks;
        Some(out)
    }

    /// Whether `frag` is the selection: its text and marks, and for each line it covers whole its
    /// block and its attributes, ids aside.
    pub(super) fn holds_selection(&self, frag: &TextEdit) -> bool {
        let (a, b) = self.range();
        if a == b || frag.text != self.text[a..b] { return false; }
        let bytes = |marks: &[MarkRange], from: usize, to: usize| -> std::collections::BTreeSet<(usize, String)> {
            marks.iter().flat_map(|m| (m.start.max(from)..m.end.min(to)).map(move |i| (i - from, format!("{:?}", m.mark)))).collect()
        };
        if bytes(&self.marks, a, b) != bytes(&frag.marks, 0, frag.text.len()) { return false; }
        let (la, lb) = (self.line_of_byte(a), self.line_of_byte(b));
        if frag.blocks.len() != lb - la + 1 { return false; }
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        (0..=lb - la).all(|k| {
            let whole = (k > 0 || a == ra.start) && (k < lb - la || b == rb.end);
            let kind = |x: ParagraphAttrs| (x.style, x.align, x.indent.to_bits(), x.first_line_indent.to_bits(), x.page_break_before, x.atom.map(f32::to_bits));
            !whole || (frag.blocks[k] == self.blocks[la + k] && (!self.attributed() || kind(frag.paragraph(k)) == kind(self.paragraph(la + k))))
        })
    }
}
