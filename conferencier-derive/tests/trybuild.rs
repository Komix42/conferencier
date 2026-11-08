#[test]
fn trybuild_suite() {
    let t = trybuild::TestCases::new();
    t.pass("tests/trybuild/pass_basic.rs");
    t.pass("tests/trybuild/pass_datetime_defaults.rs");
    t.compile_fail("tests/trybuild/fail_duplicate_keys.rs");
    t.compile_fail("tests/trybuild/fail_unsupported_type.rs");
    t.compile_fail("tests/trybuild/fail_conflicting_attrs.rs");
    t.compile_fail("tests/trybuild/fail_invalid_datetime.rs");
    t.compile_fail("tests/trybuild/fail_negative_unsigned.rs");
}
