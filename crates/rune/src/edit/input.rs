//! Keys, IME and paste on a [`TextEdit`].

use parley::Layout;

use super::*;
use crate::text::{Ink, TextFlow};

impl TextEdit {
    fn insert_newline(&mut self, block_for_new_line: Block, soft: bool) {
        if self.secure || self.is_single_line() {
            return;
        }
        self.sync_invariants();
        let before = self.snapshot();
        let (a, b) = self.range();
        let inserted_marks = self.typing.take_at((self.anchor, self.focus)).unwrap_or_else(|| self.inherited_marks_at(a));
        self.apply_text_and_attr_replace((a, b), "\n", &inserted_marks, Some(block_for_new_line), soft);
        self.note_kind(EditKind::LineBreak);
        self.record_change(before, EditKind::LineBreak);
    }

    pub(super) fn set(&mut self, i: usize, extend: bool) {
        // A caret moving into a pill steps over it, the way it was going.
        let i = self.out_of_pill(i, i > self.focus);
        self.focus = i;
        if !extend {
            self.anchor = i;
        }
        self.snap_to_pills();
        self.typing = Typing::Inherited;
        self.affinity = parley::Affinity::Downstream;
    }

    /// Moves the caret to `i` (on `side` of it, from a visual step), or collapses a selection to
    /// the end the arrow points at (`right`).
    fn move_h(&mut self, (i, side): (usize, parley::Affinity), extend: bool, right: bool, layout: Option<TextFlow<'_>>) {
        let (a, b) = self.range();
        if !extend && a != b {
            let to = self.collapse_to(layout, right);
            self.anchor = to;
            self.focus = to;
            self.affinity = parley::Affinity::Downstream;
        } else {
            self.set(i, extend);
            if self.focus == i { self.affinity = side; }
        }
        self.typing = Typing::Inherited;
    }

    /// The end of the selection Left (`!right`) or Right collapses it to, as Notepad and Word do:
    /// the end drawn further that way when both sit on one line, so Left in an Arabic line goes
    /// to its logical end. Across lines, the end the arrow steps toward in the focus's paragraph.
    fn collapse_to(&self, layout: Option<TextFlow<'_>>, right: bool) -> usize {
        let (a, b) = self.range();
        let Some(l) = layout.filter(|_| !self.is_masked()) else { return if right { b } else { a } };
        let (ca, cb) = (l.caret(self.display_index(a), parley::Affinity::Downstream, 0.0), l.caret(self.display_index(b), parley::Affinity::Upstream, 0.0));
        if (ca.y0 - cb.y0).abs() < 1.0 {
            return if (ca.x0 <= cb.x0) != right { a } else { b };
        }
        let forward = right == (l.piece_at(self.display_index(self.focus)).flow == crate::el::Flow::Ltr);
        if forward { b } else { a }
    }

    /// Applies one key. `layout` is the field's layout for the current text (device px).
    pub fn key(&mut self, ev: &KeyEvent, layout: Option<&Layout<Ink>>) -> EditOutcome {
        self.key_in(ev, layout.map(TextFlow::Single))
    }

