//! Page flow for a by-paragraph field (`El::page_flow`): where each paragraph goes on its page,
//! and which of its lines move to the next one.
//!
//! A paragraph that crosses a page's end keeps one layout. Its lines after the end are broken
//! again with the room to the next page's top above them (`line_break::rebreak_paragraph`), so
//! everything that reads a layout's lines (the caret, a press, selection boxes, glyphs) finds
//! them on their page with no mapping of its own.

use super::*;
use crate::{PageFlow, ParagraphAlign, ParagraphStyle};

/// A field's pages in device px. Page `i`'s top is its point position (`PageFlow::page_top`)
/// rounded once, and its bottom the point position of its end rounded the same way, so the
/// rounding never adds up from one page to the next and the app's sheets, drawn from the same
/// points, stay within half a pixel of the text on every page.
#[derive(Clone, Copy)]
struct Pages {
    flow: PageFlow,
    scale: f32,
    /// A page and the gap after it, unrounded.
    stride: f64,
    /// Widow and orphan control: no line of a paragraph alone at a page's end or top.
    keep: bool,
}

impl Pages {
    fn of(spec: &TextInput<'_>, scale: f32) -> Option<Pages> {
        let flow = spec.flow?.page?;
        let keep = flow.keep_lines;
        let flow = PageFlow::new(flow.height.max(1.0 / scale), flow.gap.max(0.0));
        Some(Pages { flow, scale, stride: f64::from((flow.height + flow.gap) * scale), keep })
    }

    fn top_of(self, page: usize) -> f64 {
        f64::from((self.flow.page_top(page) * self.scale).round())
    }

    fn bottom_of(self, page: usize) -> f64 {
        f64::from(((self.flow.page_top(page) + self.flow.height) * self.scale).round())
    }

    /// The page `y` is on, or the one before the gap it is in.
    fn index(self, y: f64) -> usize {
        let mut page = (y / self.stride).floor().max(0.0) as usize;
        while page > 0 && self.top_of(page) > y { page -= 1; }
        while self.top_of(page + 1) <= y { page += 1; }
        page
    }
}

/// The room around a paragraph, in device px, and whether it starts a page.
pub(super) struct Own {
    before: f64,
    pub(super) after: f64,
    page_break: bool,
}

/// What a field of paragraph styles and pages keeps as it lays its paragraphs out.
pub(super) struct Pager<'a> {
    pages: Option<Pages>,
    styles: &'a [ParagraphStyle],
    /// Where the last paragraph ended, before the room below it.
    ended: f64,
}

