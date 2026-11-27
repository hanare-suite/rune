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

    /// Whether the line a paste over the selection lands in is a line of code (not an atom's).
    pub(super) fn landing_in_code(&self) -> bool {
        let line = self.landing_line();
        !self.is_single_line() && self.blocks.get(line) == Some(&Block::Code) && self.paragraph(line).atom.is_none()
    }

    /// Whether the text of `frag`, pasted over the selection, joins a line of code: its first line
    /// joins the line before it and its last the line after it, and either may be code.
    pub(super) fn joins_code(&self, frag: &TextEdit) -> bool {
        let (a, b) = self.range();
        let f = self.fence(a, b, &frag.text);
        let p = self.edit_plan((f.a, f.b), f.text.as_deref().unwrap_or(&frag.text), None, false);
        let new = new_lines(p.added_lines, p.kept, p.edge, p.line_a == p.line_b);
        let code = |l: usize| self.blocks.get(l) == Some(&Block::Code) && self.paragraph(l).atom.is_none();
        let kept = if p.take_b { p.line_b } else { p.line_a };
        let first = frag.text.split('\n').next().is_some_and(|t| !t.is_empty());
        let last = frag.text.rsplit('\n').next().is_some_and(|t| !t.is_empty());
        let first_joins = first && !new.get(f.lead).copied().unwrap_or(true) && code(kept);
        let last_joins = last && !new.get(p.added_lines.saturating_sub(f.back)).copied().unwrap_or(true) && code(if p.edge == Edge::Tail || (p.kept.head && p.line_a < p.line_b) { p.line_b } else { kept });
        first_joins || last_joins
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
