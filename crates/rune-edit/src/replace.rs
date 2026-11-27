//! Core text and attribute replacement engine, grapheme stepping, and input sanitization.

use super::paragraph::{Edge, new_lines};
use super::*;

/// The previous grapheme boundary, so a caret or backspace takes a ZWJ emoji, flag or skin
/// tone whole.
#[doc(hidden)]
pub fn prev_char(s: &str, i: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    s[..i].grapheme_indices(true).next_back().map_or(0, |(j, _)| j)
}

/// Where Backspace deletes back to from `i`, as Chromium's `BackspaceStateMachine` and native
/// fields do: one code point (a combining accent, a Thai or Indic vowel sign, the last jamo of
/// a decomposed syllable), so a typist fixes the last thing they typed without retyping the
/// cluster; but a CR LF and an emoji sequence (a variation selector, a skin tone, a keycap, a
/// flag's pair, a tag sequence, a ZWJ sequence of emoji) go whole.
pub(crate) fn prev_backspace(s: &str, i: usize) -> usize {
    let start = prev_char(s, i);
    let cluster = &s[start..i];
    let emoji = cluster.chars().any(|c| matches!(c as u32, 0xFE0F | 0x20E3 | 0x1F3FB..=0x1F3FF | 0x1F1E6..=0x1F1FF | 0xE0020..=0xE007F))
        || (cluster.contains('\u{200D}') && cluster.chars().any(rune_text::is_pictographic));
    if cluster == "\r\n" || emoji {
        return start;
    }
    s[..i].char_indices().next_back().map_or(0, |(j, _)| j)
}

#[doc(hidden)]
pub fn next_char(s: &str, i: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    s[i..].graphemes(true).next().map_or(i, |g| i + g.len())
}

/// `\n` bytes in `s`. Counts into `u8` lanes so it vectorizes: `filter().count()` widens every
/// byte to `usize` and runs several times slower on a long field.
pub(crate) fn count_newlines(s: &[u8]) -> usize {
    s.chunks(255).map(|c| c.iter().fold(0u8, |n, &b| n + (b == b'\n') as u8) as usize).sum()
}

