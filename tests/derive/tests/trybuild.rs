#[test]
fn trybuild_suite() {
    let t = trybuild::TestCases::new();
    t.pass("tests/trybuild/pass_basic.rs");
    t.pass("tests/trybuild/pass_generated_hygiene.rs");
    t.pass("tests/trybuild/pass_datetime_defaults.rs");
    t.pass("tests/trybuild/pass_required_datetime.rs");
    t.pass("tests/trybuild/pass_ignore_expr.rs");
    t.pass("tests/trybuild/pass_ignored_key_collision.rs");
    t.pass("tests/trybuild/pass_generic_scope.rs");
    t.pass("tests/trybuild/pass_generic_scope_bounded.rs");
    t.pass("tests/trybuild/pass_hostile_method_names.rs");
    t.pass("tests/trybuild/pass_table_shapes.rs");
    t.pass("tests/trybuild/pass_scope_table_section.rs");
    t.pass("tests/trybuild/pass_generic_table.rs");
    t.pass("tests/trybuild/pass_table_confer_prefix.rs");
    t.pass("tests/trybuild/pass_deny_unknown_keys.rs");
}
