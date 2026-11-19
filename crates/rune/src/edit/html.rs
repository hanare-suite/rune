//! Safe HTML subset import and export for [`TextEdit`].
//!
//! Supported elements:
//! - Inline: `<b>`, `<strong>`, `<i>`, `<em>`, `<u>`, `<ins>`, `<s>`, `<del>`, `<strike>`,
//!   `<a href="...">`, `<code>`, `<sub>`, `<sup>`, and CSS `style` attributes (`font-weight`,
//!   `font-style`, `text-decoration`, `text-decoration-line`, `font-family`, `font-size`,
//!   `color`, `background-color`, `vertical-align`), and `<span data-pill="id">` for
//!   [`Mark::Pill`].
//! - Block: `<p>`, `<div>`, `<br>`, `<ul>`, `<ol>`, `<li>`, `<blockquote>`, `<pre>`, with
//!   `text-align`, and the elements a field's [`HtmlStyle`]s name for its paragraph styles
//!   (`<h1>`, `<p class="title">`).
//! - Stripped with content: `<script>`, `<style>`, `<head>`.
//! - Unknown tags drop their wrapper and keep their text. Nothing is executed, and dangerous
//!   link schemes (`javascript:`, `vbscript:`, `data:`) are rejected.

use std::ops::Range;

use super::{Baseline, Block, HtmlStyle, MAX_LIST_LEVEL, Mark, MarkColor, MarkRange, MarkSize, ParagraphAlign, ParagraphAttrs, TextEdit};
use crate::color::Color;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Numbered,
}

#[derive(Clone)]
struct ActiveMark {
    mark: Mark,
    start: usize,
}

/// Safe URL check for `<a href="...">`: allows `http:`, `https:`, `mailto:` and `tel:`, and
/// scheme-less URLs. Browsers drop ASCII tabs and newlines anywhere in a URL and leading
/// controls and spaces, so `java\tscript:` is checked as `javascript:`; anything else with a
/// scheme (`javascript:`, `data:`, `file:`, app-specific schemes) is rejected.
fn is_safe_href(url: &str) -> bool {
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .take(64)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if cleaned.is_empty() {
        return false;
    }
    let scheme_len = cleaned
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
        .unwrap_or(cleaned.len());
    if !cleaned[scheme_len..].starts_with(':') {
        // No scheme: a relative path, `www.example.com` or `#fragment`.
        return true;
    }
    matches!(&cleaned[..scheme_len], "http" | "https" | "mailto" | "tel")
}

/// Decodes HTML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&nbsp;`, `&#123;`, `&#x1F600;`).
pub(crate) fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        // Entities are short: look for `;` in the next 33 bytes only, so a run of bare `&`s
        // stays linear.
        let window = &rest.as_bytes()[..rest.len().min(33)];
        let Some(semi) = window.iter().position(|&b| b == b';') else {
            out.push('&');
            continue;
        };
        let entity = &rest[..semi];
        if entity.is_empty() || entity.contains(char::is_whitespace) {
            out.push('&');
            continue;
        }
        let decoded = if let Some(hex) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(dec) = entity.strip_prefix('#') {
            dec.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity {
                "amp" | "AMP" => Some('&'),
                "lt" | "LT" => Some('<'),
                "gt" | "GT" => Some('>'),
                "quot" | "QUOT" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" | "NBSP" => Some('\u{00a0}'),
                "copy" => Some('©'),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "hellip" => Some('…'),
                "lsquo" => Some('‘'),
                "rsquo" => Some('’'),
                "ldquo" => Some('“'),
                "rdquo" => Some('”'),
                "bull" => Some('•'),
                // Browsers decode these in an `href` too, so `javascript&colon;` is still a scheme.
                "colon" => Some(':'),
                "Tab" => Some('\t'),
                "NewLine" => Some('\n'),
                _ => None,
            }
        };
        if let Some(ch) = decoded {
            out.push(ch);
            rest = &rest[semi + 1..];
        } else {
            out.push('&');
        }
    }
    out.push_str(rest);
    out
}

fn escape_html(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

/// Extracts an HTML attribute's value from a tag tail (`href="..."`, `style='...'`, `href=foo`).
fn extract_attr(attrs: &str, name: &str) -> Option<String> {
    let bytes = attrs.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_' | b':')) {
            i += 1;
        }
        if i == key_start {
            i += 1;
            continue;
        }
        let key = &attrs[key_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let val = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let val_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let v = &attrs[val_start..i];
            if i < bytes.len() {
                i += 1;
            }
            v
        } else {
            let val_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
                i += 1;
            }
            &attrs[val_start..i]
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(decode_entities(val));
        }
    }
    None
}