    /// [`Self::key`] with the field's layout as it draws it, one paragraph's or a by-paragraph field's.
    pub(crate) fn key_in(&mut self, ev: &KeyEvent, layout: Option<TextFlow<'_>>) -> EditOutcome {
        let mut out = EditOutcome::default();
        if matches!(ev.key, Key::Modifier { .. }) { return out; }
        // Any key but Up and Down lets the column go.
        let column = std::mem::take(&mut self.column);
        self.sync_invariants();
        let m = ev.mods;
        self.normalize_hidden_selection();
        if self.is_hidden_multiline() && matches!(ev.key, Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End) {
            self.focus = self.text.len();
            self.anchor = if m.shift { 0 } else { self.focus };
            return out;
        }
        if let Some(c) = ev.control_binding() {
            let (key, line) = match c {
                'a' => (Key::Left, true), 'e' => (Key::Right, true), 'b' => (Key::Left, false),
                'f' => (Key::Right, false), 'p' => (Key::Up, false), 'n' => (Key::Down, false),
                'd' => (Key::Delete, false), 'h' => (Key::Backspace, false),
                _ => {
                    // Ctrl-K deletes to the end of the line, or the line break at its end.
                    self.break_history_group();
                    if self.is_hidden_multiline() && self.is_pristine() {
                        self.insert("");
                        return out;
                    }
                    let (a, b) = self.range();
                    let end = if a == b { self.line_edge(layout, true) } else { b };
                    let to = if end > a { end } else if a < self.text.len() { next_char(&self.text, a) } else { a };
                    if to > a { self.replace((a, to), "", EditKind::DeleteRun); }
                    return out;
                }
            };
            self.column = column;
            return self.key_in(&KeyEvent { key, mods: Mods { shift: m.shift, cmd: line, ..Mods::default() } }, layout);
        }
        if ev.is_command_chord() || ev.is_window_menu_chord() { return out; }
        // Typing, deleting and line breaks join the open undo step (`history::step`); any other key closes it.
        if !matches!(&ev.key, Key::Text(_) | Key::Backspace | Key::Delete | Key::Enter) { self.break_history_group(); }
        // ⌘ moves to a line's edge on macOS. Elsewhere Ctrl, which also sets `cmd`, moves by
        // word as Option does on macOS; `cmd` with no Ctrl (a script) keeps the line edge.
        let word = m.alt || (m.ctrl && !cfg!(target_os = "macos"));
        let line = m.cmd && (cfg!(target_os = "macos") || !m.ctrl);
        match &ev.key {
            Key::Text(s) => {
                if s.chars().any(|c| !c.is_control() || (!self.is_single_line() && matches!(c, '\n' | '\r' | '\t'))) {
                    let s = self.sanitize(s);
                    if !s.is_empty() { self.replace(self.range(), &s, if s.contains(['\r', '\n']) { EditKind::LineBreak } else { EditKind::Text }); }
                }
            }
            // A composition's Return and Escape are the input method's.
            Key::Enter | Key::Escape if self.preedit().is_some() => {}
            Key::Enter => {
                if self.is_single_line() {
                    out.submit = true;
                } else if self.secure {
                    if m.cmd || m.ctrl {
                        out.submit = true;
                    } else {
                        self.replace(self.range(), "\n", EditKind::LineBreak);
                    }
                } else {
                    let (a, b) = self.range();
                    let line_idx = self.line_of_byte(a);
                    let blk = self.blocks.get(line_idx).copied().unwrap_or(Block::Paragraph);
                    let line_range = self.line_range(line_idx);
                    if m.shift && self.rich {
                        // Shift+Return: soft line break keeping the current paragraph's block.
                        self.insert_newline(blk, true);
                    } else if m.shift || m.alt {
                        self.insert_newline(blk, false);
                    } else {
                        match blk {
                            Block::Code => {
                                // Return on a blank last line leaves the block, as on an empty
                                // list item; anywhere else it inserts a newline.
                                if a == b && line_range.is_empty() && self.blocks.get(line_idx + 1) != Some(&Block::Code) {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    self.insert_newline(Block::Code, false);
                                }
                            }
                            Block::Bullet(_) | Block::Numbered(_) => {
                                // Return in an empty list item ends the list; otherwise continues it.
                                if a == b && line_range.is_empty() {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    self.insert_newline(blk, false);
                                }
                            }
                            Block::Quote => {
                                if a == b && line_range.is_empty() {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    self.insert_newline(Block::Quote, false);
                                }
                            }
                            // A paragraph of a document breaks; a composer's submits.
                            Block::Paragraph if self.attributed() => self.insert_newline(blk, false),
                            Block::Paragraph => {
                                out.submit = true;
                            }
                        }
                    }
                }
            }
            Key::Tab => {
                if !m.cmd && !m.alt && !m.ctrl && !self.secure && !self.is_single_line() {
                    if self.active_block() == Block::Code && !m.shift {
                        self.replace(self.range(), "\t", EditKind::Text);
                    } else if m.shift {
                        self.outdent_list();
                    } else {
                        self.indent_list();
                    }
                }
            }
            Key::PageUp | Key::PageDown => {}
            Key::Escape => out.blur = true,
            Key::Backspace => {
                let (a, b) = self.range();
                if a != b || (self.is_hidden_multiline() && self.is_pristine()) {
                    self.replace(self.range(), "", EditKind::Delete);
                } else {
                    let line_idx = self.line_of_byte(a);
                    let line_range = self.line_range(line_idx);
                    let blk = self.blocks.get(line_idx).copied().unwrap_or(Block::Paragraph);
                    let is_soft = self.soft_breaks.binary_search(&(line_idx as u32)).is_ok();
                    if !line && !word && a == line_range.start && !is_soft && blk != Block::Paragraph {
                        match blk {
                            Block::Bullet(lvl) | Block::Numbered(lvl) => {
                                if lvl > 0 {
                                    self.set_block(blk.with_level(lvl - 1));
                                } else {
                                    self.set_block(Block::Paragraph);
                                }
                            }
                            Block::Quote | Block::Code => {
                                if a == 0 || line_range.is_empty() {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    let from = prev_backspace(&self.text, a);
                                    self.replace((from, a), "", EditKind::Delete);
                                }
                            }
                            Block::Paragraph => unreachable!(),
                        }
                    } else if let Some(atom) = self.atom_beside(a, false) {
                        self.select_atom(atom);
                    } else if a > 0 {
                        let from = if line {
                            if self.is_masked() { 0 } else { self.text[..a].rfind('\n').map_or(0, |i| i + 1) }
                        } else if word {
                            self.prev_word(a)
                        } else {
                            prev_backspace(&self.text, a)
                        };
                        self.replace((from, a), "", if line || word { EditKind::DeleteRun } else { EditKind::Delete });
                    }
                }
            }
            Key::Delete => {
                let (a, b) = self.range();
                if a != b || (self.is_hidden_multiline() && self.is_pristine()) {
                    self.replace(self.range(), "", EditKind::Delete);
                } else if let Some(atom) = self.atom_beside(a, true) {
                    self.select_atom(atom);
                } else if a < self.text.len() {
                    let to = if word { self.next_word(a) } else { next_char(&self.text, a) };
                    self.replace((a, to), "", if word { EditKind::DeleteRun } else { EditKind::Delete });
                }
            }
            Key::Left if !line && let Some(to) = self.visual_step(layout, false, word) => self.move_h(to, m.shift, false, layout),
            Key::Left => {
                let i = if line { self.line_edge(layout, false) } else if word { self.prev_word(self.focus) } else { prev_char(&self.text, self.focus) };
                self.move_h((i, parley::Affinity::Downstream), m.shift, false, layout);
            }
            Key::Right if !m.shift && !m.cmd && !m.alt && !m.ctrl && self.at_end_of_trailing_block() => self.leave_trailing_block(),
            Key::Down if !m.shift && !m.cmd && !m.alt && !m.ctrl && self.at_end_of_trailing_block() => self.leave_trailing_block(),
            Key::Right if !line && let Some(to) = self.visual_step(layout, true, word) => self.move_h(to, m.shift, true, layout),
            Key::Right => {
                let i = if line { self.line_edge(layout, true) } else if word { self.next_word(self.focus) } else { next_char(&self.text, self.focus) };
                self.move_h((i, parley::Affinity::Downstream), m.shift, true, layout);
            }
            // Home and End go to the text's ends on macOS. Elsewhere they go to the line's
            // edge, and with Ctrl to the text's ends.
            Key::Home | Key::End => {
                let end = ev.key == Key::End;
                let i = if cfg!(target_os = "macos") || m.ctrl { if end { self.text.len() } else { 0 } } else { self.line_edge(layout, end) };
                self.set(i, m.shift)
            }
            Key::Up | Key::Down => {
                let down = ev.key == Key::Down;
                let n = self.text.len();
                // A run of Up and Down keeps the column it started in, as a browser's caret does
                // through shorter lines (and lines of the other direction).
                let mut kept = None;
                let i = if line || self.is_masked() {
                    if down { n } else { 0 }
                } else if let Some(l) = layout {
                    match l.move_line(self.display_index(self.focus), down, column.goal()) {
                        Some((j, x)) => {
                            kept = Some(x);
                            let j = self.model_index(j);
                            if j == self.focus { if down { n } else { 0 } } else { j }
                        }
                        None => n,
                    }
                } else if down {
                    n
                } else {
                    0
                }
                .min(n);
                if m.shift {
                    self.focus = i;
                    self.snap_to_pills();
                    self.typing = Typing::Inherited;
                } else {
                    self.set_caret(i);
                }
                self.column = kept.map_or(Column::Free, Column::Kept);
            }
            // Only the plain chord: with another modifier it's the app's (Ctrl+Alt+Z).
            Key::Command(_) => match ev.edit_command() {
                Some('z') if m.shift => self.redo(),
                Some('z') => self.undo(),
                Some('y') if !cfg!(target_os = "macos") => self.redo(),
                Some('a') => {
                    self.anchor = 0;
                    self.focus = self.text.len();
                    self.typing = Typing::Inherited;
                }
                Some('b' | 'i' | 'u') => {
                    if let Some(mark) = self.format_chord(ev) {
                        self.toggle_mark(mark);
                    }
                }
                Some('c') if !self.secure => out.copy = Some(self.selected().to_owned()),
                Some('x') if !self.secure => {
                    out.copy = Some(self.selected().to_owned());
                    self.insert("");
                }
                _ => {}
            },
            Key::Function(_) | Key::ContextMenu | Key::Insert | Key::PrintScreen | Key::ScrollLock | Key::Pause | Key::Modifier { .. } => {}
        }
        out
    }

