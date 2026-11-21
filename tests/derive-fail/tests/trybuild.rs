// Compile-fail cases; their .stderr snapshots come from Rust 1.85 (the MSRV).
// CI also runs them on stable with TRYBUILD=overwrite, which checks only that they still fail.
#[test]
#[ignore = "MSRV diagnostics; run with: cargo +1.85 test -p derive-fail-tests -- --ignored"]
fn trybuild_suite() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/fail_duplicate_keys.rs");
    t.compile_fail("tests/trybuild/fail_unsupported_type.rs");
    t.compile_fail("tests/trybuild/fail_nested_vec.rs");
    t.compile_fail("tests/trybuild/fail_reference_field.rs");
    t.compile_fail("tests/trybuild/fail_type_alias.rs");
    t.compile_fail("tests/trybuild/fail_conflicting_attrs.rs");
    t.compile_fail("tests/trybuild/fail_default_types.rs");
    t.compile_fail("tests/trybuild/fail_default_options.rs");
    t.compile_fail("tests/trybuild/fail_default_aliases.rs");
    t.compile_fail("tests/trybuild/fail_field_defaults.rs");
    t.compile_fail("tests/trybuild/fail_init_removed.rs");
    t.compile_fail("tests/trybuild/fail_ignore_conflicts.rs");
    t.compile_fail("tests/trybuild/fail_duplicate_ignore.rs");
    t.compile_fail("tests/trybuild/fail_ignore_expr_syntax.rs");
    t.compile_fail("tests/trybuild/fail_ignore_expr_type.rs");
    t.compile_fail("tests/trybuild/fail_ignore_expr_scope.rs");
    t.compile_fail("tests/trybuild/fail_ignore_expr_await.rs");
    t.compile_fail("tests/trybuild/fail_ignore_expr_try.rs");
    t.compile_fail("tests/trybuild/fail_table_not_implemented.rs");
    t.compile_fail("tests/trybuild/fail_table_missing_attribute.rs");
    t.compile_fail("tests/trybuild/fail_table_conflicts.rs");
    t.compile_fail("tests/trybuild/fail_table_shape.rs");
    t.compile_fail("tests/trybuild/fail_table_struct.rs");
    t.compile_fail("tests/trybuild/fail_table_section.rs");
    t.compile_fail("tests/trybuild/fail_table_ignore_expr_scope.rs");
    t.compile_fail("tests/trybuild/fail_confer_prefix.rs");
    t.compile_fail("tests/trybuild/fail_deny_unknown_keys_duplicate.rs");
    t.compile_fail("tests/trybuild/fail_deny_unknown_keys_value.rs");
    t.compile_fail("tests/trybuild/fail_deny_unknown_keys_field.rs");
    t.compile_fail("tests/trybuild/fail_unknown_attribute.rs");
    t.compile_fail("tests/trybuild/fail_ignored_field_not_send.rs");
}