/// A CSS colour: `#rgb`, `#rrggbb`, `rgb(r, g, b)` or one of the names Word and browsers
/// write. None for anything else, `transparent`, `windowtext` and `inherit` among them.
fn css_color(val: &str) -> Option<Color> {
    let v = val.trim().to_ascii_lowercase();
    let rgb = |r: u8, g: u8, b: u8| crate::color::hex(&format!("#{r:02x}{g:02x}{b:02x}"));
    if let Some(h) = v.strip_prefix('#') {
        if !h.bytes().all(|b| b.is_ascii_hexdigit()) { return None; }
        return match h.len() {
            3 => crate::color::hex(&format!("#{}", h.chars().flat_map(|c| [c, c]).collect::<String>())),
            6 => crate::color::hex(&v),
            _ => None,
        };
    }
    if let Some(body) = v.strip_prefix("rgb(").or_else(|| v.strip_prefix("rgba(")).and_then(|b| b.strip_suffix(')')) {
        let mut parts = body.split([',', ' ', '/']).filter(|p| !p.is_empty());
        let mut channel = || parts.next()?.trim().parse::<f32>().ok().map(|c| c.clamp(0.0, 255.0).round() as u8);
        let (r, g, b) = (channel()?, channel()?, channel()?);
        // A colour that is fully transparent is no colour.
        if parts.next().is_some_and(|a| a.trim().parse::<f32>().is_ok_and(|a| a == 0.0)) { return None; }
        return rgb(r, g, b);
    }
    let (r, g, b) = match v.as_str() {
        "black" => (0, 0, 0), "white" => (255, 255, 255), "red" => (255, 0, 0), "green" => (0, 128, 0), "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0), "cyan" | "aqua" => (0, 255, 255), "magenta" | "fuchsia" => (255, 0, 255),
        "gray" | "grey" => (128, 128, 128), "silver" => (192, 192, 192), "maroon" => (128, 0, 0), "olive" => (128, 128, 0),
        "navy" => (0, 0, 128), "purple" => (128, 0, 128), "teal" => (0, 128, 128), "lime" => (0, 255, 0), "orange" => (255, 165, 0),
        "darkblue" => (0, 0, 139), "darkcyan" => (0, 139, 139), "darkgreen" => (0, 100, 0), "darkmagenta" => (139, 0, 139),
        "darkred" => (139, 0, 0), "darkyellow" => (128, 128, 0), "darkgray" | "darkgrey" => (169, 169, 169),
        "lightgray" | "lightgrey" => (211, 211, 211),
        _ => return None,
    };
    rgb(r, g, b)
}

/// A CSS length as points: `12pt`, `16px`. None for a keyword or another unit.
fn css_points(val: &str) -> Option<f32> {
    let v = val.trim().to_ascii_lowercase();
    let (number, scale) = if let Some(n) = v.strip_suffix("pt") { (n, 1.0) } else if let Some(n) = v.strip_suffix("px") { (n, 0.75) } else { return None };
    number.trim().parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 1.0 && *n <= 1638.0).map(|n| n * scale)
}

/// The first family of a CSS `font-family` list, without its quotes.
fn css_family(val: &str) -> Option<&str> {
    let first = val.split(',').next()?.trim().trim_matches(['"', '\'']).trim();
    (!first.is_empty() && !matches!(first.to_ascii_lowercase().as_str(), "inherit" | "initial" | "serif" | "sans-serif" | "monospace")).then_some(first)
}

/// `text-align` of an inline `style`, or the `align` attribute.
fn block_align(attrs: &str) -> Option<ParagraphAlign> {
    let of = |v: &str| match v.trim().to_ascii_lowercase().as_str() {
        "left" | "start" => Some(ParagraphAlign::Left),
        "center" => Some(ParagraphAlign::Center),
        "right" | "end" => Some(ParagraphAlign::Right),
        "justify" => Some(ParagraphAlign::Justified),
        _ => None,
    };
    let styled = extract_attr(attrs, "style").and_then(|style| {
        style.split(';').filter_map(|d| d.split_once(':')).filter(|(p, _)| p.trim().eq_ignore_ascii_case("text-align")).find_map(|(_, v)| of(v))
    });
    styled.or_else(|| extract_attr(attrs, "align").and_then(|v| of(&v)))
}

