//! Pills and locked atoms as the caret and selection meet them: listing them, inserting them, and
//! keeping a caret or selection off their insides.

use super::*;

impl TextEdit {
    /// The [`Mark::Pill`]s in `self.text`, in order, with their byte ranges and ids.
    pub fn pills(&self) -> impl Iterator<Item = (Range<usize>, &str)> + '_ {
        let t = &self.text;
        self.marks.iter().filter_map(move |m| {
            let id = m.mark.pill_id()?;
            (m.start < m.end && m.end <= t.len() && t.is_char_boundary(m.start) && t.is_char_boundary(m.end) && !t.as_bytes()[m.start..m.end].contains(&b'\n'))
                .then(|| (m.range(), id))
        })
    }

    /// Replace the selection with `label` as one [`Mark::Pill`] carrying `id`, and put the
    /// caret after it: one undo step. Line breaks in `label` become spaces.
    pub fn insert_pill(&mut self, id: impl Into<String>, label: &str) {
        let (a, b) = self.range();
        self.replace_with_pill(a..b, id, label);
    }

    /// Replace byte range `range` with `label` as one [`Mark::Pill`] carrying `id` (an `@query`
    /// the user typed, say), and put the caret after it: one undo step. The range grows to
    /// cover any pill it cuts into. Line breaks in `label` become spaces; an empty label or a
    /// secure field inserts nothing.
    pub fn replace_with_pill(&mut self, range: Range<usize>, id: impl Into<String>, label: &str) {
        let label: String = label.chars().map(|c| if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') { ' ' } else { c }).collect();
        if self.secure || label.is_empty() {
            return;
        }
        self.sync_invariants();
        self.break_history_group();
        let before = self.snapshot();
        let a = Self::clamp_char_boundary(&self.text, range.start.min(range.end));
        let b = Self::clamp_char_boundary(&self.text, range.end.max(range.start));
        let (a, b) = self.cover_pills(a, b);
        self.typing = Typing::Inherited;
        self.apply_text_and_attr_replace((a, b), &label, &[], None, false);
        // Code blocks keep no inline marks: there the label stays plain text.
        self.marks.push(MarkRange { start: a, end: a + label.len(), mark: Mark::Pill(id.into()) });
        self.normalize_marks();
        self.record_change(before, EditKind::Other);
    }

    /// The pill, or the locked atom, strictly containing byte `i` (`start < i < end`).
    pub(crate) fn pill_around(&self, i: usize) -> Option<(usize, usize)> {
        self.marks.iter().find(|m| m.mark.is_pill() && m.start < i && i < m.end).map(|m| (m.start, m.end))
            .or_else(|| self.atom_at(i).filter(|&(s, e)| s < i && i < e))
    }

    /// `i` moved out of any pill it lands inside, to its end when `forward`, else its start.
    pub(crate) fn out_of_pill(&self, i: usize, forward: bool) -> usize {
        self.pill_around(i).map_or(i, |(s, e)| if forward { e } else { s })
    }

    /// `a..b` grown to cover any pill either end cuts into; an empty range inside one moves
    /// to its end.
    pub(crate) fn cover_pills(&self, a: usize, b: usize) -> (usize, usize) {
        if a == b {
            let i = self.out_of_pill(a, true);
            return (i, i);
        }
        (self.out_of_pill(a, false), self.out_of_pill(b, true))
    }

    /// Keeps the caret off a pill's inside: a caret goes to the nearer edge, a selection grows
    /// to cover the pill.
    #[doc(hidden)]
    pub fn snap_to_pills(&mut self) {
        if !self.marks.iter().any(|m| m.mark.is_pill()) && !self.has_atoms() {
            return;
        }
        if self.anchor == self.focus {
            if let Some((s, e)) = self.pill_around(self.focus) {
                let to = if self.focus - s < e - self.focus { s } else { e };
                (self.anchor, self.focus) = (to, to);
            }
        } else {
            let forward = self.focus > self.anchor;
            self.anchor = self.out_of_pill(self.anchor, !forward);
            self.focus = self.out_of_pill(self.focus, forward);
        }
    }
}
