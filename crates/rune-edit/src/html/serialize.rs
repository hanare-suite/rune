//! Safe HTML serializer for a [`TextEdit`] range.

use std::ops::Range;

use rune_text::{Direction, Flow};

use super::{ListKind, escape_html};
use crate::{Baseline, Block, Mark, MarkColor, MarkRange, ParagraphAlign, TextEdit};

/// Serializes byte range `range` of `edit` to HTML.
pub(crate) fn serialize_html(edit: &TextEdit, range: Range<usize>) -> String {
    let start = range.start.min(edit.text.len());
    let end = range.end.min(edit.text.len()).max(start);
    if start == end && edit.blocks().iter().all(|&b| b == Block::Paragraph) {
        return String::new();
    }

    // Collect paragraphs intersecting `start..end`
    let mut paras: Vec<(usize, Range<usize>, Block, bool)> = Vec::new();
    for p in edit.paragraphs() {
        let p_end_with_nl = if p.range.end < edit.text.len() { p.range.end + 1 } else { p.range.end };
        if p.range.start <= end && p_end_with_nl >= start {
            let sub_start = p.range.start.max(start);
            let sub_end = p.range.end.min(end);
            if sub_start <= sub_end
                && (sub_start < sub_end || start == end || (p.range.start >= start && p.range.end <= end) || (start == p.range.end && end > start))
            {
                // A copy that begins at a line's end begins with that line's empty rest, as a plain paragraph
                // wherever it is from: no empty bullet or quote above what is pasted.
                let lead = sub_start == sub_end && start == p.range.end && p.range.start < start && end > start && !edit.soft_breaks().contains(&(p.index as u32 + 1));
                paras.push((p.index, sub_start..sub_end, if lead { Block::Paragraph } else { p.block }, p.soft_break && p.range.start > start));
            }
        }
    }

    // An empty code line the selection ends at the start of is no part of it: a `<pre>` cannot say its
    // break, so the copy ends behind the break before it, in a paragraph.
    // (A whole document keeps its last line, empty or not.)
    while paras.len() > 1 && !(start == 0 && end == edit.text.len()) && paras.last().is_some_and(|p| p.2 == Block::Code && p.1.is_empty() && p.1.start == end && edit.line_range(p.0).start == end) {
        paras.pop();
    }

    // A copy that ends behind a line break, at the start of a paragraph it takes nothing of,
    // ends in an empty paragraph, so a paste breaks the line there as the copy did.
    let mut open_end = end > start && edit.text.as_bytes()[end - 1] == b'\n' && paras.last().is_some_and(|p| p.1.end < end);
    // If the line it ends at the start of is a soft line, the break is a soft one, which the copy says as an
    // empty soft line of the paragraph it ends: `a<br><br>`.
    if open_end {
        let line = edit.line_of_byte(end);
        if let Some(prev) = paras.last().filter(|p| p.0 + 1 == line && !matches!(p.2, Block::Code) && edit.paragraph(p.0).atom.is_none()) {
            if edit.soft_breaks().binary_search(&(line as u32)).is_ok() && edit.line_range(line).start == end && edit.blocks().get(line) == Some(&prev.2) && edit.paragraph(line).atom.is_none() {
                paras.push((line, end..end, prev.2, true));
                open_end = false;
            }
        }
    }

    let dirs = block_dirs(edit, &paras);
    let mut out = String::new();
    let mut idx = 0;
    while idx < paras.len() {
        let (_, _, block, _) = paras[idx];
        match block {
            Block::Code => {
                out.push_str("<pre");
                out.push_str(&atom_attrs(edit, paras[idx].0, &paras[idx].1));
                out.push_str(dirs[idx]);
                // An atom's `<pre>` is its own, so it can say how it is aligned.
                if edit.paragraph(paras[idx].0).atom.is_some() {
                    out.push_str(align_css(edit.paragraph(paras[idx].0).align));
                }
                out.push_str("><code>");
                // (A parser drops the newline after `<code>`: a block that starts at an empty line says so with another.)
                if paras[idx].1.is_empty() { out.push('\n'); }
                let mut first = true;
                // An atom is a `<pre>` of its own, so it says so; text lines go in one block.
                let alone = edit.paragraph(paras[idx].0).atom.is_some();
                while idx < paras.len() && paras[idx].2 == Block::Code && (first || (!alone && edit.paragraph(paras[idx].0).atom.is_none())) {
                    if !first {
                        out.push('\n');
                    }
                    first = false;
                    let r = &paras[idx].1;
                    escape_html(&edit.text[r.clone()], &mut out);
                    idx += 1;
                }
                // (A parser drops a newline before the end of the document: a last block that ends in an empty line says so with another.)
                if idx == paras.len() && !open_end && paras[idx - 1].1.is_empty() { out.push('\n'); }
                out.push_str("</code></pre>");
            }
            Block::Bullet(_) | Block::Numbered(_) => {
                // Group consecutive list items and emit properly nested `<ul>` / `<ol>`
                let mut open_lists: Vec<ListKind> = Vec::new();
                while idx < paras.len() && paras[idx].2.is_list() {
                    let (_, ref r, blk, _) = paras[idx];
                    let head_atom = edit.paragraph(paras[idx].0).atom.is_some();
                    let (kind, level) = match blk {
                        Block::Bullet(l) => (ListKind::Bullet, l as usize),
                        Block::Numbered(l) => (ListKind::Numbered, l as usize),
                        _ => unreachable!(),
                    };
                    let target_depth = level + 1;
                    while open_lists.len() > target_depth {
                        match open_lists.pop().unwrap() {
                            ListKind::Bullet => out.push_str("</ul>"),
                            ListKind::Numbered => out.push_str("</ol>"),
                        }
                    }
                    if open_lists.len() == target_depth && open_lists.last() != Some(&kind) {
                        match open_lists.pop().unwrap() {
                            ListKind::Bullet => out.push_str("</ul>"),
                            ListKind::Numbered => out.push_str("</ol>"),
                        }
                    }
                    while open_lists.len() < target_depth {
                        let k = if open_lists.len() + 1 == target_depth {
                            kind
                        } else {
                            *open_lists.last().unwrap_or(&kind)
                        };
                        out.push_str(if k == ListKind::Bullet { "<ul" } else { "<ol" });
                        out.push_str(dirs[idx]);
                        out.push('>');
                        open_lists.push(k);
                    }
                    out.push_str("<li");
                    out.push_str(&atom_attrs(edit, paras[idx].0, r));
                    out.push_str(dirs[idx]);
                    out.push_str(align_css(edit.paragraph(paras[idx].0).align));
                    out.push('>');
                    serialize_inline(edit, r.clone(), &mut out);
                    idx += 1;
                    while idx < paras.len() && paras[idx].3 && !head_atom && edit.paragraph(paras[idx].0).atom.is_none() && paras[idx].2 == blk {
                        out.push_str("<br>");
                        serialize_inline(edit, paras[idx].1.clone(), &mut out);
                        idx += 1;
                    }
                    if paras[idx - 1].3 && paras[idx - 1].1.is_empty() { out.push_str("<br>"); }
                    out.push_str("</li>");
                }
                while let Some(k) = open_lists.pop() {
                    match k {
                        ListKind::Bullet => out.push_str("</ul>"),
                        ListKind::Numbered => out.push_str("</ol>"),
                    }
                }
            }
            Block::Quote => {
                let head_atom = edit.paragraph(paras[idx].0).atom.is_some();
                out.push_str("<blockquote");
                out.push_str(&atom_attrs(edit, paras[idx].0, &paras[idx].1));
                out.push_str(dirs[idx]);
                out.push_str(align_css(edit.paragraph(paras[idx].0).align));
                out.push('>');
                serialize_inline(edit, paras[idx].1.clone(), &mut out);
                idx += 1;
                while idx < paras.len() && paras[idx].3 && !head_atom && edit.paragraph(paras[idx].0).atom.is_none() && paras[idx].2 == Block::Quote {
                    out.push_str("<br>");
                    serialize_inline(edit, paras[idx].1.clone(), &mut out);
                    idx += 1;
                }
                // (A `<br>` ends no line at the end of a block: an empty soft line is a second one.)
                if paras[idx - 1].3 && paras[idx - 1].1.is_empty() { out.push_str("<br>"); }
                out.push_str("</blockquote>");
            }
            Block::Paragraph => {
                let attrs = edit.paragraph(paras[idx].0);
                let head_atom = attrs.atom.is_some();
                let named = edit.html_styles().iter().find(|s| s.style == attrs.style);
                let tag = named.map_or("p", |s| s.tag);
                out.push('<');
                out.push_str(tag);
                if let Some(class) = named.and_then(|s| s.class) {
                    out.push_str(" class=\"");
                    escape_html(class, &mut out);
                    out.push('"');
                }
                // An atom copied whole says so, with the id the app keeps its content under.
                out.push_str(&atom_attrs(edit, paras[idx].0, &paras[idx].1));
                out.push_str(dirs[idx]);
                out.push_str(align_css(attrs.align));
                out.push('>');
                serialize_inline(edit, paras[idx].1.clone(), &mut out);
                idx += 1;
                while idx < paras.len() && paras[idx].3 && !head_atom && edit.paragraph(paras[idx].0).atom.is_none() && paras[idx].2 == Block::Paragraph {
                    out.push_str("<br>");
                    serialize_inline(edit, paras[idx].1.clone(), &mut out);
                    idx += 1;
                }
                if paras[idx - 1].3 && paras[idx - 1].1.is_empty() { out.push_str("<br>"); }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
    }
    if open_end {
        out.push_str("<p></p>");
    }
    out
}

/// ` data-atom` and ` data-atom-height` for a paragraph that is an atom copied whole, with the id the
/// app keeps its content under: they go on its `<p>`, `<li>`, `<blockquote>` or `<pre>`.
fn atom_attrs(edit: &TextEdit, line: usize, copied: &std::ops::Range<usize>) -> String {
    let attrs = edit.paragraph(line);
    match attrs.atom.filter(|_| *copied == edit.line_range(line)) {
        Some(height) => format!(" data-atom=\"{}\" data-atom-height=\"{height}\"", attrs.id),
        None => String::new(),
    }
}

/// A block's ` style` attribute for its alignment, empty when it is left-aligned.
fn align_css(align: ParagraphAlign) -> &'static str {
    match align {
        ParagraphAlign::Left => "",
        ParagraphAlign::Center => " style=\"text-align:center\"",
        ParagraphAlign::Right => " style=\"text-align:right\"",
        ParagraphAlign::Justified => " style=\"text-align:justify\"",
    }
}

/// The inline CSS of the marks among `active` that have no element of their own and are not
/// colours (a colour has its own span): family, size and highlight.
fn span_css(active: &[&Mark]) -> String {
    let mut css = String::new();
    let rgb = |c: MarkColor| c.get().hex()[..7].to_owned();
    for m in active {
        match m {
            Mark::Family(name) => {
                let name: String = name.chars().filter(|c| !matches!(c, '\'' | '"' | ';' | '<' | '>' | '&' | '\\')).collect();
                css.push_str(&format!("font-family:'{name}';"));
            }
            Mark::Size(size) => css.push_str(&format!("font-size:{}pt;", size.get())),
            Mark::Highlight(c) => css.push_str(&format!("background-color:{};", rgb(*c))),
            _ => {}
        }
    }
    css.pop();
    css
}

/// Each copied block's ` dir` attribute, by `paras` index: the direction the field showed it in
/// (its first strong character's, else the paragraph before it's, or the one Ctrl+Shift set),
/// and code left to right. Word, Mail and browsers then lay a right-to-left copy out as it was
/// shown. A copy with no right-to-left block has none, so it is as it always was.
fn block_dirs(edit: &TextEdit, paras: &[(usize, Range<usize>, Block, bool)]) -> Vec<&'static str> {
    let forced = edit.forced_dir().map(|f| if f == Flow::Rtl { Direction::Rtl } else { Direction::Ltr });
    let mut last = None;
    let shown: Vec<Option<Direction>> = edit
        .paragraphs()
        .iter()
        .map(|p| {
            last = forced.or_else(|| Direction::of(&edit.text[p.range.clone()])).or(last);
            last
        })
        .collect();
    let at = |i: usize| shown.get(i).copied().flatten();
    let rtl = paras.iter().any(|p| p.2 != Block::Code && at(p.0) == Some(Direction::Rtl));
    paras
        .iter()
        .map(|p| match (p.2, at(p.0)) {
            _ if !rtl => "",
            (Block::Code, _) | (_, Some(Direction::Ltr)) => " dir=\"ltr\"",
            (_, Some(Direction::Rtl)) => " dir=\"rtl\"",
            _ => "",
        })
        .collect()
}

