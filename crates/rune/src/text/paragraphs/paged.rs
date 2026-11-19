//! Page flow for a by-paragraph field ([`El::page_flow`](crate::El::page_flow)): where each
//! paragraph goes on its page, and which of its lines move to the next one.
//!
//! A paragraph that crosses a page's end keeps one layout. Its lines after the end are broken
//! again with the room to the next page's top above them (`line_break::rebreak_gapped`), so
//! everything that reads a layout's lines (the caret, a press, selection boxes, glyphs) finds
//! them on their page with no mapping of its own.

use super::*;

/// A field's pages in device px.
#[derive(Clone, Copy)]
pub(super) struct Pages {
    height: f64,
    /// A page and the gap after it.
    stride: f64,
}

impl Pages {
    pub(super) fn of(spec: &TextSpec, scale: f32) -> Option<Pages> {
        let flow = spec.cold.page_flow?;
        let height = f64::from((flow.height * scale).round().max(1.0));
        Some(Pages { height, stride: height + f64::from((flow.gap * scale).round().max(0.0)) })
    }

    fn top(self, y: f64) -> f64 {
        (y / self.stride).floor() * self.stride
    }

    /// The height of a field whose last line ends at `bottom`: whole pages and their gaps.
    pub(super) fn field_height(self, bottom: f64) -> f64 {
        self.top((bottom - 1.0).max(0.0)) + self.height
    }
}

/// Where a paragraph's layout starts, and the lines of it that start a page: each line's
/// index and the device px its top moves down by.
pub(super) struct Placed {
    pub(super) top: f64,
    pub(super) gaps: Vec<(u32, f32)>,
}

impl Placed {
    /// What the gaps add to the layout's height.
    pub(super) fn added(&self) -> f32 {
        self.gaps.iter().map(|g| g.1).sum()
    }
}

/// Places a paragraph `height` px tall after what ends at `y`, with `before` px above it.
/// `forced` starts it on a new page. `lines` gives its lines' tops and bottoms in its layout,
/// and is asked only when it doesn't fit what is left of its page. The room above is dropped
/// at the top of a page the paragraph flowed onto, and kept under a forced break.
pub(super) fn place(pages: Option<Pages>, y: f64, first: bool, before: f64, forced: bool, height: f32, lines: impl FnOnce() -> Vec<(f32, f32)>) -> Placed {
    let Some(p) = pages else { return Placed { top: y + before, gaps: Vec::new() } };
    let mut y = y;
    // What came before ended in the gap (its room below ran past the page).
    if y - p.top(y) >= p.height { y = p.top(y) + p.stride; }
    let forced = forced && !first;
    if forced && y > p.top(y) { y = p.top(y) + p.stride; }
    let flowed = y == p.top(y) && !forced && !first;
    let mut top = if flowed { y } else { y + before };
    let mut bottom = p.top(y) + p.height;
    if top + f64::from(height) <= bottom {
        return Placed { top, gaps: Vec::new() };
    }
    let lines = lines();
    let mut gaps = Vec::new();
    let mut shift = 0.0f64;
    for (i, &(min, max)) in lines.iter().enumerate() {
        let (line_top, line_bottom) = (top + shift + f64::from(min), top + shift + f64::from(max));
        // A line alone on its page stays there, however tall.
        if line_bottom <= bottom || line_top <= bottom - p.height { continue; }
        let next = bottom - p.height + p.stride;
        if i == 0 {
            top = next - f64::from(min);
        } else {
            let gap = next - line_top;
            gaps.push((i as u32, gap as f32));
            shift += gap;
        }
        bottom = next + p.height;
    }
    Placed { top, gaps }
}

/// A layout's lines' tops and bottoms, one line `height` tall for a layout with none.
pub(super) fn lines_of(layout: &Layout<Ink>, height: f32) -> Vec<(f32, f32)> {
    let lines: Vec<(f32, f32)> = layout.lines().map(|l| (l.metrics().block_min_coord, l.metrics().block_max_coord)).collect();
    if lines.is_empty() { vec![(0.0, height)] } else { lines }
}
