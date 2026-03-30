//! Unified serialization format for cross-boundary plugin communication.
//!
//! `ChannelValue` replaces `Box<dyn Any + Send>` in pub/sub and extension
//! point systems with a serialized form that can cross WASM boundaries.

use std::borrow::Cow;

use serde::{Serialize, de::DeserializeOwned};

/// A serialized value for cross-boundary plugin communication.
///
/// Data is stored as MessagePack bytes with a type hint for diagnostics.
/// Native plugins use typed wrappers; WASM plugins receive raw bytes.
#[derive(Debug, Clone)]
pub struct ChannelValue {
    data: Vec<u8>,
    type_hint: Cow<'static, str>,
}

impl ChannelValue {
    /// Create a new `ChannelValue` by serializing a value.
    pub fn new<T: Serialize + 'static>(value: &T) -> Result<Self, rmp_serde::encode::Error> {
        let data = rmp_serde::to_vec(value)?;
        Ok(Self {
            data,
            type_hint: Cow::Borrowed(std::any::type_name::<T>()),
        })
    }

    /// Create a `ChannelValue` from raw bytes (for WASM boundary).
    pub fn from_raw(data: Vec<u8>, type_hint: impl Into<Cow<'static, str>>) -> Self {
        Self {
            data,
            type_hint: type_hint.into(),
        }
    }

    /// Deserialize the contained value.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, rmp_serde::decode::Error> {
        rmp_serde::from_slice(&self.data)
    }

    /// Get the raw serialized data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the type hint string.
    pub fn type_hint(&self) -> &str {
        &self.type_hint
    }
}

impl PartialEq for ChannelValue {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl Eq for ChannelValue {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn round_trip_u32() {
        let val = ChannelValue::new(&42u32).unwrap();
        assert_eq!(val.deserialize::<u32>().unwrap(), 42);
    }

    #[test]
    fn round_trip_string() {
        let val = ChannelValue::new(&"hello".to_string()).unwrap();
        assert_eq!(val.deserialize::<String>().unwrap(), "hello");
    }

    #[test]
    fn round_trip_vec() {
        let val = ChannelValue::new(&vec![1u32, 2, 3]).unwrap();
        assert_eq!(val.deserialize::<Vec<u32>>().unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn round_trip_struct() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct TestData {
            x: i32,
            y: String,
        }
        let data = TestData {
            x: 42,
            y: "test".into(),
        };
        let val = ChannelValue::new(&data).unwrap();
        assert_eq!(val.deserialize::<TestData>().unwrap(), data);
    }

    #[test]
    fn type_hint_preserved() {
        let val = ChannelValue::new(&42u32).unwrap();
        assert!(val.type_hint().contains("u32"));
    }

    #[test]
    fn from_raw_round_trip() {
        let original = ChannelValue::new(&42u32).unwrap();
        let raw = ChannelValue::from_raw(original.data().to_vec(), "u32");
        assert_eq!(raw.deserialize::<u32>().unwrap(), 42);
    }

    #[test]
    fn from_raw_accepts_string() {
        let original = ChannelValue::new(&"hello".to_string()).unwrap();
        let hint = "std::string::String".to_string();
        let raw = ChannelValue::from_raw(original.data().to_vec(), hint);
        assert_eq!(raw.deserialize::<String>().unwrap(), "hello");
        assert_eq!(raw.type_hint(), "std::string::String");
    }

    #[test]
    fn from_raw_accepts_static_str() {
        let original = ChannelValue::new(&42u32).unwrap();
        let raw = ChannelValue::from_raw(original.data().to_vec(), "u32");
        assert_eq!(raw.deserialize::<u32>().unwrap(), 42);
        assert_eq!(raw.type_hint(), "u32");
    }

    #[test]
    fn equality() {
        let a = ChannelValue::new(&42u32).unwrap();
        let b = ChannelValue::new(&42u32).unwrap();
        assert_eq!(a, b);

        let c = ChannelValue::new(&99u32).unwrap();
        assert_ne!(a, c);
    }
}