/// The paragraph style `styles` gives the element `tag` with these attributes.
fn block_style(styles: &[HtmlStyle], tag: &str, attrs: &str) -> Option<u16> {
    let class = extract_attr(attrs, "class");
    let has = |name: &str| class.as_deref().is_some_and(|c| c.split_whitespace().any(|c| c.eq_ignore_ascii_case(name)));
    // An entry with a class is the closer match, so it is asked first.
    styles.iter().find(|s| s.tag == tag && s.class.is_some_and(has)).or_else(|| styles.iter().find(|s| s.tag == tag && s.class.is_none())).map(|s| s.style)
}

/// Parses inline CSS `style="..."` for the marks this model keeps.
fn parse_style_marks(style: &str, add: &mut Vec<Mark>, remove: &mut Vec<Mark>) {
    for decl in style.split(';') {
        let Some((prop, val)) = decl.split_once(':') else { continue };
        let prop = prop.trim();
        let val = val.trim();
        if prop.eq_ignore_ascii_case("font-family") {
            if let Some(name) = css_family(val) { add.push(Mark::family(name)); }
        } else if prop.eq_ignore_ascii_case("font-size") {
            if let Some(points) = css_points(val) { add.push(Mark::Size(MarkSize::pt(points))); }
        } else if prop.eq_ignore_ascii_case("color") {
            if let Some(c) = css_color(val) { add.push(Mark::Color(MarkColor::new(c))); }
        } else if prop.eq_ignore_ascii_case("background-color") || prop.eq_ignore_ascii_case("background") || prop.eq_ignore_ascii_case("mso-highlight") {
            if let Some(c) = val.split_whitespace().find_map(css_color) { add.push(Mark::Highlight(MarkColor::new(c))); }
        } else if prop.eq_ignore_ascii_case("vertical-align") {
            if val.eq_ignore_ascii_case("super") { add.push(Mark::Baseline(Baseline::Super)); }
            else if val.eq_ignore_ascii_case("sub") { add.push(Mark::Baseline(Baseline::Sub)); }
        } else if prop.eq_ignore_ascii_case("font-weight") {
            let v = val.split_whitespace().next().unwrap_or("");
            if v.eq_ignore_ascii_case("bold")
                || v.eq_ignore_ascii_case("bolder")
                || v.parse::<f32>().is_ok_and(|w| w >= 600.0)
            {
                add.push(Mark::Bold);
            } else if v.eq_ignore_ascii_case("normal")
                || v.eq_ignore_ascii_case("lighter")
                || v.parse::<f32>().is_ok_and(|w| w < 600.0)
            {
                remove.push(Mark::Bold);
            }
        } else if prop.eq_ignore_ascii_case("font-style") {
            let v = val.split_whitespace().next().unwrap_or("");
            if v.eq_ignore_ascii_case("italic") || v.eq_ignore_ascii_case("oblique") {
                add.push(Mark::Italic);
            } else if v.eq_ignore_ascii_case("normal") {
                remove.push(Mark::Italic);
            }
        } else if prop.eq_ignore_ascii_case("text-decoration")
            || prop.eq_ignore_ascii_case("text-decoration-line")
        {
            for tok in val.split_whitespace() {
                if tok.eq_ignore_ascii_case("underline") {
                    add.push(Mark::Underline);
                } else if tok.eq_ignore_ascii_case("line-through") {
                    add.push(Mark::Strike);
                } else if tok.eq_ignore_ascii_case("none") {
                    remove.push(Mark::Underline);
                    remove.push(Mark::Strike);
                }
            }
        }
    }
}

const MAX_DEPTH: usize = 512;

struct TagFrame {
    tag: String,
    added_marks: Vec<Mark>,
    suppressed_marks: Vec<Mark>,
}

