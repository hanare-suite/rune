use super::*;
use crate::ParagraphAttrs;

fn attrs(indent: f32, first: f32) -> ParagraphAttrs {
    let mut a = ParagraphAttrs::default();
    a.indent = indent;
    a.first_line_indent = first;
    a
}

#[test]
fn a_marker_starts_where_the_layout_starts_the_first_line() {
    // The layout rounds each indent on its own and never starts a line left of 0.
    assert_eq!(marker_x(0.0, Some(&attrs(10.25, 10.25)), 1.0), 20.0);
    assert_eq!(marker_x(4.0, Some(&attrs(10.4, 10.4)), 1.0), 24.0);
    assert_eq!(marker_x(0.0, Some(&attrs(0.0, -18.0)), 1.0), 0.0);
    assert_eq!(marker_x(10.0, Some(&attrs(0.0, -18.0)), 1.0), 0.0);
    assert_eq!(marker_x(10.0, Some(&attrs(36.0, -18.0)), 2.0), 46.0);
    assert_eq!(marker_x(10.0, None, 1.0), 10.0);
}
