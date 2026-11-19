//! What each edit of a [`TextEdit`] changed, for an app that keeps its own document and its own
//! undo history beside the field: the bytes replaced and how many took their place, or the
//! bytes whose marks or paragraph attributes changed.

use super::*;

/// One change an edit made. `range` is in the text as it was before the change, and `len` is
/// how many bytes stand there after it: `range.start..range.start + len` of the text the
/// change left.
///
/// # Several changes
///
/// One event can report several changes. A rich paste over a selection reports the deletion,
/// the insertion, the marks it set and the paragraphs it set, and changes left untaken over
/// several edits queue up the same way. They come in the order they happened, and each `range`
/// is in the text the change before it left: not in the text before the first, and not in the
/// text now.
///
/// A change carries no text, so that a field nobody takes changes from keeps no copy of what
/// was typed into it. A consumer that follows the field applies the changes in order and
/// reads what each one inserted from the field's text now, after mapping the change's bytes
/// `range.start..range.start + len` forward through every later change `c`: a byte before
/// `c.range.start` stays where it is, and a byte at or past `c.range.end` moves by
/// `c.len - c.range.len()`. A byte inside `c.range` was replaced again and needs no value,
/// since applying `c` overwrites it. [`EditChangeKind::Marks`] and
/// [`EditChangeKind::Paragraphs`] replace no text and move nothing.
///
/// A consumer that only needs to know what to read again can take the changes' union
/// instead: the first changed byte, and the last one in the text before and in the text now.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EditChange {
    pub range: Range<usize>,
    pub len: usize,
    pub kind: EditChangeKind,
}

/// What an [`EditChange`] was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditChangeKind {
    /// Typed text.
    Typed,
    /// A typed line break.
    LineBreak,
    /// Backspace, Delete, or a word or line deleted at once.
    Deleted,
    /// Anything else that replaces text: a paste, a cut, an input method's commit, an atom.
    Replaced,
    /// Marks alone: `range` holds the same text, marked differently.
    Marks,
    /// Paragraph attributes or blocks alone: `range` is the paragraphs they changed on.
    Paragraphs,
}

/// How many changes wait before they fold into one that covers the whole text.
const LOG_CAP: usize = 256;

/// The changes not yet taken. It never compares, hashes or clones: a field cloned for a view
/// starts with none.
#[derive(Default)]
pub(super) struct ChangeLog {
    changes: Vec<EditChange>,
    /// The text's length before the first of them.
    base_len: usize,
}

impl Clone for ChangeLog {
    fn clone(&self) -> Self {
        ChangeLog::default()
    }
}

impl PartialEq for ChangeLog {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl TextEdit {
    /// The changes edits made since the last call, oldest first: what was typed, deleted and
    /// pasted, and changes of marks and paragraph attributes alone. An app that keeps its own
    /// history ([`Self::set_history`]) builds its steps from them. Left untaken they fold into
    /// one change over the whole text past a few hundred, so the list stays small.
    ///
    /// One edit can make several changes, and each one's range is in the text the change
    /// before it left. Apply them in order, and read inserted text from [`Self::text`] as
    /// [`EditChange`] describes.
    ///
    /// The runtime hands the same list to [`El::on_input_edits`](crate::El::on_input_edits)
    /// after each event it routes to the field; call this after edits the app makes itself.
    pub fn take_changes(&mut self) -> Vec<EditChange> {
        std::mem::take(&mut self.changes.changes)
    }

    /// Turns the field's own undo history off (or back on). Off, edits record nothing, undo
    /// and redo do nothing, and their keys go on to the app: the app's history owns undo, fed
    /// by [`Self::take_changes`]. Turning it off drops the steps recorded so far.
    pub fn set_history(&mut self, on: bool) {
        self.history_off = !on;
        if !on {
            self.history = Arc::default();
        }
    }

    /// Whether the field records its own undo history ([`Self::set_history`]).
    pub fn records_history(&self) -> bool {
        !self.history_off
    }

    fn note(&mut self, change: EditChange, old_len: usize) {
        let log = &mut self.changes;
        if log.changes.is_empty() {
            log.base_len = old_len;
        }
        // Too many untaken: they become one change of the whole text, and this one follows it.
        if log.changes.len() >= LOG_CAP {
            log.changes.clear();
            log.changes.push(EditChange { range: 0..log.base_len, len: old_len, kind: EditChangeKind::Replaced });
        }
        log.changes.push(change);
    }

    /// Notes that bytes `range` of the text before became `len` bytes. Call it after the text
    /// changed.
    pub(super) fn note_text(&mut self, range: Range<usize>, len: usize) {
        let old_len = self.text.len() + range.len() - len;
        self.note(EditChange { range, len, kind: EditChangeKind::Replaced }, old_len);
    }

    /// Says what the change just noted was.
    pub(super) fn note_kind(&mut self, kind: EditKind) {
        if let Some(last) = self.changes.changes.last_mut().filter(|c| c.kind == EditChangeKind::Replaced) {
            last.kind = match kind {
                EditKind::Text => EditChangeKind::Typed,
                EditKind::LineBreak => EditChangeKind::LineBreak,
                EditKind::Delete | EditKind::DeleteRun => EditChangeKind::Deleted,
                EditKind::Other => EditChangeKind::Replaced,
            };
        }
    }

    /// Notes that the marks on bytes `range` changed.
    pub(super) fn note_marks(&mut self, range: Range<usize>) {
        let len = range.len();
        self.note(EditChange { range, len, kind: EditChangeKind::Marks }, self.text.len());
    }

    /// Notes that the attributes or blocks of paragraphs `lines` changed.
    pub(super) fn note_paragraphs(&mut self, lines: Range<usize>) {
        let (start, end) = (self.line_range(lines.start).start, self.line_range(lines.end.saturating_sub(1).max(lines.start)).end);
        self.note(EditChange { range: start..end, len: end - start, kind: EditChangeKind::Paragraphs }, self.text.len());
    }
}
