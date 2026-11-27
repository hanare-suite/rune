//! What a paste over the selection joins, lands in, and holds: the shape the fragment paste goes by.

use super::*;

impl TextEdit {
    /// The line a paste over the selection lands in, the one what is left of the selection's lines
    /// joins: the later line when the selection takes the earlier one whole, from its start into a
    /// later line (an atom, or paragraphs up to a line's start), the first otherwise.
    pub(super) fn landing_line(&self) -> usize {
        let (a, b) = self.range();
        let (la, lb) = (self.line_of_byte(a), self.line_of_byte(b));
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        let soft = |l: usize| self.soft_breaks.binary_search(&(l as u32)).is_ok();
        if la < lb && a == ra.start && !soft(la) && ((b == rb.start && !soft(lb)) || (b == self.text.len() && b == rb.end) || self.paragraph(la).atom.is_some()) { lb } else { la }
    }

    /// A copy of what an edit's lines depend on, without the history, the change log or the typing state: text,
    /// marks, blocks, attributes and soft lines, for a dry run of an edit.
    fn scratch_copy(&self) -> TextEdit {
        let mut c = TextEdit::default();
        c.text = self.text.clone();
        c.anchor = self.anchor;
        c.focus = self.focus;
        c.marks = self.marks.clone();
        c.blocks = self.blocks.clone();
        c.soft_breaks = self.soft_breaks.clone();
        c.attrs = self.attrs.clone();
        c.next_id = self.next_id;
        c.line_mode = self.line_mode;
        c.rich = self.rich;
        c.allowed_marks = self.allowed_marks;
        c.allowed_blocks = self.allowed_blocks;
        c.history_off = true;
        c.html_styles = self.html_styles.clone();
        c
    }

    /// Unformatted text pasted as typed, but where it starts at a paragraph's start in a document, the lines that
    /// become paragraphs of their own take no marks from the one that starts there (the next paragraph's bold, say,
    /// when a cut paragraph is pasted back before it); an open last line, which joins that paragraph, inherits as
    /// typed text does, so its run size is kept. One undo step.
    pub(super) fn paste_unformatted(&mut self, plain: &str) -> EditOutcome {
        let (a, b) = self.range();
        let split = plain.rfind('\n').map(|i| i + 1);
        match split {
            Some(k) if self.attributed() && a == self.line_range(self.line_of_byte(a)).start && self.soft_breaks.binary_search(&(self.line_of_byte(a) as u32)).is_err() && self.text[a..b] != *plain => {
                let inherited = self.inherited_marks_at(a);
                let tail = &plain[k..];
                self.undo_group(|e| {
                    e.typing = Typing::Armed { at: (e.anchor, e.focus), marks: Vec::new() };
                    e.paste(plain);
                    let end = e.focus;
                    let start = end.saturating_sub(tail.len());
                    if !tail.is_empty() && !inherited.is_empty() && e.text.get(start..end) == Some(tail) {
                        e.marks.extend(inherited.into_iter().map(|mark| MarkRange { start, end, mark }));
                        e.normalize_marks();
                        e.note_marks(start..end);
                    }
                });
                EditOutcome::default()
            }
            _ => self.paste(plain),
        }
    }

    /// Which lines of `frag` land on a line of code, as the paste puts them: the paste is done to a copy and the
    /// lines it makes of the fragment's are read there, a line of code that is no atom's. So this cannot
    /// differ from what the paste does.
    pub(super) fn code_lines(&self, frag: &TextEdit) -> Vec<bool> {
        let n = frag.text.split('\n').count();
        if !self.blocks.contains(&Block::Code) {
            return vec![false; n];
        }
        let (a, b) = self.range();
        let f = self.fence(a, b, &frag.text);
        let first = self.line_of_byte(f.a);
        let mut dry = self.scratch_copy();
        dry.apply_fenced_lines((f.a, f.b), f.text.as_deref().unwrap_or(&frag.text), &[], None, false, f.back, false);
        (0..n).map(|k| {
            let line = first + f.lead + k;
            dry.blocks.get(line) == Some(&Block::Code) && dry.paragraph(line).atom.is_none()
        }).collect()
    }