impl<'a> Pager<'a> {
    pub(super) fn new(spec: &TextInput<'a>, scale: f32) -> Pager<'a> {
        Pager { pages: Pages::of(spec, scale), styles: spec.flow.map_or(&[], |f| f.styles), ended: 0.0 }
    }

    /// A paragraph whose attributes name a style is shaped in it, with its room around it; any
    /// other takes the field's. A field of paragraph styles or pages places its paragraphs at whole
    /// pixels, so no paragraph's lines are broken from a fraction of one.
    /// Gives the room, the text style and its line height in px.
    pub(super) fn own(&self, attrs: ParagraphAttrs, field: TextStyle, scale: f32) -> (Own, TextStyle, f32) {
        let own = self.styles.get(attrs.style as usize);
        let style = own.map_or(field, |o| o.text);
        let px = |pt: f32| f64::from((pt * scale).round());
        let room = Own { before: own.map_or(0.0, |o| px(o.space_before)), after: own.map_or(0.0, |o| px(o.space_after)), page_break: attrs.page_break_before };
        (room, style, style.line_px() * scale)
    }

    /// Notes that a paragraph `height` px tall now ends from `top`, and gives where the next one
    /// starts below the room under it.
    pub(super) fn end(&mut self, top: f64, height: f32, after: f64) -> f64 {
        self.ended = top + f64::from(height);
        self.ended + after
    }

    /// The field's height once its last paragraph is placed: whole pages and their gaps, or `y`.
    pub(super) fn field_end(&self, y: f64) -> f64 {
        self.pages.map_or(y, |p| p.bottom_of(p.index((self.ended - 1.0).max(0.0))))
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
    /// No attributes: the field's alignment, no indents, no atom.
    pub(super) const NONE: Shape = Shape { align: Alignment::Start, indent: [0.0; 2], atom: None };

    /// `attrs` in a field whose own alignment is `align`: a left-aligned paragraph keeps the
    /// field's, and an atom's label is centred.
    pub(super) fn of(attrs: ParagraphAttrs, align: Alignment, scale: f32) -> Shape {
        let align = match (attrs.atom, attrs.align) {
            (Some(_), _) | (None, ParagraphAlign::Center) => Alignment::Center,
            (None, ParagraphAlign::Left) => align,
            (None, ParagraphAlign::Right) => Alignment::Right,
            (None, ParagraphAlign::Justified) => Alignment::Justify,
        };
        Shape { align, indent: attrs.indent_px(scale), atom: attrs.atom.map(|h| (h * scale).round()) }
    }

    pub(super) fn indented(self) -> bool {
        self.indent != [0.0; 2]
    }
}

/// Where a paragraph's layout starts, and the lines of it that start a page: each line's
/// index and the device px its top moves down by.
struct Spot {
    top: f64,
    gaps: Vec<(u32, f32)>,
}

/// What a paged field's paragraph keeps beyond a plain one's: when it crosses a page's end,
/// the lines that start a page and the px each moved down by, its layout without them (which
/// the next layout places again) and its height without what the gaps added.
#[derive(Clone)]
pub(super) struct Paged {
    /// How it was aligned and indented, which an empty paragraph's caret needs.
    pub(super) shape: Shape,
    pub(super) gaps: Box<[(u32, f32)]>,
    pub(super) natural: Option<Rc<Layout<Ink>>>,
    pub(super) natural_height: f32,
}

impl Paged {
    /// A paragraph that crosses no page's end, `height` px tall.
    pub(super) fn plain(height: f32) -> Paged {
        Paged { shape: Shape::NONE, gaps: Box::default(), natural: None, natural_height: height }
    }
}

/// A paragraph placed: where it starts, how far its label sits below that (a locked atom's, in
/// a box taller than it), its height with the gaps, the height it takes, its layout and what it
/// keeps.
pub(super) struct Placed {
    pub(super) top: f64,
    pub(super) lead: f64,
    pub(super) height: f32,
    /// What the paragraph takes of the field from `top`: its block for an atom, else `height`.
    pub(super) extent: f32,
    pub(super) layout: Rc<Layout<Ink>>,
    pub(super) paged: Paged,
}

impl TextSystem {
    /// Places a paragraph `height` px tall, laid out as `layout`, after what ends at `y`. On its
    /// page: the lines past the page's end move to the next, in a layout broken with that room
    /// above them (the last layout's, when `kept` moved the same lines and `layout` is not new).
    /// A locked atom is one block as tall as the app made it, its label in the middle.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn place_on_page(&mut self, pager: &Pager<'_>, y: f64, first: bool, own: &Own, shape: Shape, layout: Rc<Layout<Ink>>, height: f32, kept: Option<&Paragraph>, fresh: bool, blocks: &[Block], (indent, size): (f32, f32), scale: f32, max_width: Option<f32>) -> Placed {
        let total = shape.atom.map_or(height, |h| h.max(height));
        let lead = f64::from(((total - height) / 2.0).round());
        let spot = place(pager, y, first, own, total, || if shape.atom.is_some() { vec![(0.0, total)] } else { lines_of(&layout, height) });
        let natural_height = height;
        let height = height + spot.gaps.iter().map(|g| g.1).sum::<f32>();
        let (layout, natural) = if spot.gaps.is_empty() {
            (layout, None)
        } else if let Some(o) = kept.filter(|o| !fresh && *o.paged.gaps == *spot.gaps) {
            (o.layout.clone(), Some(layout))
        } else {
            let mut l = layout.clone();
            line_break::rebreak_paragraph(Rc::make_mut(&mut l), blocks, indent, size, scale, max_width, shape.align, shape.indent, &spot.gaps);
            self.layout_ids.build(&l);
            (l, Some(layout))
        };
        Placed { top: spot.top, lead, height, extent: total.max(height), layout, paged: Paged { shape, gaps: spot.gaps.into(), natural, natural_height } }
    }
}

/// Places a paragraph `height` px tall after what ends at `y`, the room below the last one
/// included. `forced` starts it on a new page. `lines` gives its lines' tops and bottoms in its
/// layout, and is asked only when it doesn't fit what is left of its page. The room above is
/// dropped at the top of a page the paragraph flowed onto, and kept under a forced break.
///
/// A line taller than a page stands alone on its page and runs past its end, and the line
/// after it starts the first page below where it ends: a gap is never negative.
///
/// With [`Pages::keep`], the line that starts a page is never the paragraph's second (its
/// first would stand alone at the page's end: an orphan) nor its last (a widow): one more line
/// goes with it, or the whole paragraph.
fn place(pager: &Pager<'_>, y: f64, first: bool, own: &Own, height: f32, lines: impl FnOnce() -> Vec<(f32, f32)>) -> Spot {
    let Some(p) = pager.pages else { return Spot { top: y + own.before, gaps: Vec::new() } };
    let mut y = y;
    // What came before ended on a page, and its room below ran past the page's end, or on into
    // the gap: both rooms are dropped and this paragraph starts the next page.
    let last = p.index((pager.ended - 1.0).max(0.0));
    if y >= p.bottom_of(last) { y = p.top_of(last + 1); }
    let forced = own.page_break && !first;
    let mut page = p.index(y);
    if forced && y > p.top_of(page) { page += 1; y = p.top_of(page); }
    let flowed = y == p.top_of(page) && !forced && !first;
    // A paragraph that starts its page, with the room above it or without: its first line can't
    // move off that page, or the page would stand blank.
    let (opened, start_page) = (forced || first || flowed, page);
    // The paragraph is drawn at whole pixels (`top.round()`), so it is fitted there too.
    let mut top = if flowed { y } else { (y + own.before).round() };
    let mut bottom = p.bottom_of(page);
    if top + f64::from(height) <= bottom {
        return Spot { top, gaps: Vec::new() };
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
        let pinned = |k: usize, page: usize| k == 0 && opened && page == start_page;
        if line_bottom <= bottom || line_top(i, top, shift) <= p.top_of(page) || pinned(i, page) {
            i += 1;
            continue;
        }
        let mut j = i;
        if p.keep {
            if j + 1 == n && j >= 2 { j -= 1; }
            if j == 1 && page_first == 0 { j = 0; }
            // A line already at its page's top can't move further.
            if j < i && (j < page_first || line_top(j, top, shift) <= p.top_of(page) || pinned(j, page)) { j = i; }
        }
        // The next page's top, or the first one below a line so tall it ran past that: a line
        // never moves up over the one before it.
        page += 1;
        let mut next = p.top_of(page);
        if j > 0 {
            let at = line_top(j, top, shift);
            while next < at {
                page += 1;
                next = p.top_of(page);
            }
        }
        if j == 0 {
            top = (next - f64::from(lines[0].0)).round();
        } else {
            let gap = next - line_top(j, top, shift);
            gaps.push((j as u32, gap as f32));
            shift += gap;
        }
        bottom = p.bottom_of(page);
        (page_first, i) = (j, j + 1);
    }
    Spot { top, gaps }
}

/// A layout's lines' tops and bottoms, one line `height` tall for a layout with none.
fn lines_of(layout: &Layout<Ink>, height: f32) -> Vec<(f32, f32)> {
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
    // Equal texts touched no byte: only a span or an attribute that differs widens the range.
    let (mut lo, mut hi) = if prefix == old.len() && old.len() == new.len() { (usize::MAX, 0) } else { (prefix, old.len() - suffix) };
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
        // The span's new numbers and the old text's at the same place: an app that keeps a span
        // through an edit that shifts the text covers other paragraphs than it did.
        (lo, hi) = (lo.min(s.start as usize).min(old_at(s.start)), hi.max(s.end as usize).max(old_at(s.end)));
    }
    if delta != 0 {
        // A span that stayed where it was is another span to a paragraph that moved, and one
        // that moved is another to a paragraph that stayed.
        // A span the app kept at its old numbers covers other text now: the bytes at its numbers
        // in the new text are the old text's at `old_at` of them.
        hi = so[..p].iter().fold(hi, |hi, s| hi.max(s.end as usize).max(old_at(s.end)));
        lo = so[so.len() - q..].iter().fold(lo, |lo, s| lo.min(s.start as usize));
    }
    let ps = &prev.paragraphs;
    let new_lines = count_newlines(new) + 1;
    let same = ps.len().min(new_lines);
    let head_lines = (0..same).take_while(|&k| keyed.lines.get(k).copied() == Some(line_key(spec, k))).count();
    let tail_lines = (0..same).take_while(|&k| keyed.lines.get(ps.len() - 1 - k).copied() == Some(line_key(spec, new_lines - 1 - k))).count();
    // At least one paragraph is keyed again: the caller keys from where the head ends.
    let h = ps.partition_point(|p| p.end < lo).min(head_lines).min(ps.len().saturating_sub(1));
    let t = ps.partition_point(|p| p.start <= hi).max(ps.len() - tail_lines).max(h + 1).min(ps.len());
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
