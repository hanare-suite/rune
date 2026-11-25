//! `Cx::pages`: the page count of a paged text field, read like a layout rect.

use super::*;
use super::key_index::{self, KeyIndex};

/// The prefix of a page-count read's key (`Cx::pages`): its "rect" holds the count as its width.
pub(super) const PAGES: &str = "\u{1}pages:";

impl<S> Cx<S> {
    /// How many pages the paged text field keyed `key` holds ([`El::page_flow`](crate::El::page_flow)),
    /// or `None` when there is no such field yet. It is the count of the layout this commit
    /// made: a view that reads it re-runs, in the same commit, when an edit adds or removes a
    /// page, so the pages an app draws behind the field and the count it shows are never a
    /// frame behind the text. Nothing re-runs while the count holds.
    #[track_caller]
    pub fn pages(&self, key: &str) -> Option<usize> {
        self.layout_with(&format!("{PAGES}{key}"), |r| r[2] as usize)
    }
}

impl RuntimeCore {
    /// The "rect" of the page-count read of the field keyed `key`: its count as the width.
    pub(super) fn pages_of(&self, nodes: &[Node], index: Option<&KeyIndex>, key: &str) -> Option<[f32; 4]> {
        let i = match index {
            Some(index) => index.first(nodes, key),
            None => key_index::scan_first(nodes, key),
        }?;
        let flow = nodes[i].text.as_ref()?.cold.page_flow?;
        Some([0.0, 0.0, flow.pages(nodes[i].local.h) as f32, 0.0])
    }
}
