//! Page flow for a by-paragraph field (`El::page_flow`): where each paragraph goes on its page,
//! and which of its lines move to the next one.
//!
//! A paragraph that crosses a page's end keeps one layout. Its lines after the end are broken
//! again with the room to the next page's top above them (`line_break::rebreak_paragraph`), so
//! everything that reads a layout's lines (the caret, a press, selection boxes, glyphs) finds
//! them on their page with no mapping of its own.

use super::*;
use crate::ParagraphAlign;

/// A field's pages in device px.
#[derive(Clone, Copy)]
pub(super) struct Pages {
    height: f64,
    /// A page and the gap after it.
    stride: f64,
    /// Widow and orphan control: no line of a paragraph alone at a page's end or top.
    keep: bool,
}

impl Pages {
    pub(super) fn of(spec: &TextInput<'_>, scale: f32) -> Option<Pages> {
        let flow = spec.flow?.page?;
        let height = f64::from((flow.height * scale).round().max(1.0));
        Some(Pages { height, stride: height + f64::from((flow.gap * scale).round().max(0.0)), keep: flow.keep_lines })
    }

    fn top(self, y: f64) -> f64 {
        (y / self.stride).floor() * self.stride
    }

    /// The height of a field whose last line ends at `bottom`: whole pages and their gaps.
    pub(super) fn field_height(self, bottom: f64) -> f64 {
        self.top((bottom - 1.0).max(0.0)) + self.height
    }
}

/// What a paragraph's attributes ask of its layout, in device px.
#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub(super) align: Alignment,
    /// What every line starts in from the left, and the first line on top of that.
    pub(super) indent: [f32; 2],
    /// A locked atom's height.
    pub(super) atom: Option<f32>,
}

impl Shape {
    /// `attrs` in a field whose own alignment is `align`: a left-aligned paragraph keeps the
    /// field's, and an atom's label is centred.
    pub(super) fn of(attrs: ParagraphAttrs, align: Alignment, scale: f32) -> Shape {
        let align = match (attrs.atom, attrs.align) {
            (Some(_), _) | (None, ParagraphAlign::Center) => Alignment::Center,
            (None, ParagraphAlign::Left) => align,
            (None, ParagraphAlign::Right) => Alignment::Right,
            (None, ParagraphAlign::Justified) => Alignment::Justify,
        };
        Shape { align, indent: [(attrs.indent * scale).round(), (attrs.first_line_indent * scale).round()], atom: attrs.atom.map(|h| (h * scale).round()) }
    }

