use std::fmt;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TopicId(pub CompactString);

impl TopicId {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtensionPointId(pub CompactString);

impl ExtensionPointId {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransformTarget(CompactString);

impl TransformTarget {
    pub const BUFFER: Self = Self(CompactString::const_new("kasane.buffer"));
    pub const BUFFER_LINE: Self = Self(CompactString::const_new("kasane.buffer.line"));
    pub const STATUS_BAR: Self = Self(CompactString::const_new("kasane.status-bar"));
    pub const MENU: Self = Self(CompactString::const_new("kasane.menu"));
    pub const MENU_PROMPT: Self = Self(CompactString::const_new("kasane.menu.prompt"));
    pub const MENU_INLINE: Self = Self(CompactString::const_new("kasane.menu.inline"));
    pub const MENU_SEARCH: Self = Self(CompactString::const_new("kasane.menu.search"));
    pub const INFO: Self = Self(CompactString::const_new("kasane.info"));
    pub const INFO_PROMPT: Self = Self(CompactString::const_new("kasane.info.prompt"));
    pub const INFO_MODAL: Self = Self(CompactString::const_new("kasane.info.modal"));

    pub fn buffer_line(line: usize) -> Self {
        Self(CompactString::from(format!("kasane.buffer.line.{line}")))
    }

    pub fn as_buffer_line(&self) -> Option<usize> {
        self.0
            .strip_prefix("kasane.buffer.line.")
            .and_then(|s| s.parse().ok())
    }

    pub fn parent(&self) -> Option<Self> {
        let s = if self.as_buffer_line().is_some() {
            "kasane.buffer.line"
        } else {
            self.0.as_str()
        };
        if s.matches('.').count() <= 1 {
            return None;
        }
        s.rsplit_once('.')
            .map(|(parent, _)| Self(CompactString::from(parent)))
    }

    pub fn refinement_chain(&self) -> Vec<TransformTarget> {
        match self.parent() {
            Some(parent) => vec![parent, self.clone()],
            None => vec![self.clone()],
        }
    }

    pub fn is_refinement(&self) -> bool {
        self.parent().is_some()
    }

    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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
        assert_eq!(SettingValue::Float(3.14).type_name(), "float");
        assert_eq!(
            SettingValue::Str(CompactString::new("hello")).type_name(),
            "string"
        );

        assert_eq!(SettingValue::Bool(true).to_string(), "true");
        assert_eq!(SettingValue::Integer(-5).to_string(), "-5");
        assert_eq!(SettingValue::Float(3.14).to_string(), "3.14");
        assert_eq!(
            SettingValue::Str(CompactString::new("hello")).to_string(),
            "hello"
        );
    }

    #[test]
    fn topic_and_extension_ids_round_trip_strings() {
        let topic = TopicId::new("test.counter");
        let extension = ExtensionPointId::new("test.items");

        assert_eq!(topic.as_str(), "test.counter");
        assert_eq!(extension.as_str(), "test.items");
    }

    #[test]
    fn transform_target_hierarchy_handles_parametric_targets() {
        let target = TransformTarget::buffer_line(42);
        assert_eq!(target.as_buffer_line(), Some(42));
        assert_eq!(target.parent(), Some(TransformTarget::BUFFER));
        assert_eq!(
            target.refinement_chain(),
            vec![TransformTarget::BUFFER, target.clone()]
        );
        assert!(target.is_refinement());
    }
}
