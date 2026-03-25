#[test]
fn test_define_plugin_pass() {
    trybuild::TestCases::new().pass("tests/pass/define_plugin_*.rs");
}

#[test]
fn test_define_plugin_fail() {
    trybuild::TestCases::new().compile_fail("tests/fail/define_plugin_*.rs");
}

#[test]
fn test_plugin_fail() {
    trybuild::TestCases::new().compile_fail("tests/fail/plugin_*.rs");
}
