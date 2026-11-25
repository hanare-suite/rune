//! What a [`Span`] carries for a document's run marks: a font family by name, a flat highlight and
//! a baseline shift.

use crate::*;

/// Where a baseline-shifted run sits: at 58 % of its size, a third of its em below the
/// baseline or above it, as Word sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Baseline {
    Sub,
    Super,
}

impl Baseline {
    /// The run's size as a share of the size it would have had.
    pub const SCALE: f32 = 0.58;
    /// How far the run moves, as a share of the em it would have had.
    pub const SHIFT: f32 = 1.0 / 3.0;
}

impl Span {
    /// The run in the family named `name`, when the app registered one.
    pub fn family(mut self, name: &str) -> Span {
        self.family = Some(name.into());
        self
    }

    /// A flat `color` background: the run's full height, square corners, no padding.
    pub fn highlight(mut self, color: Color) -> Span {
        (self.bg, self.bg_pad, self.bg_flat) = (Some(color), 0.0, true);
        self
    }

    /// The run as a subscript or superscript.
    pub fn baseline(mut self, baseline: Baseline) -> Span {
        self.baseline = Some(baseline);
        self
    }
}
