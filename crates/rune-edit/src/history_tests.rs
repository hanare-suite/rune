use super::*;

#[test]
fn a_group_opened_on_a_secure_field_records_from_when_it_turns_plain() {
    let mut e = TextEdit::new("pw").secure();
    e.undo_group(|e| {
        e.insert("1");
        assert!(matches!(e.group, Group::Unrecorded));
        e.set_secure(false);
        e.insert("a");
        e.set_secure(false);
        e.insert("b");
    });
    assert_eq!(e.text, "pw1ab");
    e.undo();
    assert_eq!(e.text, "pw1", "one step, from when the field turned plain, not moved by a second set_secure(false)");
    e.undo();
    assert_eq!(e.text, "pw1");
}

#[test]
fn a_group_is_closed_in_clones_and_holds_undo_until_it_closes() {
    let mut e = TextEdit::new("a");
    e.undo_group(|e| {
        e.insert("b");
        assert!(e.group.is_open() && !e.clone().group.is_open());
        e.undo();
        assert_eq!(e.text, "ab", "undo waits for the group to close");
        e.set_secure(true);
        assert!(matches!(e.group, Group::Unrecorded), "turning secure drops the baseline's text");
        e.set_secure(false);
        e.replace_text("c");
    });
    assert!(!e.group.is_open());
    e.undo();
    assert_eq!(e.text, "c", "replace_text in the group discards the history before it");
}

#[test]
fn a_saved_history_with_a_highlight_restores_into_a_fresh_field() {
    use rune_core::color::oklch;
    let marks = || vec![MarkRange::new(0..5, Mark::Highlight(MarkColor::new(oklch(0.9, 0.15, 100.0))))];
    let mut e = TextEdit::from_parts("hello world", marks(), vec![]);
    e.set_selection(11, 11);
    e.insert("!");
    let saved = e.history_json(32, 64 * 1024).expect("history json");
    let mut fresh = TextEdit::from_parts("hello world!", marks(), vec![]);
    assert!(fresh.restore_history_json(&saved), "a highlight survives the hex form the history is stored in");
}

#[test]
fn a_mark_colour_is_on_the_8_bit_grid_and_survives_hex_bit_for_bit() {
    use rune_core::color::{hex, oklcha};
    for c in [oklcha(0.9, 0.15, 100.0, 1.0), oklcha(0.55, 0.02, 260.0, 0.45), oklcha(0.3, 0.1, 20.0, 0.1234)] {
        let once = MarkColor::new(c);
        assert_eq!(MarkColor::new(once.get()), once, "snapping is idempotent");
        let back = MarkColor::new(hex(&once.get().hex()).unwrap());
        assert_eq!(back, once, "hex round trip is exact");
    }
}
