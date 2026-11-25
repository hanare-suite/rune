//! One layout of a text and where it sits (`Piece`), and the spans, pills and boxes cut to a
//! paragraph's words.

use super::*;

/// One layout of a text and where it sits: its whole layout, or one paragraph of a field's.
#[derive(Clone, Copy)]
pub struct Piece<'a> {
    pub layout: &'a Layout<Ink>,
    /// A paragraph's own layout, which glyph caching keys on.
    pub shared: Option<&'a Rc<Layout<Ink>>>,
    /// Its content bytes; a whole layout's run past the end.
    pub start: usize,
    pub end: usize,
    pub y: f32,
    pub height: f32,
    pub flow: Flow,
    /// Its paragraph's index in the field, for its [`Block`].
    pub index: usize,
    pub shaped: bool,
    pub(super) pad: [f32; 2],
    pub(super) isolates: &'a [(u32, u32)],
    pub(super) disjoint_spans: bool,
    pub(super) whole: bool,
}

impl<'a> Piece<'a> {
    pub(crate) fn of_paragraph(k: usize, p: &'a Paragraph, disjoint_spans: bool) -> Piece<'a> {
        Piece { layout: &p.layout, shared: Some(&p.layout), start: p.start, end: p.end, y: p.y, height: p.height, flow: p.flow, index: k, shaped: p.shaped, pad: p.pad, isolates: &p.isolates, disjoint_spans, whole: false }
    }

    pub(super) fn whole(layout: &'a Layout<Ink>) -> Piece<'a> {
        Piece { layout, shared: None, start: 0, end: usize::MAX, y: 0.0, height: layout.height(), flow: Flow::of_layout(layout), index: 0, shaped: true, pad: [0.0, 0.0], isolates: &[], disjoint_spans: false, whole: true }
    }

    /// Whether it's a text's one layout rather than a paragraph of a field's.
    pub fn is_whole(&self) -> bool {
        self.whole
    }

    /// Whether it's a paragraph with no words.
    pub fn is_blank(&self) -> bool {
        !self.whole && self.start == self.end
    }

    /// Its shaped text's byte at content byte `at` (clamped into it).
    pub fn shaped(&self, at: usize) -> usize {
        if self.whole { return at; }
        shaped(self.isolates, at.clamp(self.start, self.end) - self.start)
    }

    /// The content byte at its shaped text's byte `at`.
    pub fn content(&self, at: usize) -> usize {
        if self.whole { return at; }
        self.start + content(self.isolates, at)
    }

    /// `spans` over its words, on its shaped text (a whole layout's own).
    pub fn spans<'s>(&self, spans: &'s [Span]) -> Cow<'s, [Span]> {
        if self.whole || spans.is_empty() { return Cow::Borrowed(spans); }
        Cow::Owned(spans_in(spans, self.start, self.end, self.isolates, self.disjoint_spans).collect())
    }

    /// `pills` inside it, on its shaped text.
    pub fn pills<'s>(&self, pills: &'s [PillChip]) -> Cow<'s, [PillChip]> {
        if self.whole || pills.is_empty() { return Cow::Borrowed(pills); }
        Cow::Owned(pills_in(pills, self.start, self.end, self.isolates).collect())
    }

    /// Strikethrough byte ranges over its words, on its shaped text.
    pub fn strikes<'s>(&self, strikes: &'s [(u32, u32, Option<Color>)]) -> Cow<'s, [(u32, u32, Option<Color>)]> {
        if self.whole || strikes.is_empty() { return Cow::Borrowed(strikes); }
        Cow::Owned(strikes.iter().filter_map(|&(a, b, c)| {
            let (a, b) = self.clip(a as usize, b as usize)?;
            Some((a as u32, b as u32, c))
        }).collect())
    }

    /// Overline byte ranges over its words, on its shaped text.
    pub fn overlines<'s>(&self, overlines: &'s [(u32, u32, Option<Color>)]) -> Cow<'s, [(u32, u32, Option<Color>)]> {
        self.strikes(overlines)
    }

    /// [`SpanBoxRun`]s meeting its words, clipped to its paragraph on its shaped text.
    pub fn span_boxes<'s>(&self, span_boxes: &'s [SpanBoxRun]) -> Cow<'s, [SpanBoxRun]> {
        if self.whole || span_boxes.is_empty() { return Cow::Borrowed(span_boxes); }
        Cow::Owned(span_boxes_in(span_boxes, self.start, self.end, self.isolates).collect())
    }

    /// Content bytes `a..b` on its shaped text, when they meet its words.
    pub fn clip(&self, a: usize, b: usize) -> Option<(usize, usize)> {
        if self.whole { return (a < b).then_some((a, b)); }
        (a < self.end && self.start < b && a < b).then(|| (self.shaped(a), self.shaped(b)))
    }
}