    /// The caret is at the end of the text, in a code block or quote there that it can't move
    /// out of by itself.
    fn at_end_of_trailing_block(&self) -> bool {
        let n = self.text.len();
        !self.secure
            && !self.is_single_line()
            && self.preedit().is_none()
            && (self.anchor, self.focus) == (n, n)
            && matches!(self.blocks.last(), Some(Block::Code | Block::Quote))
    }

    /// Adds an empty paragraph after a trailing code block or quote and puts the caret in it:
    /// one undo step.
    fn leave_trailing_block(&mut self) {
        self.sync_invariants();
        self.break_history_group();
        let before = self.snapshot();
        let n = self.text.len();
        self.typing = Typing::Inherited;
        self.apply_text_and_attr_replace((n, n), "\n", &[], Some(Block::Paragraph), false);
        self.record_change(before, EditKind::Other);
    }

    /// Where Left (Right when `right`) moves the caret, by a character or a `word`, when the
    /// text moves visually ([`CaretMotion`]): across the line as it's drawn, and at the line's
    /// edge on the way its paragraph runs, so Left leaves a right-to-left line forward. `None`
    /// when it moves by the logical order.
    fn visual_step(&self, layout: Option<TextFlow<'_>>, right: bool, word: bool) -> Option<(usize, parley::Affinity)> {
        use super::caret_motion::{Arrow, Landing, visual_in};
        let arrow = match (right, word) {
            (false, false) => Arrow::Left,
            (true, false) => Arrow::Right,
            (false, true) => Arrow::WordLeft,
            (true, true) => Arrow::WordRight,
        };
        let l = layout?;
        let hidden = self.is_masked();
        let (landing, flow) = visual_in(l, self.display_index(self.focus), self.affinity, arrow, hidden)?;
        Some(match landing {
            // Off macOS a word forward goes on past the spaces after it, as Chromium's lands at the
            // next word's start: to the right where the text runs left to right (Ctrl+Right), to
            // the left where it runs right to left (Ctrl+Left).
            Landing::At(mut j, mut side) if word && !cfg!(target_os = "macos") => {
                let step = if right { Arrow::Right } else { Arrow::Left };
                while super::caret_motion::space_ahead(l, j, side, right)
                    && let Some((Landing::At(k, s), _)) = visual_in(l, j, side, step, hidden)
                {
                    (j, side) = (k, s);
                }
                (self.model_index(j), side)
            }
            Landing::At(j, side) => (self.model_index(j), side),
            Landing::Edge => {
                let forward = right == (flow == crate::el::Flow::Ltr);
                let i = match (forward, word) {
                    (true, false) => next_char(&self.text, self.focus),
                    (true, true) => self.next_word(self.focus),
                    (false, false) => prev_char(&self.text, self.focus),
                    (false, true) => self.prev_word(self.focus),
                };
                (i, parley::Affinity::Downstream)
            }
        })
    }
    fn line_edge(&self, layout: Option<TextFlow<'_>>, end: bool) -> usize {
        if self.is_masked() {
            return if end { self.text.len() } else { 0 };
        }
        if let Some(l) = layout {
            return self.model_index(l.line_edge(self.display_index(self.focus), end));
        }
        if end {
            self.text[self.focus..].find('\n').map_or(self.text.len(), |i| self.focus + i)
        } else {
            self.text[..self.focus].rfind('\n').map_or(0, |i| i + 1)
        }
    }