impl TextEdit {
    /// Text typed, pasted or committed into this field. Single-line fields drop control
    /// characters, line breaks included (stripped, as `<input type=text>` does); multi-line
    /// fields fold CRLF to LF (and multi-line secure fields also drop control characters other
    /// than `\n` and `\t`). Sized up front so no partial copy of a secret is left behind by a
    /// reallocation.
    pub(crate) fn sanitize(&self, s: &str) -> Wiped {
        let mut out = Wiped(String::with_capacity(s.len()));
        if self.is_single_line() {
            out.extend(s.chars().filter(|c| !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}')));
        } else if self.secure {
            let mut chars = s.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\r' {
                    if chars.peek() != Some(&'\n') {
                        out.push('\n');
                    }
                    continue;
                }
                if (!c.is_control() || matches!(c, '\n' | '\t')) && !matches!(c, '\u{2028}' | '\u{2029}') {
                    out.push(c);
                }
            }
        } else if !s.contains('\r') {
            // Most pastes: copied whole, not a character at a time.
            out.push_str(s);
        } else {
            let mut chars = s.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\r' && chars.peek() == Some(&'\n') { continue; }
                out.push(c);
            }
        }
        out
    }

    /// Replaces `a..b` with `s` as the atoms beside it allow ([`Self::fence`]), updating `text`,
    /// `marks`, `blocks`, `soft_breaks` and the paragraph attributes together.
    pub(crate) fn apply_text_and_attr_replace(&mut self, (a, b): (usize, usize), s: &str, inserted_marks: &[Mark], new_line_block: Option<Block>, mark_new_lines_soft: bool) {
        let f = self.fence(a, b, s);
        self.apply_fenced((f.a, f.b), f.text.as_deref().unwrap_or(s), inserted_marks, new_line_block, mark_new_lines_soft, f.back);
    }

    /// Core replacement engine. `back` is how far before the inserted text's end the caret goes.
    pub(crate) fn apply_fenced(
        &mut self,
        range: (usize, usize),
        s: &str,
        inserted_marks: &[Mark],
        new_line_block: Option<Block>,
        mark_new_lines_soft: bool,
        back: usize,
    ) {
        self.apply_fenced_lines(range, s, inserted_marks, new_line_block, mark_new_lines_soft, back, false);
    }

    /// [`apply_fenced`](Self::apply_fenced) that, with `want`, says which of the lines from the
    /// one `range` starts on, as many as the text's line breaks add, are new paragraphs, and
    /// which hold what the edit left of an existing one. A paste decorates the new ones.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_fenced_lines(
        &mut self,
        (a, b): (usize, usize),
        s: &str,
        inserted_marks: &[Mark],
        new_line_block: Option<Block>,
        mark_new_lines_soft: bool,
        back: usize,
        want: bool,
    ) -> Vec<bool> {
        // Callers sync first; a second whole-text pass here would show on every keystroke.
        self.column = Column::Free;
        self.affinity = parley::Affinity::Downstream;
        let del_len = b - a;
        let ins_len = s.len();
        if self.marks.is_empty()
            && inserted_marks.is_empty()
            && self.soft_breaks.is_empty()
            && !mark_new_lines_soft
            && new_line_block.is_none_or(|b| b == Block::Paragraph)
            && self.attrs.is_empty()
            && !want
            && self.blocks.iter().all(|&b| b == Block::Paragraph)
        {
            // Plain text: no per-line passes, so a keystroke in a long plain field costs what
            // it did before blocks and marks existed.
            let removed = count_newlines(&self.text.as_bytes()[a..b]);
            let added = count_newlines(s.as_bytes());
            self.text.replace_range(a..b, s);
            self.blocks.resize(self.blocks.len() + added - removed, Block::Paragraph);
            self.anchor = a + ins_len;
            self.focus = self.anchor;
            self.cancel_composition();
            if del_len + ins_len > 0 { self.note_text(a..b, ins_len); }
            return Vec::new();
        }
        let line_a = self.line_of_byte(a);
        let line_b = self.line_of_byte(b);
        let line_a_range = self.line_range(line_a);
        let line_b_range = self.line_range(line_b);

        let removed_lines = line_b - line_a;
        let added_lines = count_newlines(s.as_bytes());
        // An empty first paragraph gives way to the one merged into it; an atom the edit ends
        // before keeps its line.
        let kept = self.kept_atoms((a, b), (line_a, line_b), s);
        // So does an atom deleted from its start: what follows it is not the atom.
        let atom_gone = line_a < line_b && a == line_a_range.start && !kept.head && self.paragraph(line_a).atom.is_some();
        // An atom that stays on a line of its own behind the new text gives nothing to the
        // line before it.
        // And paragraphs taken whole, from a line's start to the start of another's, whatever
        // goes in their place: a paragraph's id, attributes and block live in its line break,
        // and the one that stays is the last one's, which the text left joins.
        let whole = line_a < line_b && a == line_a_range.start && b == line_b_range.start;
        let take_b = !(kept.tail && added_lines > 0) && (atom_gone || whole);
        // An insertion at the start of a line, over a selection that starts there or none, goes
        // before what is left of it (an atom's line the selection takes counts as gone): the
        // line keeps its id and attributes and the paragraphs the text brings are new ones above
        // it. Return gives them the line's style and alignment; other text, the defaults.
        // The label of an atom taken whole, without its break, leaves its paragraph as text.
        let label_gone = line_a == line_b && a == line_a_range.start && b == line_a_range.end && self.paragraph(line_a).atom.is_some();
        let block_a = self.blocks.get(line_a).copied().unwrap_or(Block::Paragraph);
        let front = (a == line_a_range.start && added_lines > 0 && !mark_new_lines_soft && !kept.head && !kept.tail
            && (self.paragraph(line_a).atom.is_none() || atom_gone || label_gone) && new_line_block.is_none_or(|blk| blk == block_a))
            .then(|| new_line_block.is_some());
        // Text with a line break over a selection that starts in the middle of a line and ends in a
        // later one: the last line the edit leaves ends with the later paragraph's break, so it is
        // that paragraph, with its id, attributes and block; the first line keeps its own.
        let tail_whole = front.is_none() && !take_b && !kept.head && !kept.tail && added_lines > 0 && line_a < line_b && a != line_a_range.start
            && self.paragraph(line_b).atom.is_none();
        let block_b = self.blocks.get(line_b).copied().unwrap_or(Block::Paragraph);
        let edge = match front {
            Some(copy) => Edge::Front { copy },
            None if tail_whole => Edge::Tail,
            None => Edge::Inside { copy: new_line_block.is_some(), rest: b < line_b_range.end },
        };
        let new_lines = if want { new_lines(added_lines, kept, edge, line_a == line_b) } else { Vec::new() };
        self.splice_attrs(line_a, line_b, added_lines, take_b, kept, edge);
        // Determine the surviving block for `line_a` when `a..b` merges `line_a..=line_b`.
        let merged_block = if take_b || (kept.tail && added_lines == 0) {
            self.blocks.get(line_b).copied().unwrap_or(Block::Paragraph)
        } else {
            self.blocks.get(line_a).copied().unwrap_or(Block::Paragraph)
        };
        let merged_soft = if line_a < line_b
            && line_a_range.is_empty()
            && a == line_a_range.start
            && b == line_b_range.start
        {
            self.soft_breaks.binary_search(&(line_b as u32)).is_ok() && line_a > 0
        } else {
            self.soft_breaks.binary_search(&(line_a as u32)).is_ok()
        };

        let next_line_blk = new_line_block.unwrap_or(match merged_block {
            Block::Code => Block::Code,
            _ => Block::Paragraph,
        });

        // Update `blocks` for removed and added lines.
        if removed_lines > 0 {
            self.blocks.drain(line_a + 1..=line_b);
        }
        if let Some(slot) = self.blocks.get_mut(line_a) {
            *slot = merged_block;
        }
        if added_lines > 0 {
            let insert_pos = (line_a + 1).min(self.blocks.len());
            self.blocks.splice(insert_pos..insert_pos, std::iter::repeat_n(next_line_blk, added_lines));
        }
        // Text with a line break in the middle of a line leaves the tail in the line's block; one
        // that ends at the start of a later line leaves that line's own.
        if tail_whole {
            if let Some(slot) = self.blocks.get_mut(line_a + added_lines) {
                *slot = block_b;
            }
        } else if added_lines > 0 && front.is_none() && new_line_block.is_none() && !kept.tail && b < line_b_range.end {
            if let Some(slot) = self.blocks.get_mut(line_a + added_lines) {
                *slot = merged_block;
            }
        }
        if front.is_some() {
            if let Some(slot) = self.blocks.get_mut(line_a) {
                *slot = next_line_blk;
            }
            if let Some(slot) = self.blocks.get_mut(line_a + added_lines) {
                *slot = merged_block;
            }
        }

        // Update `soft_breaks`.
        let mut next_soft = Vec::new();
        for &sb in &self.soft_breaks {
            let idx = sb as usize;
            if idx < line_a {
                next_soft.push(sb);
            } else if idx > line_b {
                let shifted = idx - removed_lines + added_lines;
                if shifted > 0 {
                    next_soft.push(shifted as u32);
                }
            }
        }
        if merged_soft && line_a > 0 {
            next_soft.push(line_a as u32);
        }
        if mark_new_lines_soft && added_lines > 0 {
            for k in 1..=added_lines {
                next_soft.push((line_a + k) as u32);
            }
        }
        self.soft_breaks = next_soft;

        // Shift and split existing inline marks around `a..b`.
        let mut updated_marks = Vec::with_capacity(self.marks.len() + inserted_marks.len());
        for m in self.marks.drain(..) {
            if m.end <= a {
                updated_marks.push(m);
            } else if m.start >= b {
                updated_marks.push(MarkRange {
                    start: m.start - del_len + ins_len,
                    end: m.end - del_len + ins_len,
                    mark: m.mark,
                });
            } else if m.mark.is_pill() {
                // An edit that cuts into a pill leaves its remains as plain text.
            } else {
                // `m` overlaps `a..b` (or touches inside it).
                if m.start < a {
                    updated_marks.push(MarkRange {
                        start: m.start,
                        end: a,
                        mark: m.mark.clone(),
                    });
                }
                if m.end > b {
                    updated_marks.push(MarkRange {
                        start: a + ins_len,
                        end: m.end - del_len + ins_len,
                        mark: m.mark,
                    });
                }
            }
        }
        if ins_len > 0 {
            // Typed or replacing text never becomes a pill.
            for im in inserted_marks.iter().filter(|m| !m.is_pill()) {
                // Avoid wrapping a pure newline at the trailing edge of a mark unless inside the mark.
                if s == "\n" {
                    let was_inside = updated_marks.iter().any(|m| m.mark == *im && m.start >= a + ins_len);
                    if !was_inside {
                        continue;
                    }
                }
                updated_marks.push(MarkRange {
                    start: a,
                    end: a + ins_len,
                    mark: im.clone(),
                });
            }
        }
        self.marks = updated_marks;
        self.text.replace_range(a..b, s);
        if del_len + ins_len > 0 { self.note_text(a..b, ins_len); }
        self.anchor = a + ins_len - back;
        self.focus = self.anchor;
        self.cancel_composition();
        self.sync_invariants();
        self.mark_edited();
        new_lines
    }

    pub(crate) fn replace(&mut self, (mut a, mut b): (usize, usize), s: &str, kind: EditKind) {
        if self.is_hidden_multiline() && self.is_pristine() {
            (a, b) = (0, self.text.len());
        }
        if self.secure {
            let mut next = String::with_capacity(self.text.len() - (b - a) + s.len());
            next.push_str(&self.text[..a]);
            next.push_str(s);
            next.push_str(&self.text[b..]);
            self.replace_text(next);
            self.anchor = a + s.len();
            self.focus = self.anchor;
            self.mark_edited();
            return;
        }
        self.sync_invariants();
        // Backspace or Delete next to a pill, or a selection into one, takes it whole.
        let (a, b) = self.cover_pills(a, b);
        let fenced = self.fence(a, b, s);
        let (a, b, back) = (fenced.a, fenced.b, fenced.back);
        let s = fenced.text.as_deref().unwrap_or(s);
        if kind == EditKind::Other { self.break_history_group(); }
        if &self.text[a..b] == s
            && self.typing == Typing::Inherited
            && !self.marks.iter().any(|m| m.mark.is_pill() && m.start < b && a < m.end)
        {
            self.anchor = a + s.len() - back;
            self.focus = self.anchor;
            self.cancel_composition();
            self.mark_edited();
            return;
        }
        // Only the bytes this replaces: a keystroke in a long draft copies none of the rest.
        let before = self.ranged(a, b);
        let inserted_marks: Vec<Mark> = if s.is_empty() {
            self.typing = Typing::Inherited;
            Vec::new()
        } else if let Some(p) = self.typing.take_at((self.anchor, self.focus)) {
            p
        } else if a < b {
            // Replacing a selection inherits marks that covered the first character of the selection.
            let line_idx = self.line_of_byte(a);
            if self.blocks.get(line_idx) == Some(&Block::Code) {
                Vec::new()
            } else {
                self.marks
                    .iter()
                    .filter(|m| m.start <= a && a < m.end)
                    .map(|m| m.mark.clone())
                    .collect()
            }
        } else {
            self.inherited_marks_at(a)
        };

        self.apply_fenced((a, b), s, &inserted_marks, None, false, back);
        self.note_kind(kind);
        self.record_ranged(before, s, kind);
    }
}
