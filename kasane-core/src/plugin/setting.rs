//! Typed plugin setting values with schema-validated types.

use std::fmt;

use compact_str::CompactString;

/// A typed setting value for the plugin settings system.
///
/// Each variant corresponds to a manifest-declared type. Values are validated
/// at load time against the manifest schema and at runtime via `set-setting`.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    Str(CompactString),
}

impl SettingValue {
    /// Return the type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            SettingValue::Bool(_) => "bool",
            SettingValue::Integer(_) => "integer",
            SettingValue::Float(_) => "float",
            SettingValue::Str(_) => "string",
        }
    }
}

impl fmt::Display for SettingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingValue::Bool(v) => write!(f, "{v}"),
            SettingValue::Integer(v) => write!(f, "{v}"),
            SettingValue::Float(v) => write!(f, "{v}"),
            SettingValue::Str(v) => write!(f, "{v}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_and_eq() {
        let a = SettingValue::Bool(true);
        let b = a.clone();
        assert_eq!(a, b);

        let c = SettingValue::Integer(42);
        let d = SettingValue::Integer(42);
        assert_eq!(c, d);
        assert_ne!(a, c);
    }

    #[test]
    fn display() {
        assert_eq!(SettingValue::Bool(true).to_string(), "true");
        assert_eq!(SettingValue::Integer(-5).to_string(), "-5");
        assert_eq!(SettingValue::Float(3.14).to_string(), "3.14");
        assert_eq!(
            SettingValue::Str(CompactString::new("hello")).to_string(),
            "hello"
        );
    }

    #[test]
    fn type_name() {
        assert_eq!(SettingValue::Bool(false).type_name(), "bool");
        assert_eq!(SettingValue::Integer(0).type_name(), "integer");
        assert_eq!(SettingValue::Float(0.0).type_name(), "float");
        assert_eq!(
            SettingValue::Str(CompactString::new("")).type_name(),
            "string"
        );
    }
}
