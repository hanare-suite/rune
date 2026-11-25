//! The measures a paragraph layout keeps for a line with no text in it.

use super::*;

impl TextSystem {
    /// What an empty line in `style`, `line` px tall, measures: the room a newline takes at a
    /// line's end, and the box a blank line's caret and selection use (its baseline, ascent and
    /// descent).
    pub(super) fn blank_line(&mut self, style: TextStyle, scale: f32, line: f32) -> (f32, [f32; 3]) {
        let (asc, desc) = self.primary_extents(style.family, style.weight, style.italic).unwrap_or((0.8, 0.2));
        let (asc_px, desc_px) = (asc * style.size * scale, desc.abs() * style.size * scale);
        let (qa, qd) = (asc_px.round(), desc_px.round());
        (0.25 * (asc_px + desc_px), [qa + ((line - (qa + qd)) * 0.5).floor(), asc_px, desc_px])
    }
}
