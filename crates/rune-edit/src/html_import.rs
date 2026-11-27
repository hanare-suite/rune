//! Loading HTML into a field, and how paragraph styles read and write as HTML.

use super::*;

impl TextEdit {
    /// Parse an HTML fragment (`<b>`, `<strong>`, `<i>`, `<em>`, `<u>`, `<s>`, `<del>`, `<strike>`,
    /// `<a href>`, `<code>`, `<pre>`, `<ul>`, `<ol>`, `<li>`, `<blockquote>`, `<p>`, `<div>`,
    /// `<br>`, and inline `style` for `font-weight`, `font-style`, `text-decoration`) into a
    /// [`TextEdit`].
    /// Unknown tags keep their text and strip the wrapper; `<script>` and `<style>` are dropped.
    /// Text colours (`style="color:..."`, `<font color>`) are stripped by default; use
    /// [`from_html_with_color`](Self::from_html_with_color) to load saved coloured HTML.
    pub fn from_html(html_str: &str) -> TextEdit {
        let mut edit = html::parse_html(html_str, &[]).rich();
        edit.allowed_marks = None;
        // A loaded page's alignment is not a document: only a paste keeps paragraph attributes.
        edit.attrs.clear();
        edit.sync_invariants();
        edit
    }

    /// Like [`from_html`](Self::from_html), but opts the returned field into [`Mark::Color`]
    /// (as if built with [`allow_color(true)`](Self::allow_color)) and keeps CSS `style="color:..."`
    /// and `<font color>` text colours.
    ///
    /// ```
    /// use rune_edit::TextEdit;
    /// let edit = TextEdit::from_html_with_color(r#"<p><span style="color:#d93025">red</span></p>"#);
    /// assert_eq!(edit.active_color(), Some("#d93025"));
    /// ```
    pub fn from_html_with_color(html_str: &str) -> TextEdit {
        let mut edit = html::parse_html(html_str, &[]).rich();
        edit.allowed_marks = None;
        edit.attrs.clear();
        edit.allow_color(true)
    }

    /// Says which HTML element each paragraph style is: copying writes a paragraph of style
    /// `n` as the first entry that names it (`<h1>`, `<p class="title">`), and pasting gives
    /// a block that matches an entry its style. Without entries headings paste as bold text.
    pub fn set_html_styles(&mut self, styles: impl Into<Arc<[HtmlStyle]>>) {
        self.html_styles = styles.into();
    }

    pub(crate) fn html_styles(&self) -> &[HtmlStyle] {
        &self.html_styles
    }
}