/// Its shaped text's byte at byte `local` of a paragraph with these isolated pills.
fn shaped(isolates: &[(u32, u32)], local: usize) -> usize {
    local + isolates.iter().map(|&(a, b)| ISOLATE * (usize::from((a as usize) < local) + usize::from(b as usize <= local))).sum::<usize>()
}

/// The paragraph byte at its shaped text's byte `at`: inside an FSI it's the pill's start, inside
/// a PDI its end.
fn content(isolates: &[(u32, u32)], at: usize) -> usize {
    let mut shift = 0;
    for &(a, b) in isolates {
        let (a, b) = (a as usize, b as usize);
        let fsi = a + shift;
        if at <= fsi { return at - shift; }
        if at <= fsi + ISOLATE { return a; }
        let pdi = b + shift + ISOLATE;
        if at < pdi { return at - shift - ISOLATE; }
        if at <= pdi + ISOLATE { return b; }
        shift += 2 * ISOLATE;
    }
    at - shift
}

/// `spans` meeting content bytes `a..b`, cut to them and moved onto the shaped text.
pub(crate) fn spans_in<'s>(spans: &'s [Span], a: usize, b: usize, isolates: &'s [(u32, u32)], disjoint: bool) -> impl Iterator<Item = Span> + 's {
    let window = if disjoint && spans.len() > 8 {
        let lo = spans.partition_point(|s| (s.end as usize) <= a);
        let hi = lo + spans[lo..].partition_point(|s| (s.start as usize) < b);
        &spans[lo..hi]
    } else {
        spans
    };
    window.iter().filter(move |s| (s.start as usize) < b && a < s.end as usize).map(move |s| Span {
        start: shaped(isolates, (s.start as usize).max(a) - a) as u32,
        end: shaped(isolates, (s.end as usize).min(b) - a) as u32,
        ..s.clone()
    })
}

/// `pills` inside content bytes `a..b`, each its label's shaped bytes.
pub(crate) fn pills_in<'s>(pills: &'s [PillChip], a: usize, b: usize, isolates: &'s [(u32, u32)]) -> impl Iterator<Item = PillChip> + 's {
    pills.iter().filter(move |p| inside(p, a, b)).map(move |p| PillChip {
        start: (shaped(isolates, p.start as usize - a) + ISOLATE) as u32,
        end: (shaped(isolates, p.end as usize - a) - ISOLATE) as u32,
        ..p.clone()
    })
}

pub(crate) fn inside(p: &PillChip, a: usize, b: usize) -> bool {
    a <= p.start as usize && p.end as usize <= b && p.start < p.end
}

/// `span_boxes` meeting content bytes `a..b`, clipped to them and moved onto the shaped text.
pub(crate) fn span_boxes_in<'s>(span_boxes: &'s [SpanBoxRun], a: usize, b: usize, isolates: &'s [(u32, u32)]) -> impl Iterator<Item = SpanBoxRun> + 's {
    span_boxes.iter().filter(move |sb| (sb.start as usize) < b && a < sb.end as usize && sb.start < sb.end).map(move |sb| {
        let clip_start = (sb.start as usize) < a;
        let clip_end = (sb.end as usize) > b;
        let start = shaped(isolates, (sb.start as usize).max(a) - a) as u32;
        let end = shaped(isolates, (sb.end as usize).min(b) - a) as u32;
        let mut style = sb.style;
        if clip_start || clip_end {
            let pl = if clip_start { 0.0 } else { style.pad_left() };
            let pr = if clip_end { 0.0 } else { style.pad_right() };
            let c = style.corner_radii();
            style = style.pad_x(pl, pr).corners([
                if clip_start { 0.0 } else { c[0] },
                if clip_end { 0.0 } else { c[1] },
                if clip_end { 0.0 } else { c[2] },
                if clip_start { 0.0 } else { c[3] },
            ]);
        }
        SpanBoxRun { start, end, style }
    })
}