    pub(super) fn indented(self) -> bool {
        self.indent != [0.0; 2]
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
///
/// A line taller than a page stands alone on its page and runs past its end, and the line
/// after it starts the first page below where it ends: a gap is never negative.
///
/// With [`Pages::keep`], the line that starts a page is never the paragraph's second (its
/// first would stand alone at the page's end: an orphan) nor its last (a widow): one more line
/// goes with it, or the whole paragraph.
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
    let n = lines.len();
    let mut gaps = Vec::new();
    let mut shift = 0.0f64;
    // The line being placed, and the first of the paragraph's lines on the page being filled.
    let (mut i, mut page_first) = (0, 0);
    while i < n {
        let line_top = |k: usize, top: f64, shift: f64| top + shift + f64::from(lines[k].0);
        let line_bottom = top + shift + f64::from(lines[i].1);
        // A line alone on its page stays there, however tall.
        if line_bottom <= bottom || line_top(i, top, shift) <= bottom - p.height {
            i += 1;
            continue;
        }
        let mut j = i;
        if p.keep {
            if j + 1 == n && j >= 2 { j -= 1; }
            if j == 1 && page_first == 0 { j = 0; }
            // A line already at its page's top can't move further.
            if j < i && (j < page_first || line_top(j, top, shift) <= bottom - p.height) { j = i; }
        }
        // The next page's top, or the first one below a line so tall it ran past that: a line
        // never moves up over the one before it.
        let mut next = bottom - p.height + p.stride;
        if j > 0 {
            let at = line_top(j, top, shift);
            if at > next { next += ((at - next) / p.stride).ceil() * p.stride; }
        }
        if j == 0 {
            top = next - f64::from(lines[0].0);
        } else {
            let gap = next - line_top(j, top, shift);
            gaps.push((j as u32, gap as f32));
            shift += gap;
        }
        bottom = next + p.height;
        (page_first, i) = (j, j + 1);
    }
    Placed { top, gaps }
}

/// A layout's lines' tops and bottoms, one line `height` tall for a layout with none.
pub(super) fn lines_of(layout: &Layout<Ink>, height: f32) -> Vec<(f32, f32)> {
    let lines: Vec<(f32, f32)> = layout.lines().map(|l| (l.metrics().block_min_coord, l.metrics().block_max_coord)).collect();
    if lines.is_empty() { vec![(0.0, height)] } else { lines }
}

/// What a rich text's last layout was keyed from, so an edit keys only the paragraphs it
/// touched (`reuse_rich`), as a plain text's does.
pub(crate) struct RichKeyed {
    pub(super) content: Rc<str>,
    pub(super) style_key: u64,
    pub(super) start_flow: Flow,
    pub(super) spans: Vec<Span>,
    /// Each paragraph's block and what of its attributes its layout reads.
    pub(super) lines: Vec<LineKey>,
}

pub(super) type LineKey = (Block, (u16, ParagraphAlign, u32, u32, bool));

/// Paragraph `k`'s block and the attributes its layout reads.
pub(super) fn line_key(spec: &TextInput<'_>, k: usize) -> LineKey {
    (spec.blocks.get(k).copied().unwrap_or(Block::Paragraph), spec.flow.and_then(|f| f.attrs.get(k)).copied().unwrap_or_default().layout_bits())
}

/// `prev`'s paragraphs that `spec` keeps as they were, when `prev` was this field's rich text
/// in the same styles: their words byte for byte, the spans over them, their blocks and their
/// attributes. Everything from the first byte or span that differs to the last is keyed again.
pub(super) fn reuse_rich(prev: &ParagraphLayout, spec: &TextInput<'_>, style_key: u64, start_flow: Flow) -> Option<Reuse> {
    let keyed = prev.rich.as_ref().filter(|k| k.style_key == style_key && k.start_flow == start_flow)?;
    let (old, new) = (keyed.content.as_bytes(), spec.content.as_bytes());
    let prefix = common_start(old, new);
    let suffix = common_end(old, new).min(old.len().min(new.len()) - prefix);
    let delta = new.len() as isize - old.len() as isize;
    // The bytes of the old text an edit touched: `lo..hi`. A span that changed widens them.
    let (mut lo, mut hi) = (prefix, old.len() - suffix);
    let (so, sn) = (keyed.spans.as_slice(), spec.spans);
    let moved = |at: u32| (at as isize + delta) as u32;
    let p = so.iter().zip(sn).take_while(|(a, b)| a == b).count();
    let q = so[p..].iter().rev().zip(sn[p..].iter().rev())
        .take_while(|(a, b)| moved(a.start) == b.start && moved(a.end) == b.end && a.same_style(b)).count();
    let old_at = |at: u32| if (at as usize) <= prefix { at as usize } else { ((at as isize - delta).max(prefix as isize)) as usize };
    for s in &so[p..so.len() - q] {
        (lo, hi) = (lo.min(s.start as usize), hi.max(s.end as usize));
    }
    for s in &sn[p..sn.len() - q] {
        (lo, hi) = (lo.min(old_at(s.start)), hi.max(old_at(s.end)));
    }
    if delta != 0 {
        // A span that stayed where it was is another span to a paragraph that moved, and one
        // that moved is another to a paragraph that stayed.
        hi = so[..p].iter().fold(hi, |hi, s| hi.max(s.end as usize));
        lo = so[so.len() - q..].iter().fold(lo, |lo, s| lo.min(s.start as usize));
    }
    let ps = &prev.paragraphs;
    let new_lines = count_newlines(new) + 1;
    let same = ps.len().min(new_lines);
    let head_lines = (0..same).take_while(|&k| keyed.lines.get(k).copied() == Some(line_key(spec, k))).count();
    let tail_lines = (0..same).take_while(|&k| keyed.lines.get(ps.len() - 1 - k).copied() == Some(line_key(spec, new_lines - 1 - k))).count();
    let h = ps.partition_point(|p| p.end < lo).min(head_lines);
    let t = ps.partition_point(|p| p.start <= hi).max(ps.len() - tail_lines).max(h);
    let at = |i: usize| (i as isize + delta) as usize;
    Some(Reuse {
        head: ps[..h].iter().map(|p| (p.start, p.end, p.flow, p.key, Box::default(), None)).collect(),
        tail: ps[t..].iter().map(|p| (at(p.start), at(p.end), p.flow, p.key, Box::default(), None)).collect(),
        entering: t.checked_sub(1).map_or(start_flow, |i| ps[i].flow),
    })
}

#[cfg(test)]
#[path = "paged_tests.rs"]
mod tests;