    pub fn ime(&mut self, ev: &Ime) -> EditOutcome {
        if self.secure { self.cancel_composition(); return EditOutcome::default(); }
        self.sync_invariants();
        for effect in ime::step(&mut self.composition, ime::Event::Ime(ev)) {
            match effect {
                ime::Effect::Commit(mut s) => {
                    self.commit_composed(&s);
                    s.zeroize();
                }
                // Reconversion: the composition that follows replaces the selection.
                ime::Effect::Select(range) => {
                    let (a, b) = (Self::clamp_char_boundary(&self.text, range.start), Self::clamp_char_boundary(&self.text, range.end));
                    self.break_history_group();
                    self.set_selection(a, b);
                }
            }
        }
        EditOutcome::default()
    }

    /// Writes a composition's text over the selection as typing: it joins the open undo step,
    /// as each Hangul syllable or kana phrase a person commits is part of what they type, the
    /// way a browser's field groups them.
    pub(super) fn commit_composed(&mut self, s: &str) {
        let clean = self.sanitize(s);
        if clean.is_empty() { return; }
        let kind = if clean.contains(['\r', '\n']) { EditKind::LineBreak } else { EditKind::Text };
        self.replace(self.range(), &clean, kind);
    }

    pub fn paste(&mut self, s: &str) -> EditOutcome {
        self.normalize_hidden_selection();
        let s = self.sanitize(s);
        self.insert(&s);
        EditOutcome::default()
    }

