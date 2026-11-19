//! A text field's locked atoms in the accessibility tree: each a group named by its label, so
//! a reader announces "Tabelle, group" where the caret steps over one.

use accesskit::{Node as AkNode, NodeId as AkId, Rect as AkRect, Role};

use super::*;
use super::a11y_tree::{atom_id, round};
use crate::text::TextFlow;

/// How many atoms one field publishes.
const MAX_ATOMS: usize = 256;

impl<S: 'static> Runtime<S> {
    /// Node `i`'s atoms, pushed after it as children of `out[me]`.
    pub(super) fn a11y_atoms(&self, i: usize, w: &a11y_tree::Walk, me: usize, out: &mut Vec<(AkId, AkNode)>) {
        let n = &self.nodes[i];
        let Some(spec) = n.text.as_deref() else { return };
        if !spec.cold.attrs.iter().any(|a| a.atom.is_some()) || spec.redacted { return; }
        let Some(TextFlow::Paragraphs(layout)) = n.text_layout.as_ref().map(crate::text::TextLayout::view) else { return };
        let (s, r) = (self.env.scale, self.drawn_box(i));
        let atoms = spec.cold.attrs.iter().enumerate().filter_map(|(k, a)| Some((k, a.atom?)));
        for (slot, (k, height)) in atoms.take(MAX_ATOMS).enumerate() {
            let Some(p) = layout.paragraphs.get(k) else { continue };
            let middle = r[1] + n.inset[1] + (p.y + p.height / 2.0) / s;
            let mut group = AkNode::new(Role::Group);
            group.set_label(spec.content.get(p.start..p.end).unwrap_or_default());
            group.set_bounds(AkRect {
                x0: round(r[0] + w.shift[0]), y0: round(middle - height / 2.0 + w.shift[1]),
                x1: round(r[2] + w.shift[0]), y1: round(middle + height / 2.0 + w.shift[1]),
            });
            let id = atom_id(n.id, slot);
            out[me].1.push_child(id);
            out.push((id, group));
        }
    }
}