    /// `frag` with the text of the lines `code` flags taken from `plain`, line for line, where `plain` has
    /// the same lines: a code line keeps the whitespace the HTML collapsed. Marks of those lines go.
    pub(super) fn with_plain_code_lines(frag: &TextEdit, plain: &str, code: &[bool]) -> Option<TextEdit> {
        let (old, pl): (Vec<&str>, Vec<&str>) = (frag.text.split('\n').collect(), plain.split('\n').collect());
        if old.len() != code.len() || !code.iter().any(|&c| c) { return None; }
        // The plain line each fragment line comes from: the same line when the lines match, else the lines
        // that hold text, with the atoms' labels left out, in order.
        let is_atom = |k: usize| frag.attrs.get(k).is_some_and(|a| a.atom.is_some());
        let src: Vec<Option<&str>> = if old.len() == pl.len() {
            pl.iter().map(|l| Some(*l)).collect()
        } else {
            let mut labels: Vec<&str> = (0..old.len()).filter(|&k| is_atom(k)).map(|k| old[k]).collect();
            let kept: Vec<&str> = pl.iter().copied().filter(|l| {
                if l.is_empty() { return false; }
                if let Some(i) = labels.iter().position(|x| x == l) { labels.remove(i); return false; }
                true
            }).collect();
            let texts: Vec<usize> = (0..old.len()).filter(|&k| !is_atom(k) && !old[k].is_empty()).collect();
            if texts.len() != kept.len() { return None; }
            let mut out: Vec<Option<&str>> = vec![None; old.len()];
            for (k, l) in texts.into_iter().zip(kept) { out[k] = Some(l); }
            out
        };
        let code: Vec<bool> = code.iter().enumerate().map(|(k, &c)| c && src[k].is_some()).collect();
        if !code.iter().any(|&c| c) { return None; }
        let (pl, code) = (src, code);
        // Where each byte of the fragment's text goes: a plain line's own bytes are no one's.
        let mut map: Vec<Option<usize>> = vec![None; frag.text.len() + 1];
        let (mut text, mut at) = (String::new(), 0usize);
        for (k, line) in old.iter().enumerate() {
            let from = if code[k] { pl[k].unwrap_or(line) } else { line };
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

    /// A soft line of the fragment that lands on the line that keeps what was there: where the head of that group of lines
    /// is a new line (the paste is at a paragraph's start), the receiving paragraph is the head. Its first line takes the
    /// receiving line's id, attributes and block, and the soft lines (the landed one with an id of its own) mirror it, as
    /// typing the text with `Shift+Return` gives. Where the head is a line that was there (the paste is inside a
    /// paragraph), it stays as it is and mirroring does the rest, as typed text does.
    pub(super) fn receiving_paragraph_heads_soft_lines(&mut self, frag: &TextEdit, line_idx: usize, frag_lines: usize, new: &[bool], lead: usize) {
        let is_new = |j: usize| new.get(j).copied().unwrap_or(false);
        if frag_lines < 2 || is_new(lead + frag_lines - 1) { return; }
        let last_line = line_idx + frag_lines - 1;
        let mut head_k = frag_lines - 1;
        while head_k > 0 && frag.soft_breaks.contains(&(head_k as u32)) && self.soft_breaks.contains(&((line_idx + head_k) as u32)) { head_k -= 1; }
        let head_line = line_idx + head_k;
        if head_line >= last_line || !is_new(lead + head_k) { return; }
        if self.attributed() {
            let recv = self.paragraph(last_line);
            let id = self.fresh_id();
            let slot = self.attrs_mut();
            slot[head_line] = recv;
            slot[last_line] = recv.id(id);
        }
        self.blocks[head_line] = self.blocks[last_line];
    }

    /// Everything an edit can change in the document, to tell whether it changed anything.
    pub(super) fn state_key(&self) -> (String, Vec<(usize, usize, String)>, Vec<Block>, Vec<ParagraphAttrs>, Vec<u32>) {
        let mut marks: Vec<(usize, usize, String)> = self.marks.iter().map(|m| (m.start, m.end, format!("{:?}", m.mark))).collect();
        marks.sort();
        (self.text.clone(), marks, self.blocks.clone(), self.attrs.clone(), self.soft_breaks.clone())
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
        if (1..=lb - la).any(|k| self.soft_breaks.contains(&((la + k) as u32)) != frag.soft_breaks.contains(&(k as u32))) { return false; }
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        (0..=lb - la).all(|k| {
            let whole = (k > 0 || a == ra.start) && (k < lb - la || b == rb.end);
            let kind = |x: ParagraphAttrs| (x.style, x.align, x.indent.to_bits(), x.first_line_indent.to_bits(), x.page_break_before, x.atom.map(f32::to_bits));
            !whole || (frag.blocks[k] == self.blocks[la + k] && (!self.attributed() || kind(frag.paragraph(k)) == kind(self.paragraph(la + k))))
        })
    }
}