    /// Paste rich HTML (`html`) with plain-text fallback (`plain`) into `self` at the current
    /// selection, recorded as a single undo step. Does not touch the OS clipboard. A plain field,
    /// or HTML with no marks or blocks this model keeps, pastes `plain` instead. Pills
    /// (`<span data-pill="id">`) paste as their plain labels: a field restores them only from
    /// its app's own copies, and an app's paste hook can with
    /// [`paste_html_with_pills`](Self::paste_html_with_pills).
    pub fn paste_html(&mut self, html_str: &str, plain: &str) -> EditOutcome {
        self.paste_rich(html_str, plain, false)
    }

    /// [`paste_html`](Self::paste_html), keeping the HTML's [`Mark::Pill`]s.
    pub fn paste_html_with_pills(&mut self, html_str: &str, plain: &str) -> EditOutcome {
        self.paste_rich(html_str, plain, true)
    }

    fn paste_rich(&mut self, html_str: &str, plain: &str, pills: bool) -> EditOutcome {
        if self.secure || html_str.trim().is_empty() {
            return self.paste(plain);
        }
        let mut frag = self.html_fragment(html_str);
        if !pills {
            frag.marks.retain(|m| !m.mark.is_pill());
        }
        // An atom is the app's to restore, from a fragment: here its label is text.
        for attrs in &mut frag.attrs {
            if attrs.atom.is_some() {
                (attrs.atom, attrs.id) = (None, 0);
            }
        }
        self.paste_fragment(frag, plain)
    }

    /// What a paste of `html_str` would insert, as a field of its own: the text, the marks with
    /// the pills among them, the blocks and the paragraph attributes. An app that keeps content
    /// behind pills and atoms reads it in its paste hook, takes out what it has no content for
    /// or gives a pill an id of its own, and hands the result to
    /// [`paste_fragment`](Self::paste_fragment).
    pub fn html_fragment(&self, html_str: &str) -> TextEdit {
        html::parse_html(html_str, self.html_styles()).rich()
    }