pub(crate) fn parse_html(html: &str, styles: &[HtmlStyle]) -> TextEdit {
    let mut text = String::new();
    // Each line's attributes, as far as a block element set them.
    let mut line_attrs: Vec<ParagraphAttrs> = Vec::new();
    let mut marks: Vec<MarkRange> = Vec::new();
    let mut blocks: Vec<Block> = vec![Block::Paragraph];
    let mut soft_breaks: Vec<u32> = Vec::new();

    let mut stack: Vec<TagFrame> = Vec::new();
    // Open frames per tag name, so a stray closing tag never scans the whole stack.
    let mut open_tags: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut active_marks: Vec<ActiveMark> = Vec::new();
    let mut list_stack: Vec<ListKind> = Vec::new();
    let mut in_pre: usize = 0;
    let mut in_quote: usize = 0;
    let mut in_li: usize = 0;
    // Whether the current line has been claimed by a block tag or text, whether that block is
    // still open awaiting its first content, and whether text/br has been written on this line.
    let mut line_has_block = false;
    let mut line_open = false;
    let mut line_has_content = false;

    let current_default_block = |list_stack: &[ListKind], in_pre: usize, in_quote: usize, in_li: usize| -> Block {
        if in_pre > 0 {
            Block::Code
        } else if in_li > 0 && let Some(&kind) = list_stack.last() {
            let level = (list_stack.len() - 1).min(MAX_LIST_LEVEL as usize) as u8;
            match kind {
                ListKind::Bullet => Block::Bullet(level),
                ListKind::Numbered => Block::Numbered(level),
            }
        } else if in_quote > 0 {
            Block::Quote
        } else {
            Block::Paragraph
        }
    };

    let flush_active_marks = |active: &mut [ActiveMark], marks: &mut Vec<MarkRange>, pos: usize, in_pre: usize| {
        if in_pre == 0 {
            for a in active.iter_mut() {
                if a.start < pos {
                    marks.push(MarkRange {
                        start: a.start,
                        end: pos,
                        mark: a.mark.clone(),
                    });
                }
                a.start = pos;
            }
        } else {
            for a in active.iter_mut() {
                a.start = pos;
            }
        }
    };

    let start_new_paragraph = |text: &mut String,
                               blocks: &mut Vec<Block>,
                               active: &mut [ActiveMark],
                               marks: &mut Vec<MarkRange>,
                               line_has_block: &mut bool,
                               line_open: &mut bool,
                               line_has_content: &mut bool,
                               block: Block,
                               in_pre: usize| {
        if (!*line_has_block || *line_open) && !*line_has_content {
            if let Some(last) = blocks.last_mut() {
                *last = block;
            }
            *line_has_block = true;
            *line_open = true;
            return;
        }
        let pos = text.len();
        flush_active_marks(active, marks, pos, in_pre);
        text.push('\n');
        blocks.push(block);
        let new_pos = text.len();
        for a in active.iter_mut() {
            a.start = new_pos;
        }
        *line_has_block = true;
        *line_open = true;
        *line_has_content = false;
    };

    let push_br = |text: &mut String,
                   blocks: &mut Vec<Block>,
                   soft_breaks: &mut Vec<u32>,
                   active: &mut [ActiveMark],
                   marks: &mut Vec<MarkRange>,
                   in_pre: usize| {
        flush_active_marks(active, marks, text.len(), in_pre);
        let blk = blocks.last().copied().unwrap_or(Block::Paragraph);
        text.push('\n');
        blocks.push(blk);
        if in_pre == 0 {
            soft_breaks.push((blocks.len() - 1) as u32);
        }
        for a in active.iter_mut() {
            a.start = text.len();
        }
    };
    // A `<br>` (text and block counts where it fell) waiting to see whether text follows it.
    let mut deferred_br: Option<(usize, usize)> = None;

    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Comment `<!-- ... -->`
            if html[i..].starts_with("<!--") {
                if let Some(end) = html[i + 4..].find("-->") {
                    i += 4 + end + 3;
                } else {
                    break;
                }
                continue;
            }
            // Processing instruction or doctype `<!...>` / `<?...>`
            if html[i..].starts_with("<!") || html[i..].starts_with("<?") {
                if let Some(end) = html[i + 2..].find('>') {
                    i += 2 + end + 1;
                } else {
                    break;
                }
                continue;
            }
            let Some(close_angle) = html[i + 1..].find('>') else {
                // Unclosed `<`, treat as literal text
                let decoded = decode_entities(&html[i..]);
                text.push_str(&decoded);
                break;
            };
            let raw_tag = html[i + 1..i + 1 + close_angle].trim();
            i += 1 + close_angle + 1;

            if raw_tag.is_empty() {
                continue;
            }
            let is_closing = raw_tag.starts_with('/');
            let body = raw_tag.trim_start_matches('/').trim_end_matches('/').trim();
            let tag_end = body
                .find(|c: char| c.is_whitespace() || c == '/')
                .unwrap_or(body.len());
            let tag = body[..tag_end].to_ascii_lowercase();
            let attrs = &body[tag_end..];
            let self_closing = raw_tag.ends_with('/')
                || matches!(
                    tag.as_str(),
                    "area" | "base" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "source" | "track" | "wbr"
                );

            if !is_closing && matches!(tag.as_str(), "script" | "style" | "head") {
                if !self_closing {
                    // Skip until matching `</tag>` (case-insensitive)
                    let pat = format!("</{tag}");
                    let mut search = i;
                    let mut found = None;
                    while search + pat.len() <= bytes.len() {
                        if html[search..].len() >= pat.len()
                            && html.is_char_boundary(search)
                            && html.is_char_boundary(search + pat.len())
                            && html[search..search + pat.len()].eq_ignore_ascii_case(&pat)
                        {
                            // No `>` after this means none after any later match either.
                            found = html[search + pat.len()..].find('>').map(|gt| search + pat.len() + gt + 1);
                            break;
                        }
                        search += 1;
                    }
                    i = found.unwrap_or(bytes.len());
                }
                continue;
            }

            if !is_closing && tag == "br" {
                if deferred_br == Some((text.len(), blocks.len())) {
                    push_br(&mut text, &mut blocks, &mut soft_breaks, &mut active_marks, &mut marks, in_pre);
                    line_has_block = true;
                    line_open = true;
                }
                deferred_br = None;
                if in_pre > 0 {
                    push_br(&mut text, &mut blocks, &mut soft_breaks, &mut active_marks, &mut marks, in_pre);
                    line_has_content = false;
                } else {
                    // A `<br>` only breaks the line when something follows it in the same block,
                    // as browsers draw it: `<p>A<br></p>` is one line, `<p><br></p>` one blank line.
                    deferred_br = Some((text.len(), blocks.len()));
                    line_has_block = true;
                    line_open = true;
                    line_has_content = true;
                }
                continue;
            }

            // Deeper nesting than any real document keeps its text but not its tags, so a flood of
            // opening tags can't grow the stack without bound.
            if !is_closing && !self_closing && stack.len() >= MAX_DEPTH {
                continue;
            }

            if !is_closing {
                match tag.as_str() {
                    "ul" => {
                        list_stack.push(ListKind::Bullet);
                        // The first item takes a line still waiting for content (`<li><ul>`), but
                        // never a finished one, even if it was blank (`<p></p><ul>`).
                        if line_has_content {
                            line_open = false;
                        }
                    }
                    "ol" => {
                        list_stack.push(ListKind::Numbered);
                        // The first item takes a line still waiting for content (`<li><ul>`), but
                        // never a finished one, even if it was blank (`<p></p><ul>`).
                        if line_has_content {
                            line_open = false;
                        }
                    }
                    "li" => {
                        in_li += 1;
                        let blk = current_default_block(&list_stack, in_pre, in_quote, in_li);
                        start_new_paragraph(
                            &mut text,
                            &mut blocks,
                            &mut active_marks,
                            &mut marks,
                            &mut line_has_block,
                            &mut line_open,
                            &mut line_has_content,
                            blk,
                            in_pre,
                        );
                        if let Some(align) = block_align(attrs) {
                            line_attrs.resize(blocks.len(), ParagraphAttrs::default());
                            line_attrs[blocks.len() - 1].align = align;
                        }
                    }
                    "blockquote" => {
                        in_quote += 1;
                        let blk = current_default_block(&list_stack, in_pre, in_quote, in_li);
                        start_new_paragraph(
                            &mut text,
                            &mut blocks,
                            &mut active_marks,
                            &mut marks,
                            &mut line_has_block,
                            &mut line_open,
                            &mut line_has_content,
                            blk,
                            in_pre,
                        );
                    }
                    "pre" => {
                        in_pre += 1;
                        start_new_paragraph(
                            &mut text,
                            &mut blocks,
                            &mut active_marks,
                            &mut marks,
                            &mut line_has_block,
                            &mut line_open,
                            &mut line_has_content,
                            Block::Code,
                            in_pre,
                        );
                    }
                    "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        let blk = current_default_block(&list_stack, in_pre, in_quote, in_li);
                        start_new_paragraph(
                            &mut text,
                            &mut blocks,
                            &mut active_marks,
                            &mut marks,
                            &mut line_has_block,
                            &mut line_open,
                            &mut line_has_content,
                            blk,
                            in_pre,
                        );
                        // Inside a list item the item's own line keeps what it has.
                        let (style, align) = (block_style(styles, &tag, attrs), block_align(attrs));
                        if style.is_some() || align.is_some() {
                            line_attrs.resize(blocks.len(), ParagraphAttrs::default());
                            let slot = &mut line_attrs[blocks.len() - 1];
                            slot.style = style.unwrap_or(slot.style);
                            slot.align = align.unwrap_or(slot.align);
                        }
                    }
                    _ => {}
                }

                let mut to_add = Vec::new();
                let mut to_suppress = Vec::new();
                match tag.as_str() {
                    // A heading the field has a style for is that style, not bold text.
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" if block_style(styles, &tag, attrs).is_some() => {}
                    "sub" => to_add.push(Mark::Baseline(Baseline::Sub)),
                    "sup" => to_add.push(Mark::Baseline(Baseline::Super)),
                    "b" | "strong" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => to_add.push(Mark::Bold),
                    "i" | "em" => to_add.push(Mark::Italic),
                    "u" | "ins" => to_add.push(Mark::Underline),
                    "s" | "del" | "strike" => to_add.push(Mark::Strike),
                    "code" if in_pre == 0 => to_add.push(Mark::Code),
                    "a" => {
                        if let Some(href) = extract_attr(attrs, "href")
                            && is_safe_href(&href)
                        {
                            to_add.push(Mark::Link(href.trim().to_owned()));
                        }
                    }
                    "span" => {
                        if let Some(id) = extract_attr(attrs, "data-pill") {
                            to_add.push(Mark::Pill(id));
                        }
                    }
                    _ => {}
                }
                if let Some(style) = extract_attr(attrs, "style") {
                    parse_style_marks(&style, &mut to_add, &mut to_suppress);
                    // The element's own style wins over its tag: Google Docs wraps a whole
                    // paste in `<b style="font-weight:normal">`.
                    to_add.retain(|m| !to_suppress.iter().any(|r| r.same_kind(m)));
                }

                let pos = text.len();
                let mut frame = TagFrame {
                    tag: tag.clone(),
                    added_marks: Vec::new(),
                    suppressed_marks: Vec::new(),
                };

                for rem in to_suppress {
                    if let Some(idx) = active_marks.iter().position(|a| a.mark.same_kind(&rem)) {
                        let popped = active_marks.remove(idx);
                        if in_pre == 0 && popped.start < pos {
                            marks.push(MarkRange {
                                start: popped.start,
                                end: pos,
                                mark: popped.mark.clone(),
                            });
                        }
                        frame.suppressed_marks.push(popped.mark);
                    }
                }
                for m in to_add {
                    if let Some(idx) = active_marks.iter().position(|a| a.mark.same_kind(&m)) {
                        if active_marks[idx].mark != m {
                            let old = active_marks.remove(idx);
                            if in_pre == 0 && old.start < pos {
                                marks.push(MarkRange {
                                    start: old.start,
                                    end: pos,
                                    mark: old.mark.clone(),
                                });
                            }
                            frame.suppressed_marks.push(old.mark);
                            active_marks.push(ActiveMark { mark: m.clone(), start: pos });
                            frame.added_marks.push(m);
                        }
                    } else {
                        active_marks.push(ActiveMark { mark: m.clone(), start: pos });
                        frame.added_marks.push(m);
                    }
                }

                if !self_closing {
                    *open_tags.entry(tag.clone()).or_default() += 1;
                    stack.push(frame);
                }
            } else {
                // Closing tag
                let pos = text.len();
                let open = open_tags.get(&tag).is_some_and(|&n| n > 0);
                if let Some(frame_idx) = open.then(|| stack.iter().rposition(|f| f.tag == tag)).flatten() {
                    while stack.len() > frame_idx {
                        let frame = stack.pop().unwrap();
                        if let Some(n) = open_tags.get_mut(&frame.tag) {
                            *n -= 1;
                        }
                        for m in frame.added_marks.into_iter().rev() {
                            if let Some(idx) = active_marks.iter().rposition(|a| a.mark == m) {
                                let ended = active_marks.remove(idx);
                                if in_pre == 0 && ended.start < pos {
                                    marks.push(MarkRange {
                                        start: ended.start,
                                        end: pos,
                                        mark: ended.mark,
                                    });
                                }
                            }
                        }
                        for sup in frame.suppressed_marks {
                            if !active_marks.iter().any(|a| a.mark.same_kind(&sup)) {
                                active_marks.push(ActiveMark { mark: sup, start: pos });
                            }
                        }
                        match frame.tag.as_str() {
                            "ul" | "ol" => {
                                list_stack.pop();
                                line_open = false;
                            }
                            "li" => {
                                in_li = in_li.saturating_sub(1);
                                line_open = false;
                            }
                            "blockquote" => {
                                in_quote = in_quote.saturating_sub(1);
                                line_open = false;
                            }
                            "pre" => {
                                in_pre = in_pre.saturating_sub(1);
                                line_open = false;
                            }
                            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                                line_open = false;
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else {
            // Plain text node up to next `<`
            let next_lt = html[i..].find('<').map_or(bytes.len(), |off| i + off);
            let raw_slice = &html[i..next_lt];
            i = next_lt;
            let decoded = decode_entities(raw_slice);
            if in_pre > 0 {
                // Inside `<pre>`, preserve whitespace and split newlines into `Block::Code` paragraphs.
                let mut chars = decoded.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '\r' {
                        if chars.peek() == Some(&'\n') {
                            continue;
                        }
                        text.push('\n');
                        blocks.push(Block::Code);
                        line_has_block = true;
                        line_open = false;
                        line_has_content = false;
                    } else if c == '\n' {
                        // Ignore a single leading newline immediately after `<pre>`
                        if line_open && !line_has_content {
                            line_open = false;
                            continue;
                        }
                        text.push('\n');
                        blocks.push(Block::Code);
                        line_has_block = true;
                        line_open = false;
                        line_has_content = false;
                    } else if c == '\t' || !c.is_control() {
                        text.push(c);
                        line_has_block = true;
                        line_open = false;
                        line_has_content = true;
                    }
                }
            } else {
                // Outside `<pre>`: ignore pure inter-block whitespace if we're between closed blocks and a `<` follows
                if !line_open && decoded.chars().all(char::is_whitespace) && (!line_has_block || line_has_content) {
                    continue;
                }
                if line_open && !line_has_content && decoded.chars().all(char::is_whitespace) && i < bytes.len() && html[i..].starts_with('<') {
                    // Check if the next tag is a block child like `<p>` or `<li>` inside `<blockquote>` or `<ul>`
                    let rest_tag = html[i + 1..].trim_start();
                    let next_is_block = ["p", "div", "ul", "ol", "li", "blockquote", "pre", "h1", "h2", "h3", "h4", "h5", "h6"]
                        .iter()
                        .any(|b| {
                            rest_tag.len() >= b.len()
                                && rest_tag.is_char_boundary(b.len())
                                && rest_tag[..b.len()].eq_ignore_ascii_case(b)
                                && rest_tag[b.len()..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/')
                        });
                    if next_is_block {
                        continue;
                    }
                }
                if !line_open && !decoded.is_empty() && (!line_has_block || line_has_content) {
                    let blk = current_default_block(&list_stack, in_pre, in_quote, in_li);
                    if !line_has_block {
                        if let Some(last) = blocks.last_mut() {
                            *last = blk;
                        }
                        line_has_block = true;
                        line_open = true;
                    } else if !decoded.chars().all(char::is_whitespace) {
                        // Loose text after a closed block (`<p>A</p>B`) starts its own line.
                        start_new_paragraph(
                            &mut text,
                            &mut blocks,
                            &mut active_marks,
                            &mut marks,
                            &mut line_has_block,
                            &mut line_open,
                            &mut line_has_content,
                            blk,
                            in_pre,
                        );
                    }
                }
                // Collapse ASCII whitespace runs (newlines/tabs/spaces) to single spaces in HTML flow
                if decoded.chars().all(char::is_whitespace) {
                    if deferred_br == Some((text.len(), blocks.len())) {
                        continue;
                    }
                } else {
                    if deferred_br == Some((text.len(), blocks.len())) {
                        push_br(&mut text, &mut blocks, &mut soft_breaks, &mut active_marks, &mut marks, in_pre);
                    }
                    deferred_br = None;
                }
                let mut prev_space = text.is_empty() || text.ends_with('\n') || text.ends_with(' ');
                for ch in decoded.chars() {
                    if ch == '\u{00a0}' {
                        text.push('\u{00a0}');
                        prev_space = false;
                        line_has_block = true;
                        line_open = true;
                        line_has_content = true;
                    } else if ch.is_whitespace() {
                        if !prev_space {
                            text.push(' ');
                            prev_space = true;
                            line_has_block = true;
                            line_open = true;
                            line_has_content = true;
                        }
                    } else if !ch.is_control() {
                        text.push(ch);
                        prev_space = false;
                        line_has_block = true;
                        line_open = true;
                        line_has_content = true;
                    }
                }
            }
        }
    }

    let end_pos = text.len();
    flush_active_marks(&mut active_marks, &mut marks, end_pos, in_pre);

    // Strip a single trailing newline inside `<pre>` if it left an empty trailing line
    if blocks.len() > 1
        && blocks.last() == Some(&Block::Code)
        && text.ends_with('\n')
        && !soft_breaks.contains(&((blocks.len() - 1) as u32))
    {
        text.pop();
        blocks.pop();
    }

    let mut edit = TextEdit::from_parts(text, marks, blocks);
    // A line `<br>` made is part of the paragraph before it.
    line_attrs.resize(edit.blocks().len(), ParagraphAttrs::default());
    for &line in &soft_breaks {
        if let Some(i) = (line as usize).checked_sub(1).filter(|_| (line as usize) < line_attrs.len()) {
            line_attrs[i + 1] = line_attrs[i];
        }
    }
    edit.soft_breaks = soft_breaks;
    if line_attrs.iter().any(|a| *a != ParagraphAttrs::default()) {
        edit.set_paragraph_attrs(line_attrs);
    }
    edit
}

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
                && (sub_start < sub_end || start == end || (p.range.start >= start && p.range.end <= end))
            {
                paras.push((p.index, sub_start..sub_end, p.block, p.soft_break && p.range.start > start));
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
                out.push_str(dirs[idx]);
                out.push_str("><code>");
                let mut first = true;
                while idx < paras.len() && paras[idx].2 == Block::Code {
                    if !first {
                        out.push('\n');
                    }
                    first = false;
                    let r = &paras[idx].1;
                    escape_html(&edit.text[r.clone()], &mut out);
                    idx += 1;
                }
                out.push_str("</code></pre>");
            }
            Block::Bullet(_) | Block::Numbered(_) => {
                // Group consecutive list items and emit properly nested `<ul>` / `<ol>`
                let mut open_lists: Vec<ListKind> = Vec::new();
                while idx < paras.len() && paras[idx].2.is_list() {
                    let (_, ref r, blk, _) = paras[idx];
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
                    out.push_str(dirs[idx]);
                    out.push_str(align_css(edit.paragraph(paras[idx].0).align));
                    out.push('>');
                    serialize_inline(edit, r.clone(), &mut out);
                    idx += 1;
                    while idx < paras.len() && paras[idx].3 && paras[idx].2 == blk {
                        out.push_str("<br>");
                        serialize_inline(edit, paras[idx].1.clone(), &mut out);
                        idx += 1;
                    }
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
                out.push_str("<blockquote");
                out.push_str(dirs[idx]);
                out.push('>');
                serialize_inline(edit, paras[idx].1.clone(), &mut out);
                idx += 1;
                while idx < paras.len() && paras[idx].3 && paras[idx].2 == Block::Quote {
                    out.push_str("<br>");
                    serialize_inline(edit, paras[idx].1.clone(), &mut out);
                    idx += 1;
                }
                out.push_str("</blockquote>");
            }
            Block::Paragraph => {
                let attrs = edit.paragraph(paras[idx].0);
                let named = edit.html_styles().iter().find(|s| s.style == attrs.style);
                let tag = named.map_or("p", |s| s.tag);
                out.push('<');
                out.push_str(tag);
                if let Some(class) = named.and_then(|s| s.class) {
                    out.push_str(" class=\"");
                    escape_html(class, &mut out);
                    out.push('"');
                }
                out.push_str(dirs[idx]);
                out.push_str(align_css(attrs.align));
                out.push('>');
                serialize_inline(edit, paras[idx].1.clone(), &mut out);
                idx += 1;
                while idx < paras.len() && paras[idx].3 && paras[idx].2 == Block::Paragraph {
                    out.push_str("<br>");
                    serialize_inline(edit, paras[idx].1.clone(), &mut out);
                    idx += 1;
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
    }
    out
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

/// The inline CSS of the marks among `active` that have no element of their own: family,
/// size, colour and highlight.
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
            Mark::Color(c) => css.push_str(&format!("color:{};", rgb(*c))),
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
    use crate::el::{Direction, Flow};
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
        // Canonical nesting order: Link, Bold, Italic, Underline, Strike, Code
        active.sort_by_key(|m| match m {
            Mark::Link(_) => 0u8,
            Mark::Bold => 1,
            Mark::Italic => 2,
            Mark::Underline => 3,
            Mark::Strike => 4,
            Mark::Code => 5,
            Mark::Pill(_) => 6,
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
