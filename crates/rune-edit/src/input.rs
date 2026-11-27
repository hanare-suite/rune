//! Keys, IME and paste on a [`TextEdit`].

use parley::Layout;

use super::*;
use rune_text::{Flow, Ink, TextFlow};

impl TextEdit {
    /// Whether `frag` is the selection: its text and marks, and for each line it covers whole its
    /// block and its attributes, ids aside.
    fn holds_selection(&self, frag: &TextEdit) -> bool {
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

    /// The line a paste over the selection lands in, the one what is left of the selection's lines
    /// joins: the later line when the selection takes the earlier one whole, from its start into a
    /// later line (an atom, or paragraphs up to a line's start), the first otherwise.
    fn landing_line(&self) -> usize {
        let (a, b) = self.range();
        let (la, lb) = (self.line_of_byte(a), self.line_of_byte(b));
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        if la < lb && a == ra.start && (b == rb.start || self.paragraph(la).atom.is_some()) { lb } else { la }
    }

    /// Inserts `s` at `at` as no one's text: it inherits no marks.
    fn insert_plain_at(&mut self, at: usize, s: &str) {
        self.apply_text_and_attr_replace((at, at), s, &[], None, false);
    }

    fn insert_newline(&mut self, block_for_new_line: Block, soft: bool) {
        if self.secure || self.is_single_line() {
            return;
        }
        self.sync_invariants();
        let before = self.snapshot();
        let (a, b) = self.range();
        // Return over a selected break changes nothing, as typing the break does not.
        if !soft && a < b && &self.text[a..b] == "\n" {
            self.anchor = b;
            self.focus = b;
            self.cancel_composition();
            self.mark_edited();
            return;
        }
        // A soft break is text, not a paragraph break: over a selection it goes in behind the
        // deletion, by the rules for text, in what is left of the paragraphs.
        if soft && a < b {
            let f = self.fence(a, b, "");
            let gap = f.text.as_deref().unwrap_or("");
            if self.text[f.a..f.b] == *gap {
                self.anchor = f.a + gap.len() - f.back;
                self.focus = self.anchor;
            } else {
                self.apply_fenced((f.a, f.b), gap, &[], None, false, f.back);
            }
        }
        let (a, b) = self.range();
        // An atom is never a soft line and never has one: beside it the break is a hard break.
        let asked_soft = soft;
        let soft = soft && self.paragraph(self.line_of_byte(a)).atom.is_none();
        let block_for_new_line = if asked_soft { self.blocks.get(self.line_of_byte(a)).copied().unwrap_or(Block::Paragraph) } else { block_for_new_line };
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
        if self.is_masked() {
            return if right { b } else { a };
        }
        let d = super::caret_motion::collapse_edge(layout, self.display_index(a), self.display_index(b), self.display_index(self.focus), right);
        self.model_index(d)
    }

    /// Applies one key. `layout` is the field's layout for the current text (device px).
    pub fn key(&mut self, ev: &KeyEvent, layout: Option<&Layout<Ink>>) -> EditOutcome {
        self.key_in(ev, layout.map(TextFlow::Single))
    }

    /// [`Self::key`] with the field's layout as it draws it, one paragraph's or a by-paragraph field's.
    #[doc(hidden)]
    pub fn key_in(&mut self, ev: &KeyEvent, layout: Option<TextFlow<'_>>) -> EditOutcome {
        self.key_inner(ev, layout, false)
    }

    /// [`Self::key_in`] for a read-only field: keeps caret navigation, `Shift` selection,
    /// `Cmd/Ctrl+A`, `Cmd/Ctrl+C`, and `Escape` blur while refusing every mutating key.
    #[doc(hidden)]
    pub fn key_in_read_only(&mut self, ev: &KeyEvent, layout: Option<TextFlow<'_>>) -> EditOutcome {
        self.key_inner(ev, layout, true)
    }

    fn key_inner(&mut self, ev: &KeyEvent, layout: Option<TextFlow<'_>>, read_only: bool) -> EditOutcome {
        let mut out = EditOutcome::default();
        if matches!(ev.key, Key::Modifier { .. } | Key::Media(_)) { return out; }
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
                'd' if !read_only => (Key::Delete, false),
                'h' if !read_only => (Key::Backspace, false),
                'd' | 'h' => return out,
                _ if read_only => return out,
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
            return self.key_inner(&KeyEvent { key, mods: Mods { shift: m.shift, cmd: line, ..Mods::default() } }, layout, read_only);
        }
        if ev.is_command_chord() || ev.is_window_menu_chord() { return out; }
        // Typing, deleting and line breaks join the open undo step (`history::step`); any other key closes it.
        if !matches!(&ev.key, Key::Text(_) | Key::Backspace | Key::Delete | Key::Enter) { self.break_history_group(); }
        // ⌘ moves to a line's edge on macOS. Elsewhere Ctrl, which also sets `cmd`, moves by
        // word as Option does on macOS; `cmd` with no Ctrl (a script) keeps the line edge.
        let word = m.alt || (m.ctrl && !cfg!(target_os = "macos"));
        let line = m.cmd && (cfg!(target_os = "macos") || !m.ctrl);
        match &ev.key {
            Key::Text(_) if read_only => {}
            Key::Text(s) => {
                if s.chars().any(|c| !c.is_control() || (!self.is_single_line() && matches!(c, '\n' | '\r' | '\t'))) {
                    let s = self.sanitize(s);
                    if !s.is_empty() { self.replace(self.range(), &s, if s.contains(['\r', '\n']) { EditKind::LineBreak } else { EditKind::Text }); }
                }
            }
            // A composition's Return and Escape are the input method's.
            Key::Enter | Key::Escape if self.preedit().is_some() => {}
            Key::Enter if read_only => {}
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
            Key::Tab if read_only => {}
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
            Key::Backspace if read_only => {}
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
            Key::Delete if read_only => {}
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
            Key::Right if !read_only && !m.shift && !m.cmd && !m.alt && !m.ctrl && self.at_end_of_trailing_block() => self.leave_trailing_block(),
            Key::Down if !read_only && !m.shift && !m.cmd && !m.alt && !m.ctrl && self.at_end_of_trailing_block() => self.leave_trailing_block(),
            Key::Right if !line && let Some(to) = self.visual_step(layout, true, word) => self.move_h(to, m.shift, true, layout),
            Key::Right => {
                let i = if line { self.line_edge(layout, true) } else if word { self.next_word(self.focus) } else { next_char(&self.text, self.focus) };
                self.move_h((i, parley::Affinity::Downstream), m.shift, true, layout);
            }
            // Home and End go to the text's ends on macOS unless `line_home_end` opts into
            // visual line edges (keeping Cmd+Home/End and Ctrl+Home/End on the text's ends).
            // Elsewhere they go to the line's edge, and with Ctrl to the text's ends.
            Key::Home | Key::End => {
                let i = self.home_end_target(layout, ev.key == Key::End, m, cfg!(target_os = "macos"));
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
                Some('z') if !read_only && m.shift => self.redo(),
                Some('z') if !read_only => self.undo(),
                Some('y') if !read_only && !cfg!(target_os = "macos") => self.redo(),
                Some('a') => {
                    self.anchor = 0;
                    self.focus = self.text.len();
                    self.typing = Typing::Inherited;
                }
                Some('b' | 'i' | 'u') if !read_only => {
                    if let Some(mark) = self.format_chord(ev) {
                        self.toggle_mark(mark);
                    }
                }
                Some('c') if !self.secure => out.copy = Some(self.selected().to_owned()),
                Some('x') if !read_only && !self.secure => {
                    out.copy = Some(self.selected().to_owned());
                    self.insert("");
                }
                _ => {}
            },
            Key::Function(_) | Key::ContextMenu | Key::Insert | Key::PrintScreen | Key::ScrollLock | Key::Pause | Key::Media(_) | Key::Modifier { .. } => {}
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
    /// text moves visually (`CaretMotion`): across the line as it's drawn, and at the line's
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
                let forward = right == (flow == Flow::Ltr);
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

    /// Target byte offset for `Key::Home` (`!end`) or `Key::End` (`end`) with modifiers `m` on
    /// `macos`: document start/end by default on macOS, or visual line start/end when
    /// `line_home_end` is enabled and neither Cmd nor Ctrl is held.
    pub(crate) fn home_end_target(&self, layout: Option<TextFlow<'_>>, end: bool, m: Mods, macos: bool) -> usize {
        if super::caret_motion::home_end_is_doc(m, macos, self.line_home_end) {
            if end { self.text.len() } else { 0 }
        } else {
            self.line_edge(layout, end)
        }
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
    /// [`set_paragraph_ids`](Self::set_paragraph_ids), in the fragment's order. A code block takes
    /// the plain text, but a fragment's atoms still go in as atoms on lines of their own: the code
    /// line splits around them and its parts keep the Code block. A fragment that holds what the
    /// selection holds (its text, marks, and for the lines it covers whole their blocks and
    /// attributes, ids aside) changes nothing. A copy through [`selected_html`](Self::selected_html) holds the
    /// selection exactly where the HTML can say it; it cannot say an indent, a page break, a paragraph style on
    /// a list item, quote or code line, so a paste of such a copy follows the rules instead.
    pub fn paste_fragment(&mut self, mut frag: TextEdit, plain: &str) -> EditOutcome {
        if self.secure {
            return self.paste(plain);
        }
        // Only a field that holds a document keeps a paste's fonts, sizes, colours and
        // paragraph attributes: its allowed marks (`keep_allowed`) and its own attributes say
        // so. Any other field takes a code editor's coloured copy as the plain text it is.
        if !self.attributed() {
            frag.attrs.clear();
        }
        // HTML with no formatting this field keeps (such as layout-only markup, or a code
        // editor's copy on a field that has not opted into colour) would collapse plain
        // whitespace for nothing.
        let mut frag = self.keep_allowed(frag);
        let unformatted = frag.marks.is_empty() && frag.attrs.is_empty() && frag.blocks.iter().all(|&b| b == Block::Paragraph);
        if !plain.is_empty() && (frag.text.is_empty() || unformatted) {
            return self.paste(plain);
        }
        // A fragment with nothing to keep is the text it holds.
        if plain.is_empty() && unformatted {
            // The text is no one's: it takes no marks from the caret (pasting what is selected
            // changes nothing, marks included).
            let (a, b) = self.range();
            if self.text[a..b] != frag.text {
                self.typing = Typing::Armed { at: (self.anchor, self.focus), marks: Vec::new() };
            }
            return self.paste(&frag.text);
        }
        if !self.rich {
            return self.paste(if plain.is_empty() { frag.text.as_str() } else { plain });
        }
        self.sync_invariants();
        // A code block takes the plain text, whose whitespace HTML would have collapsed; a
        // fragment's atoms still go in as atoms, on lines of their own, and the code line splits
        // around them.
        let atoms = frag.attrs.iter().any(|a| a.atom.is_some());
        let landing = self.landing_line();
        let in_code = !self.is_single_line() && self.blocks.get(landing) == Some(&Block::Code) && self.paragraph(landing).atom.is_none();
        if in_code && !atoms {
            return self.paste(if plain.is_empty() { frag.text.as_str() } else { plain });
        }
        // A paste that holds what the selection holds changes nothing, as typing it does not.
        if self.holds_selection(&frag) {
            self.anchor = self.range().1;
            self.focus = self.anchor;
            self.cancel_composition();
            self.mark_edited();
            return EditOutcome::default();
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

        // Where the selection lies against the lines it touches. What it takes joins them into
        // one line, the line the paste lands in.
        let (la, lb) = (self.line_of_byte(a), self.line_of_byte(b));
        let (ra, rb) = (self.line_range(la), self.line_range(lb));
        let recv_start = a == ra.start;
        // A paragraph the selection takes from its start into a later line, whose text the paste
        // does not join: a fragment atom first in the paste takes its place.
        let consumed_first = la < lb && recv_start;
        let empty_line = recv_start && b == rb.end;
        let recv_line = self.landing_line();
        let recv = (self.paragraph(recv_line), self.blocks.get(recv_line).copied().unwrap_or(Block::Paragraph));
        let mut without_id = recv.0;
        without_id.id = 0;
        // Only a single line pasted into an empty plain line gives it the pasted paragraph's
        // attributes; every other line the paste lands in keeps its own.
        let plain_single = frag.blocks.len() == 1 && empty_line && without_id == ParagraphAttrs::default() && recv.1 == Block::Paragraph;
        // A fragment's atom is an atom on a line of its own, never an existing paragraph's text:
        // where its line would join what is left of a line (the first or the last the paste
        // leaves, as the edit sees them), a line break goes on that side, so the line stays
        // whole. A single line pasted into an empty plain line takes the line.
        let frag_atom = |frag: &TextEdit, line: usize| frag.attrs.get(line).is_some_and(|a| a.atom.is_some());
        let mut fenced = self.fence(a, b, &frag.text);
        if frag.attrs.iter().any(|a| a.atom.is_some()) {
            for _ in 0..4 {
                let new = self.new_lines_of((fenced.a, fenced.b), fenced.text.as_deref().unwrap_or(&frag.text));
                let (last, joined) = (frag.blocks.len() - 1, |k: usize| !new.get(fenced.lead + k).copied().unwrap_or(false));
                // Text before the paste on its line, or an atom behind it: the break goes before an
                // atom line that joins it. A single line with nothing before it takes the break after.
                let before = a > ra.start;
                if frag_atom(&frag, 0) && joined(0) && !plain_single && !consumed_first && (before || last > 0) {
                    // Without marks, whatever the caret inherits: the break is no one's text.
                    frag.insert_plain_at(0, "\n");
                } else if frag_atom(&frag, last) && (joined(last) || (last == 0 && joined(0))) && !plain_single {
                    frag.insert_plain_at(frag.text.len(), "\n");
                } else {
                    break;
                }
                fenced = self.fence(a, b, &frag.text);
            }
        }
        // The paste is one edit of the text, as typed text over the selection is: beside an atom
        // the text goes on lines of its own, behind a line break (`lead`) or before one. The
        // lines it adds that are new paragraphs, and not an existing one's text, take the
        // fragment's style, alignment and block; the others keep their own.
        let (fa, fb) = (fenced.a, fenced.b);
        let text = fenced.text.as_deref().unwrap_or(&frag.text);
        let (ins_at, first) = (fa + fenced.lead, self.line_of_byte(fa));
        let new = self.apply_fenced_lines((fa, fb), text, &[], None, false, fenced.back, true);
        let line_idx = first + fenced.lead;
        let frag_lines = frag.blocks.len();
        let is_new = |j: usize| new.get(j).copied().unwrap_or(false);
        let mut styled = false;
        for k in 0..frag_lines {
            let (j, line) = (fenced.lead + k, line_idx + k);
            let from = frag.paragraph(k);
            let consumes = consumed_first && k == 0 && from.atom.is_some() && fenced.lead == 0 && !is_new(j);
            let single = (plain_single && k == 0 && !is_new(j)) || consumes;
            if !is_new(j) && !single { continue; }
            // Text of a fragment goes into a code block as plain text: a line the edit made in one
            // stays a code line, with no style of the fragment's.
            if self.blocks.get(line) == Some(&Block::Code) && from.atom.is_none() { continue; }
            // A pasted atom that takes an empty line gets a new id: its id never named text.
            let atom_id = if single && from.atom.is_some() { self.fresh_id() } else { 0 };
            if self.attributed() {
                if let Some(slot) = self.attrs_mut().get_mut(line) {
                    let was = *slot;
                    if consumes { *slot = ParagraphAttrs::default().id(slot.id); }
                    (slot.style, slot.align) = (from.style, from.align);
                    if from.atom.is_some() {
                        slot.atom = from.atom;
                        if atom_id != 0 { slot.id = atom_id; }
                    }
                    styled |= *slot != was;
                }
            }
            if let Some(slot) = self.blocks.get_mut(line) {
                if !single || consumes || *slot == Block::Paragraph {
                    *slot = frag.blocks[k];
                }
            }
        }
        // An empty paragraph the fragment's last line joins takes that line's block, so a
        // pasted list keeps its last item.
        let last = fenced.lead + frag_lines - 1;
        if empty_line && recv.1 == Block::Paragraph && frag_lines > 1 && !frag.text.ends_with('\n') && !is_new(last) {
            if let Some(slot) = self.blocks.get_mut(line_idx + frag_lines - 1) {
                *slot = frag.blocks[frag_lines - 1];
            }
        }
        if frag_lines > 1 {
            for &sb in &frag.soft_breaks {
                let target = line_idx + sb as usize;
                if target > 0 {
                    self.soft_breaks.push(target as u32);
                }
            }
        }

        // Add `frag.marks` shifted by `ins_at`.
        for m in frag.marks.iter().filter(|_| !in_code) {
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
        if !frag.marks.is_empty() && !in_code { self.note_marks(ins_at..end); }
        if styled || frag_lines > 1 || frag.blocks.first().is_some_and(|&b| b != Block::Paragraph) {
            self.note_paragraphs(first..first + new.len().max(frag_lines));
        }
        self.record_change(before, EditKind::Other);
        EditOutcome::default()
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
