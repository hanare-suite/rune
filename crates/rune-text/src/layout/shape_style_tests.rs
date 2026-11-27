use super::*;

#[test]
fn an_alias_set_again_or_to_a_family_already_held_leaks_nothing_new() {
    alias_family("Alias Leak Test", "Alias Leak Family");
    let held = alias_of("alias leak test").expect("set");
    alias_family("ALIAS LEAK TEST", "Alias Leak Family");
    assert!(std::ptr::eq(alias_of("Alias Leak Test").expect("set"), held), "the same alias again keeps its strings");
    alias_family("Alias Leak Other", "Alias Leak Family");
    assert!(std::ptr::eq(alias_of("Alias Leak Other").expect("set"), held), "a family already held is not copied again");
    alias_family("Alias Leak Test", "Alias Leak Changed");
    assert_eq!(alias_of("Alias Leak Test"), Some("Alias Leak Changed"));
}
