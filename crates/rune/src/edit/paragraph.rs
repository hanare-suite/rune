//! What a paragraph of a [`TextEdit`] carries besides its [`Block`]: a style, an alignment,
//! indents, a page break, an id the app finds its own data by, and whether it is a locked atom.
//! Splitting a paragraph copies them, merging two keeps the first's, and an atom's label stays
//! whole through every edit.

use super::*;

/// Where a paragraph's lines sit between its indents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ParagraphAlign {
    #[default]
    Left,
    Center,
    Right,
    /// Every line but the last fills the width.
    Justified,
}

/// A paragraph's attributes: one per `\n`-delimited line of [`TextEdit::text`], beside its
/// [`Block`]. More fields may come, so build one from `ParagraphAttrs::default()` and the
/// methods here. It compares and hashes by its bits.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ParagraphAttrs {
    /// Its entry in the field's [`El::paragraph_styles`](crate::El::paragraph_styles); the
    /// field's own style when there is no such entry.
    pub style: u16,
    pub align: ParagraphAlign,
    /// Points every line starts in from the left.
    pub indent: f32,
    /// Points the first line starts in from the others: negative hangs it out.
    pub first_line_indent: f32,
    /// Starts the paragraph on a new page of a field with [`El::page_flow`](crate::El::page_flow).
    pub page_break_before: bool,
    /// A locked atom this many points tall: the paragraph's text is its label, the caret steps
    /// over it as one character, a selection covers it whole and deleting it is one edit.
    pub atom: Option<f32>,
    /// The app's own handle on the paragraph. A split gives the new paragraph the next id of
    /// [`TextEdit::set_paragraph_ids`]; 0 without one.
    pub id: u64,
}

impl ParagraphAttrs {
    pub fn style(mut self, style: u16) -> Self {
        self.style = style;
        self
    }

    pub fn align(mut self, align: ParagraphAlign) -> Self {
        self.align = align;
        self
    }

    /// The left indent, and the first line's on top of it.
    pub fn indent(mut self, left: f32, first_line: f32) -> Self {
        (self.indent, self.first_line_indent) = (left, first_line);
        self
    }

    pub fn page_break_before(mut self, on: bool) -> Self {
        self.page_break_before = on;
        self
    }

    /// A locked atom `height` points tall.
    pub fn atom(mut self, height: f32) -> Self {
        self.atom = Some(height);
        self
    }

    pub fn id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    fn bits(&self) -> (u16, ParagraphAlign, u32, u32, bool, Option<u32>, u64) {
        (self.style, self.align, self.indent.to_bits(), self.first_line_indent.to_bits(), self.page_break_before, self.atom.map(f32::to_bits), self.id)
    }

    /// What the paragraph's own layout depends on: not its page break or id, which only place
    /// it or name it.
    pub(crate) fn layout_bits(&self) -> (u16, ParagraphAlign, u32, u32, bool) {
        (self.style, self.align, self.indent.to_bits(), self.first_line_indent.to_bits(), self.atom.is_some())
    }

    /// What the field's layout depends on: everything but the id.
    pub(crate) fn flow_bits(&self) -> impl std::hash::Hash {
        (self.layout_bits(), self.page_break_before, self.atom.map(f32::to_bits))
    }

    /// Finite numbers only, an atom at least a point tall.
    fn clean(mut self) -> Self {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        self.indent = finite(self.indent).max(0.0);
        self.first_line_indent = finite(self.first_line_indent);
        self.atom = self.atom.map(|h| finite(h).max(1.0));
        self
    }

    /// Its fields as a stored history keeps them.
    pub(super) fn to_json(self) -> serde_json::Value {
        let align = match self.align { ParagraphAlign::Left => 0, ParagraphAlign::Center => 1, ParagraphAlign::Right => 2, ParagraphAlign::Justified => 3 };
        serde_json::json!([self.style, align, self.indent, self.first_line_indent, self.page_break_before, self.atom, self.id])
    }