/// Serializes inline marks within `range` (a single line/paragraph slice) to HTML. A pill
/// wholly inside `range` is one `<span data-pill>` around its label's own marks, so marks
/// that cross its edges never split it; a pill cut by `range` is plain text.
fn serialize_inline(edit: &TextEdit, range: Range<usize>, out: &mut String) {
    let mut at = range.start;
    for m in edit.marks().iter().filter(|m| range.start <= m.start && m.end <= range.end) {
        let Some(id) = m.mark.pill_id() else { continue };
        serialize_marks(edit, at..m.start, out);
        out.push_str("<span data-pill=\"");
        escape_html(id, out);
        out.push_str("\">");
        serialize_marks(edit, m.range(), out);
        out.push_str("</span>");
        at = m.end;
    }
    serialize_marks(edit, at..range.end, out);
}

/// Serializes the inline marks other than pills within `range`.
fn serialize_marks(edit: &TextEdit, range: Range<usize>, out: &mut String) {
    if range.is_empty() {
        return;
    }
    let overlapping: Vec<&MarkRange> = edit
        .marks()
        .iter()
        .filter(|m| m.start < range.end && m.end > range.start && !m.mark.is_pill())
        .collect();
    if overlapping.is_empty() {
        escape_html(&edit.text[range], out);
        return;
    }
    let mut bounds = vec![range.start, range.end];
    for m in &overlapping {
        bounds.push(m.start.clamp(range.start, range.end));
        bounds.push(m.end.clamp(range.start, range.end));
    }
    bounds.sort_unstable();
    bounds.dedup();

    // `overlapping` is sorted by start (as `marks()` is): sweep it instead of rescanning.
    let mut covering: Vec<&MarkRange> = Vec::new();
    let mut next = 0;
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b {
            continue;
        }
        covering.retain(|m| m.end > a);
        while next < overlapping.len() && overlapping[next].start <= a {
            if overlapping[next].end > a {
                covering.push(overlapping[next]);
            }
            next += 1;
        }
        let mut active: Vec<&Mark> = covering.iter().map(|m| &m.mark).collect();
        // Canonical nesting order: Link, Color, Bold, Italic, Underline, Strike, Code
        active.sort_by_key(|m| match m {
            Mark::Link(_) => 0u8,
            Mark::Color(_) => 1,
            Mark::Bold => 2,
            Mark::Italic => 3,
            Mark::Underline => 4,
            Mark::Strike => 5,
            Mark::Code => 6,
            Mark::Pill(_) => 7,
            other => other.kind_order(),
        });
        let css = span_css(&active);
        if !css.is_empty() {
            out.push_str("<span style=\"");
            out.push_str(&css);
            out.push_str("\">");
        }
        for m in &active {
            match m {
                Mark::Baseline(Baseline::Sub) => out.push_str("<sub>"),
                Mark::Baseline(Baseline::Super) => out.push_str("<sup>"),
                Mark::Bold => out.push_str("<strong>"),
                Mark::Italic => out.push_str("<em>"),
                Mark::Underline => out.push_str("<u>"),
                Mark::Strike => out.push_str("<s>"),
                Mark::Code => out.push_str("<code>"),
                Mark::Color(hex) => {
                    out.push_str("<span style=\"color:");
                    escape_html(hex, out);
                    out.push_str("\">");
                }
                Mark::Link(url) => {
                    out.push_str("<a href=\"");
                    escape_html(url, out);
                    out.push_str("\">");
                }
                _ => {}
            }
        }
        escape_html(&edit.text[a..b], out);
        for m in active.iter().rev() {
            match m {
                Mark::Bold => out.push_str("</strong>"),
                Mark::Italic => out.push_str("</em>"),
                Mark::Underline => out.push_str("</u>"),
                Mark::Strike => out.push_str("</s>"),
                Mark::Code => out.push_str("</code>"),
                Mark::Color(_) => out.push_str("</span>"),
                Mark::Link(_) => out.push_str("</a>"),
                Mark::Baseline(Baseline::Sub) => out.push_str("</sub>"),
                Mark::Baseline(Baseline::Super) => out.push_str("</sup>"),
                _ => {}
            }
        }
        if !css.is_empty() {
            out.push_str("</span>");
        }
    }
}
