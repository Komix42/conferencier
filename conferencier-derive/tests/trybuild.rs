#[test]
fn trybuild_suite() {
    let t = trybuild::TestCases::new();
    t.pass("tests/trybuild/pass_basic.rs");
    t.compile_fail("tests/trybuild/fail_duplicate_keys.rs");
    t.compile_fail("tests/trybuild/fail_unsupported_type.rs");
    t.compile_fail("tests/trybuild/fail_conflicting_attrs.rs");
}
