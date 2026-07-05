#[test]
fn crate_compiles_and_links() {
    // Verify the crate is reachable and compiles
    assert!(std::any::type_name_of_val(&()).contains("()"));
}
