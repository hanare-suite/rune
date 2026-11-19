//! A span's own font family and its sub- or superscript, as style ranges of a layout.

use parley::{FontFamilyName, LineHeight, RangedBuilder, StyleProperty, VerticalAlign};
use parley::fontique::Collection;

use super::{Ink, Span};
use crate::edit::Baseline;

/// The stack each family `spans` name is shaped in: the family, then `rest`. A name the
/// collection doesn't hold gets none, so its run keeps the text's own family.
pub(super) fn families<'a>(spans: &'a [Span], collection: &mut Collection, rest: &[FontFamilyName<'a>]) -> Families<'a> {
    let mut out: Vec<(&'a str, Vec<FontFamilyName<'a>>)> = Vec::new();
    for name in spans.iter().filter_map(|s| s.family.as_deref()) {
        if out.iter().any(|(n, _)| *n == name) || collection.family_id(name).is_none() {
            continue;
        }
        out.push((name, std::iter::once(FontFamilyName::named(name)).chain(rest.iter().cloned()).collect()));
    }
    Families(out)
}

pub(super) struct Families<'a>(Vec<(&'a str, Vec<FontFamilyName<'a>>)>);

impl<'a> Families<'a> {
    pub(super) fn get(&self, name: &str) -> Option<&Vec<FontFamilyName<'a>>> {
        self.0.iter().find(|(n, _)| *n == name).map(|(_, list)| list)
    }
}

/// Sets each sub- and superscript span at [`Baseline::SCALE`] of its size, moved by
/// [`Baseline::SHIFT`] of the em it would have had, with a line box as small as its glyphs.
pub(super) fn push_baselines(b: &mut RangedBuilder<'_, Ink>, spans: &[Span], size: f32, len: usize) {
    for s in spans {
        let Some(baseline) = s.baseline else { continue };
        let r = (s.start as usize).min(len)..(s.end as usize).min(len);
        if r.is_empty() { continue; }
        let full = s.size.unwrap_or(size);
        let shift = full * Baseline::SHIFT;
        b.push(StyleProperty::FontSize(full * Baseline::SCALE), r.clone());
        b.push(LineHeight::FontSizeRelative(1.0), r.clone());
        b.push(StyleProperty::VerticalAlign(VerticalAlign::length(if baseline == Baseline::Super { shift } else { -shift })), r);
    }
}