    pub(super) fn from_json(v: &serde_json::Value) -> Option<Self> {
        let f = |i: usize| v[i].as_f64().map(|n| n as f32);
        let align = match v[1].as_u64()? { 0 => ParagraphAlign::Left, 1 => ParagraphAlign::Center, 2 => ParagraphAlign::Right, _ => ParagraphAlign::Justified };
        Some(Self { style: v[0].as_u64()? as u16, align, indent: f(2)?, first_line_indent: f(3)?, page_break_before: v[4].as_bool()?, atom: f(5), id: v[6].as_u64()? }.clean())
    }

    /// What a paragraph split off this one takes: everything but its page break, its atom and
    /// its id.
    fn split(self, id: u64) -> Self {
        Self { page_break_before: false, atom: None, id, ..self }
    }
}

impl PartialEq for ParagraphAttrs {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for ParagraphAttrs {}

impl std::hash::Hash for ParagraphAttrs {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.bits().hash(h);
    }
}

/// An edit's range and text as the atoms beside it allow: see [`TextEdit::fence`].
pub(super) struct Fenced {
    pub(super) a: usize,
    pub(super) b: usize,
    /// The text to insert, when it gained a line break that keeps an atom on its own line.
    pub(super) text: Option<String>,
    /// How far before the inserted text's end the caret goes: over a line break added there.
    pub(super) back: usize,
}

/// Which lines of an edit hold an atom's untouched label, and so keep its attributes.
#[derive(Clone, Copy, Default)]
pub(super) struct Kept {
    /// The first line the edit touches is an atom the edit starts after.
    pub(super) head: bool,
    /// The last line it leaves is an atom the edit ends before.
    pub(super) tail: bool,
}

impl TextEdit {
    /// Each paragraph's attributes, one per `\n`-delimited line of `self.text`.
    pub fn paragraph_attrs(&self) -> Vec<ParagraphAttrs> {
        let lines = 1 + count_newlines(self.text.as_bytes());
        (0..lines).map(|i| self.paragraph(i)).collect()
    }

    /// The attributes of paragraph `line`, the default past the last.
    pub fn paragraph(&self, line: usize) -> ParagraphAttrs {
        self.attrs.get(line).copied().unwrap_or_default()
    }

    /// Replaces every paragraph's attributes, as [`Self::set_blocks`] does their blocks: not an
    /// undo step, and not reported as a change. The list is padded or cut to the text's lines.
    pub fn set_paragraph_attrs(&mut self, attrs: Vec<ParagraphAttrs>) {
        self.attrs = attrs.into_iter().map(ParagraphAttrs::clean).collect();
        self.sync_invariants();
    }

    /// Replaces paragraph `line`'s attributes, as [`Self::set_paragraph_attrs`] does all of them.
    pub fn set_paragraph(&mut self, line: usize, attrs: ParagraphAttrs) {
        self.sync_invariants();
        if let Some(slot) = self.attrs_mut().get_mut(line) {
            *slot = attrs.clean();
        }
        self.sync_invariants();
    }

    /// The attributes of the paragraph at the caret, or the selection's first.
    pub fn active_paragraph(&self) -> ParagraphAttrs {
        self.paragraph(self.selected_lines().start)
    }

    /// The paragraphs the selection touches, as line indices.
    pub fn selected_lines(&self) -> Range<usize> {
        let (first, last) = self.selected_line_range();
        first..last + 1
    }

    /// Changes the attributes of every paragraph the selection touches: one edit, reported as
    /// one change of [`EditChangeKind::Paragraphs`]. A paragraph's id and its atom stay as they
    /// were, whatever `f` writes there.
    pub fn format_paragraphs(&mut self, mut f: impl FnMut(&mut ParagraphAttrs)) {
        if self.secure || self.is_single_line() {
            return;
        }
        self.sync_invariants();
        self.break_history_group();
        let before = self.snapshot();
        let lines = self.selected_lines();
        let was = self.attrs.clone();
        for slot in &mut self.attrs_mut()[lines.clone()] {
            let old = *slot;
            f(slot);
            *slot = ParagraphAttrs { id: old.id, atom: old.atom, ..*slot }.clean();
        }
        if was.is_empty() && self.attrs.iter().all(|a| *a == ParagraphAttrs::default()) {
            self.attrs.clear();
        }
        if self.attrs != was {
            self.note_paragraphs(lines);
        }
        self.record_change(before, EditKind::Other);
    }