    /// Pastes `frag`, a fragment [`html_fragment`](Self::html_fragment) made, with the plain
    /// text `plain` where the field keeps none of its formatting, as one edit. It keeps the
    /// fragment's pills, and its atoms as atoms: each on a line of its own, with a new id from
    /// [`set_paragraph_ids`](Self::set_paragraph_ids), in the fragment's order.
    pub fn paste_fragment(&mut self, mut frag: TextEdit, plain: &str) -> EditOutcome {
        if self.secure {
            return self.paste(plain);
        }
        // Only a field that holds a document keeps a paste's fonts, sizes, colours and
        // paragraph attributes. Any other field takes a code editor's coloured copy as the
        // plain text it is.
        if !self.attributed() {
            frag.marks.retain(|m| !matches!(m.mark, Mark::Size(_) | Mark::Color(_) | Mark::Family(_) | Mark::Highlight(_) | Mark::Baseline(_)));
            frag.attrs.clear();
        }
        // HTML that only colours or lays out text (a code editor's copy) would collapse the
        // plain text's whitespace for nothing.
        let mut frag = self.keep_allowed(frag);
        let unformatted = frag.marks.is_empty() && frag.attrs.is_empty() && frag.blocks.iter().all(|&b| b == Block::Paragraph);
        if !plain.is_empty() && (frag.text.is_empty() || unformatted) {
            return self.paste(plain);
        }
        if !self.rich {
            return self.paste(if plain.is_empty() { frag.text.as_str() } else { plain });
        }
        self.sync_invariants();
        // A code block takes the plain text, whose whitespace HTML would have collapsed.
        if !self.is_single_line() {
            let (a, _) = self.range();
            if self.blocks.get(self.line_of_byte(a)) == Some(&Block::Code) {
                return self.paste(if plain.is_empty() { frag.text.as_str() } else { plain });
            }
        }
        self.break_history_group();
        let before = self.snapshot();
        let (a, b) = self.range();

        if self.is_single_line() {
            // Single-line field: strip line breaks from `frag.text` while keeping inline marks.
            let mut clean = String::with_capacity(frag.text.len());
            let mut byte_map = vec![0usize; frag.text.len() + 1];
            for (i, ch) in frag.text.char_indices() {
                byte_map[i] = clean.len();
                if !ch.is_control() && !matches!(ch, '\u{2028}' | '\u{2029}') {
                    clean.push(ch);
                } else if ch == '\n' && !clean.ends_with(' ') && !clean.is_empty() {
                    clean.push(' ');
                }
            }
            byte_map[frag.text.len()] = clean.len();
            self.apply_text_and_attr_replace((a, b), &clean, &[], None, false);
            for m in &frag.marks {
                let s_idx = a + byte_map[m.start.min(frag.text.len())];
                let e_idx = a + byte_map[m.end.min(frag.text.len())];
                if s_idx < e_idx {
                    self.marks.push(MarkRange { start: s_idx, end: e_idx, mark: m.mark.clone() });
                }
            }
            self.typing = Typing::Inherited;
            self.normalize_marks();
            if !frag.marks.is_empty() { self.note_marks(a..a + clean.len()); }
            self.record_change(before, EditKind::Other);
            return EditOutcome::default();
        }

        // First delete `a..b` if non-empty. Selected atoms leave their line to the pasted text,
        // as they do to typed text: only a deletion on its own takes their line break too.
        if a < b {
            let f = self.fence(a, b, "");
            let (a, b) = if f.text.is_some() { (f.a, f.b) } else { (a, b) };
            self.apply_fenced((a, b), f.text.as_deref().unwrap_or(""), &[], None, false, f.back);
        }
        // Where the caret landed.
        let at = self.range().0;
        // An atom the fragment starts or ends with stays on a line of its own: behind a line
        // break unless the line starts here, and before one unless it ends here. Beside an
        // atom of the field the fence below does the same.
        let frag_atom = |frag: &TextEdit, line: usize| frag.attrs.get(line).is_some_and(|a| a.atom.is_some());
        if frag.attrs.iter().any(|a| a.atom.is_some()) {
            let here = self.line_of_byte(at);
            let (range, beside_atom) = (self.line_range(here), self.paragraph(here).atom.is_some());
            if frag_atom(&frag, frag.blocks().len() - 1) && at < range.end && !beside_atom {
                frag.set_caret(frag.text.len());
                frag.insert("\n");
            }
            if frag_atom(&frag, 0) && at > range.start && !beside_atom {
                frag.set_caret(0);
                frag.insert("\n");
            }
        }
        // Beside an atom the text goes on lines of its own, behind a line break (`lead`) or
        // before one. Its marks and blocks go where its own first byte and line do.
        let fenced = self.fence(at, at, &frag.text);
        let (ins_at, line_idx) = (at + fenced.lead, self.line_of_byte(at) + fenced.lead);
        let own_line = fenced.text.is_some();
        let line_range = if own_line { ins_at..ins_at } else { self.line_range(line_idx) };
        let line_was_empty = line_range.is_empty();
        // What the paste lands beside: nothing before it on its line, nothing after it.
        let (at_line_start, at_line_end) = (ins_at == line_range.start, ins_at == line_range.end);
        let mut orig_block = if own_line { Block::Paragraph } else { self.blocks.get(line_idx).copied().unwrap_or(Block::Paragraph) };

        self.apply_fenced((at, at), fenced.text.as_deref().unwrap_or(&frag.text), &[], None, false, fenced.back);

        // Apply `frag.blocks` and `frag.soft_breaks`.
        let frag_lines = frag.blocks.len();
        // A fragment that ends in a line break, pasted at the start of a line with text,
        // pushes that line down whole: the line keeps its block, its attributes and its id,
        // and the pasted paragraphs above it are new ones.
        let ends_open = frag_lines > 1 && frag.text.ends_with('\n');
        // A locked block (an atom) is not pushed down by swapping attributes: its lock, its id and
        // its height would move onto the pasted text. It stays where the paste's own lines put it.
        let at_atom = |e: &Self, i: usize| e.attrs.get(i).is_some_and(|a| a.atom.is_some());
        if ends_open && at_line_start && !at_line_end && !own_line && !at_atom(self, line_idx) && !at_atom(self, line_idx + frag_lines - 1) {
            let last = line_idx + frag_lines - 1;
            if last < self.attrs.len() {
                self.attrs.swap(line_idx, last);
            }
            if last < self.blocks.len() {
                self.blocks.swap(line_idx, last);
                self.blocks[line_idx] = Block::Paragraph;
            }
            orig_block = Block::Paragraph;
        }
        if frag_lines == 1 {
            if line_was_empty && orig_block == Block::Paragraph && frag.blocks[0] != Block::Paragraph {
                if let Some(slot) = self.blocks.get_mut(line_idx) {
                    *slot = frag.blocks[0];
                }
            }
        } else if frag_lines > 1 {
            if (line_was_empty || ins_at == line_range.start) && orig_block == Block::Paragraph {
                if let Some(slot) = self.blocks.get_mut(line_idx) {
                    *slot = frag.blocks[0];
                }
            }
            // The line a trailing break leaves the caret on keeps the block it had.
            for k in 1..frag_lines - usize::from(ends_open) {
                if let Some(slot) = self.blocks.get_mut(line_idx + k) {
                    *slot = frag.blocks[k];
                }
            }
            for &sb in &frag.soft_breaks {
                let target = line_idx + sb as usize;
                if target > 0 {
                    self.soft_breaks.push(target as u32);
                }
            }
        }

        // The pasted paragraphs keep their style and alignment. The first and the last join
        // the paragraph they land in, and take its attributes unless it holds nothing on
        // that side, as a word processor pastes. Ids stay the field's own, and a pasted atom
        // takes a new one.
        let mut styled = false;
        if !frag.attrs.is_empty() {
            for k in 0..frag_lines {
                let joins = (k == 0 && !at_line_start) || (k + 1 == frag_lines && !at_line_end);
                if joins && !(frag_lines == 1 && line_was_empty) { continue; }
                let from = frag.paragraph(k);
                // A pasted atom is a new paragraph whatever line it lands on: its id never
                // named text.
                let atom_id = from.atom.map(|_| self.fresh_id());
                if let Some(slot) = self.attrs_mut().get_mut(line_idx + k) {
                    let was = *slot;
                    (slot.style, slot.align) = (from.style, from.align);
                    if let Some(id) = atom_id {
                        (slot.atom, slot.id) = (from.atom, if id == 0 { slot.id } else { id });
                    }
                    styled |= *slot != was;
                }
            }
        }

        // Add `frag.marks` shifted by `ins_at`.
        for m in &frag.marks {
            self.marks.push(MarkRange {
                start: ins_at + m.start,
                end: ins_at + m.end,
                mark: m.mark.clone(),
            });
        }
        self.typing = Typing::Inherited;
        self.sync_invariants();
        // The pasted text's marks and blocks, as one change over it.
        let end = (ins_at + frag.text.len()).min(self.text.len());
        if !frag.marks.is_empty() { self.note_marks(ins_at..end); }
        if styled || frag_lines > 1 || frag.blocks.first().is_some_and(|&b| b != Block::Paragraph) {
            self.note_paragraphs(line_idx..line_idx + frag_lines.max(1));
        }
        self.record_change(before, EditKind::Other);
        EditOutcome::default()
    }
}
