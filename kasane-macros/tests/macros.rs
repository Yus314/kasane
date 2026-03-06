#[test]
fn test_plugin_pass() {
    trybuild::TestCases::new().pass("tests/pass/plugin_*.rs");
}

#[test]
fn test_component_pass() {
    trybuild::TestCases::new().pass("tests/pass/component_*.rs");
}

#[test]
fn test_fail() {
    trybuild::TestCases::new().compile_fail("tests/fail/*.rs");
}