    /// Gives every paragraph an edit makes from now on an id: `next`, then the one after it,
    /// and so on. Paragraphs already there keep the ids they have. 0 turns ids off.
    pub fn set_paragraph_ids(&mut self, next: u64) {
        self.next_id = next;
        if next != 0 {
            self.sync_invariants();
            self.attrs_mut();
        }
    }

    /// The id the next new paragraph takes, for an app to keep with its document; 0 when ids
    /// are off.
    pub fn next_paragraph_id(&self) -> u64 {
        self.next_id
    }

    /// Replaces the selection with a locked atom on a line of its own: `label` as its text,
    /// `height` points tall, with the next paragraph id. One edit. Line breaks in `label`
    /// become spaces; an empty label inserts nothing.
    pub fn insert_atom(&mut self, label: &str, height: f32) {
        let label: String = label.chars().map(|c| if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') { ' ' } else { c }).collect();
        if self.secure || self.is_single_line() || label.trim().is_empty() {
            return;
        }
        self.sync_invariants();
        self.undo_group(|e| {
            let (a, b) = e.range();
            e.replace((a, b), "", EditKind::Other);
            let at = e.range().0;
            let line = e.line_of_byte(at);
            let range = e.line_range(line);
            // On a line of its own: a break before it unless the line starts here, and one
            // after it unless the line ends here.
            let (lead, trail) = (at > range.start || e.paragraph(line).atom.is_some(), at < range.end);
            let text = format!("{}{label}{}", if lead { "\n" } else { "" }, if trail { "\n" } else { "" });
            e.replace((at, at), &text, EditKind::Other);
            let line = line + usize::from(lead);
            let id = if e.paragraph(line).id != 0 { e.paragraph(line).id } else { e.fresh_id() };
            let attrs = ParagraphAttrs { atom: Some(height), id, ..e.paragraph(line) }.clean();
            if let Some(slot) = e.attrs_mut().get_mut(line) { *slot = attrs; }
            let end = at + usize::from(lead) + label.len();
            (e.anchor, e.focus) = (end, end);
        });
    }

    /// The next id of [`Self::set_paragraph_ids`], or 0 without one.
    pub(super) fn fresh_id(&mut self) -> u64 {
        let id = self.next_id;
        if id != 0 { self.next_id += 1; }
        id
    }

    /// The attributes, one a line: an empty list (every paragraph the default) filled out.
    pub(super) fn attrs_mut(&mut self) -> &mut Vec<ParagraphAttrs> {
        let lines = if self.secure { 0 } else { 1 + count_newlines(self.text.as_bytes()) };
        if self.attrs.len() != lines {
            self.attrs.resize(lines, ParagraphAttrs::default());
        }
        &mut self.attrs
    }

    /// Whether paragraphs carry attributes: Return then breaks a plain paragraph rather than
    /// submitting.
    pub(super) fn attributed(&self) -> bool {
        !self.attrs.is_empty()
    }

    pub(super) fn has_atoms(&self) -> bool {
        self.attrs.iter().any(|a| a.atom.is_some())
    }

    /// The atom paragraph whose label holds byte `i` or ends at it, as its bytes.
    pub(super) fn atom_at(&self, i: usize) -> Option<(usize, usize)> {
        if !self.has_atoms() { return None; }
        let i = self.text.floor_char_boundary(i);
        let start = self.text[..i].rfind('\n').map_or(0, |n| n + 1);
        self.attrs.get(count_newlines(&self.text.as_bytes()[..start]))?.atom?;
        Some((start, self.text[i..].find('\n').map_or(self.text.len(), |n| i + n)))
    }

    /// Keeps the attributes one a line.
    pub(super) fn sync_attrs(&mut self, lines: usize) {
        if !self.attrs.is_empty() {
            self.attrs.resize(lines, ParagraphAttrs::default());
        }
    }

    /// An edit replacing `a..b` with `s`, as the atoms beside it allow. Deleting an atom takes
    /// its line break too, so no empty atom is left. Text that would join an atom's label goes
    /// on a line of its own instead: before the atom when the edit ends at its start, after it
    /// when the edit starts at its end.
    pub(super) fn fence(&self, a: usize, b: usize, s: &str) -> Fenced {
        let plain = Fenced { a, b, text: None, back: 0 };
        if !self.has_atoms() { return plain; }
        let (la, lb) = (self.line_of_byte(a), self.line_of_byte(b));
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        let atom = |line: usize| self.attrs.get(line).is_some_and(|x| x.atom.is_some());
        let len = self.text.len();
        if s.is_empty() && a < b && atom(la) && a == ra.start && atom(lb) && b == rb.end {
            return if b < len { Fenced { b: b + 1, ..plain } } else if a > 0 { Fenced { a: a - 1, ..plain } } else { plain };
        }
        // The label after the edit stays whole when the edit ends at its start: with nothing
        // left before it on its line, or behind a line break.
        let before = atom(lb) && b == rb.start && !(a == b && s.is_empty())
            && !(s.is_empty() && a == ra.start) && !s.ends_with('\n');
        let after = atom(la) && a == ra.end && !(a == b && s.is_empty())
            && !(s.is_empty() && b == rb.end) && !s.starts_with('\n');
        if !before && !after { return plain; }
        let text = format!("{}{s}{}", if after { "\n" } else { "" }, if before { "\n" } else { "" });
        Fenced { a, b, text: Some(text), back: usize::from(before) }
    }

    /// Which ends of an edit replacing `a..b` (lines `la` to `lb`) with `s` are an atom's
    /// untouched label.
    pub(super) fn kept_atoms(&self, (a, b): (usize, usize), (la, lb): (usize, usize), s: &str) -> Kept {
        if !self.has_atoms() { return Kept::default(); }
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        let atom = |line: usize| self.attrs.get(line).is_some_and(|x| x.atom.is_some());
        Kept {
            head: atom(la) && a == ra.end && (s.starts_with('\n') || (s.is_empty() && b == rb.end)),
            tail: atom(lb) && b == rb.start && (s.ends_with('\n') || (s.is_empty() && a == ra.start && la < lb)),
        }
    }

    /// The attributes after an edit that merged lines `la..=lb` into one and added `added`
    /// lines after it. The merged line keeps `la`'s attributes (`lb`'s with `take_b`), each
    /// added line takes a copy with a new id, and an atom stays only on a line `kept` names.
    pub(super) fn splice_attrs(&mut self, la: usize, lb: usize, added: usize, take_b: bool, kept: Kept) {
        if self.attrs.is_empty() { return; }
        let get = |attrs: &[ParagraphAttrs], i: usize| attrs.get(i).copied().unwrap_or_default();
        let (first, last) = (get(&self.attrs, la), get(&self.attrs, lb));
        let survivor = if kept.head { first } else if take_b || (kept.tail && added == 0) { last } else { first };
        let mut lines = Vec::with_capacity(added + 1);
        lines.push(if kept.head || (kept.tail && added == 0) { survivor } else { ParagraphAttrs { atom: None, ..survivor } });
        for k in 1..=added {
            let tail = k == added;
            let own = if tail && kept.tail {
                last
            } else if tail && kept.head && lb > la {
                ParagraphAttrs { atom: None, ..last }
            } else {
                let id = self.fresh_id();
                lines[0].split(id)
            };
            lines.push(own);
        }
        // The atom an insertion pushed down a line took its attributes with it: the line the
        // typed text is on is a new paragraph.
        if kept.tail && added > 0 && la == lb {
            let id = self.fresh_id();
            lines[0] = last.split(id);
        }
        let end = (lb + 1).min(self.attrs.len());
        self.attrs.splice(la.min(end)..end, lines);
    }
}

#[cfg(test)]
#[path = "paragraph_tests.rs"]
mod tests;
