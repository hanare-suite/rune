//! Safe HTML subset import and export for [`TextEdit`](super::TextEdit).
//!
//! Supported elements:
//! - Inline: `<b>`, `<strong>`, `<i>`, `<em>`, `<u>`, `<ins>`, `<s>`, `<del>`, `<strike>`,
//!   `<a href="...">`, `<code>`, `<sub>`, `<sup>`, `<font color="...">`, CSS `style` attributes
//!   (`font-weight`, `font-style`, `text-decoration`, `text-decoration-line`, `color`,
//!   `font-family`, `font-size`, `background-color`, `vertical-align`), and
//!   `<span data-pill="id">` for [`Mark::Pill`].
//! - Block: `<p>`, `<div>`, `<br>`, `<ul>`, `<ol>`, `<li>`, `<blockquote>`, `<pre>`, with
//!   `text-align`, and the elements a field's [`HtmlStyle`]s name for its paragraph styles
//!   (`<h1>`, `<p class="title">`). A locked atom copied whole is `<p data-atom="id"
//!   data-atom-height="points">` around its label, and a copy that ends behind a line break
//!   ends in an empty `<p>`.
//! - Stripped with content: `<script>`, `<style>`, `<head>`.
//! - Unknown tags drop their wrapper and keep their text. Nothing is executed, and dangerous
//!   link schemes (`javascript:`, `vbscript:`, `data:`) are rejected.

use super::{Baseline, HtmlStyle, Mark, MarkColor, MarkSize, ParagraphAlign};

pub(crate) mod color;
mod parse;
mod serialize;

pub(crate) use color::normalize_hex;
pub(crate) use parse::parse_html;
pub(crate) use serialize::serialize_html;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Numbered,
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

/// The atom a block element is the copy of (`data-atom`, the id it was copied under, and
/// `data-atom-height`, its height in points).
fn block_atom(attrs: &str) -> Option<(u64, f32)> {
    let id = extract_attr(attrs, "data-atom")?.trim().parse().ok()?;
    let height = extract_attr(attrs, "data-atom-height").and_then(|h| h.trim().parse::<f32>().ok()).filter(|h| h.is_finite()).unwrap_or(1.0);
    Some((id, height.max(1.0)))
}

/// The paragraph style `styles` gives the element `tag` with these attributes.
fn block_style(styles: &[HtmlStyle], tag: &str, attrs: &str) -> Option<u16> {
    let class = extract_attr(attrs, "class");
    let has = |name: &str| class.as_deref().is_some_and(|c| c.split_whitespace().any(|c| c.eq_ignore_ascii_case(name)));
    // An entry with a class is the closer match, so it is asked first.
    styles.iter().find(|s| s.tag == tag && s.class.is_some_and(has)).or_else(|| styles.iter().find(|s| s.tag == tag && s.class.is_none())).map(|s| s.style)
}

/// The whitespace-separated tokens of a CSS value, a function's arguments (`rgb(1, 2, 3)`) staying one.
fn css_tokens(val: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0usize, None);
    for (i, c) in val.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => {
                if let Some(s) = start.take() { out.push(&val[s..i]); }
                continue;
            }
            _ => {}
        }
        start.get_or_insert(i);
    }
    if let Some(s) = start { out.push(&val[s..]); }
    out
}

/// Parses inline CSS `style="..."` for the marks this model keeps: `font-weight`, `font-style`,
/// `text-decoration`, `color`, `font-family`, `font-size`, `background-color` and `vertical-align`.
fn parse_style_marks(style: &str, add: &mut Vec<Mark>, remove: &mut Vec<Mark>) {
    for decl in style.split(';') {
        let Some((prop, val)) = decl.split_once(':') else { continue };
        let prop = prop.trim();
        let val = val.trim();
        if prop.eq_ignore_ascii_case("font-weight") {
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
        } else if prop.eq_ignore_ascii_case("color")
            && let Some(hex) = color::parse_css_color(val)
        {
            add.push(Mark::Color(hex));
        } else if prop.eq_ignore_ascii_case("font-family") {
            if let Some(name) = css_family(val) { add.push(Mark::family(name)); }
        } else if prop.eq_ignore_ascii_case("font-size") {
            if let Some(points) = css_points(val) { add.push(Mark::Size(MarkSize::pt(points))); }
        } else if prop.eq_ignore_ascii_case("background-color") || prop.eq_ignore_ascii_case("background") || prop.eq_ignore_ascii_case("mso-highlight") {
            // The whole value first (`rgb(255, 255, 0)` has spaces in it), then its tokens (`yellow no-repeat`).
            let hex = color::parse_css_color(val).or_else(|| css_tokens(val).into_iter().find_map(color::parse_css_color));
            let highlight = hex.and_then(|hex| rune_core::color::hex(&hex));
            if let Some(c) = highlight { add.push(Mark::Highlight(MarkColor::new(c))); }
        } else if prop.eq_ignore_ascii_case("vertical-align") {
            if val.eq_ignore_ascii_case("super") { add.push(Mark::Baseline(Baseline::Super)); }
            else if val.eq_ignore_ascii_case("sub") { add.push(Mark::Baseline(Baseline::Sub)); }
        }
    }
}
