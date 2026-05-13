//! Typed plugin setting values with schema-validated types.

use std::fmt;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// A plugin setting value, tagged with its concrete type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    Str(CompactString),
}

impl SettingValue {
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
    fn setting_value_type_name_and_display() {
        assert_eq!(SettingValue::Bool(true).type_name(), "bool");
        assert_eq!(SettingValue::Integer(-5).type_name(), "integer");
        assert_eq!(SettingValue::Float(2.5).type_name(), "float");
        assert_eq!(
            SettingValue::Str(CompactString::new("hello")).type_name(),
            "string"
        );

        assert_eq!(SettingValue::Bool(true).to_string(), "true");
        assert_eq!(SettingValue::Integer(-5).to_string(), "-5");
        assert_eq!(SettingValue::Float(2.5).to_string(), "2.5");
        assert_eq!(
            SettingValue::Str(CompactString::new("hello")).to_string(),
            "hello"
        );
    }
}
