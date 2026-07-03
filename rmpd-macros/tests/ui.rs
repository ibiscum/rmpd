#[test]
fn command_metadata_compile_fail_cases() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/compile_fail/*.rs");
}
