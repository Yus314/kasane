use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::manifest::{
    AuthoritiesSection, CapabilitiesSection, HandlersSection, PluginManifest, PluginSection,
    SettingSchema, ViewSection,
};

const MAGIC: &[u8; 8] = b"KASPKG1\0";
const FORMAT_VERSION: u16 = 1;
const PREAMBLE_LEN: usize = 8 + 2 + 4;
const DIGEST_PLACEHOLDER: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("manifest validation failed: {0}")]
    Manifest(#[from] crate::manifest::ManifestError),

    #[error("JSON encode/decode error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid package magic")]
    InvalidMagic,

    #[error("unsupported package format version: {0}")]
    UnsupportedFormatVersion(u16),

    #[error("truncated package preamble")]
    TruncatedPreamble,

    #[error("truncated package header")]
    TruncatedHeader,

    #[error("duplicate package entry name: {0}")]
    DuplicateEntryName(String),

    #[error("missing component entry `{0}`")]
    MissingComponentEntry(String),

    #[error("entry `{name}` is out of bounds")]
    EntryOutOfBounds { name: String },

    #[error("digest mismatch for {subject}: expected {expected}, got {actual}")]
    DigestMismatch {
        subject: String,
        expected: String,
        actual: String,
    },

    #[error("package header length changed during finalization")]
    HeaderLengthChanged,

    #[error("entry `{0}` was not found")]
    EntryNotFound(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageFormat {
    pub version: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageMetadata {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginMetadata {
    pub id: String,
    pub abi_version: String,
    pub entry: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuntimeMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_version: Option<u32>,
    pub capabilities: CapabilitiesSection,
    pub authorities: AuthoritiesSection,
    pub handlers: HandlersSection,
    pub view: ViewSection,
    pub settings: BTreeMap<String, SettingSchema>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PackageEntryKind {
    Wasm,
    Asset,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageEntry {
    pub name: String,
    pub kind: PackageEntryKind,
    pub offset: u64,
    pub length: u64,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageDigests {
    pub code: String,
    pub artifact: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PackageHeader {
    pub format: PackageFormat,
    pub package: PackageMetadata,
    pub plugin: PluginMetadata,
    pub runtime: RuntimeMetadata,
    pub entries: Vec<PackageEntry>,
    pub digests: PackageDigests,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetInput {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct BuildInput {
    pub package_name: String,
    pub package_version: String,
    pub component_entry: String,
    pub component: Vec<u8>,
    pub manifest: PluginManifest,
    pub assets: Vec<AssetInput>,
}

#[derive(Clone, Debug)]
pub struct BuildOutput {
    pub header: PackageHeader,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct InspectedPackage {
    pub header: PackageHeader,
    pub payload_offset: usize,
    pub package_len: usize,
}

impl RuntimeMetadata {
    fn from_manifest(manifest: &PluginManifest) -> Self {
        let mut capabilities = manifest.capabilities.clone();
        capabilities.wasi.sort();

        let mut authorities = manifest.authorities.clone();
        authorities.host.sort();

        let mut handlers = manifest.handlers.clone();
        handlers.flags.sort();
        handlers.transform_targets.sort();
        handlers.publish_topics.sort();
        handlers.subscribe_topics.sort();
        handlers.extensions_defined.sort();
        handlers.extensions_consumed.sort();

        let mut view = manifest.view.clone();
        view.deps.sort();

        let mut settings = BTreeMap::new();
        for (key, value) in &manifest.settings {
            settings.insert(key.clone(), value.clone());
        }

        Self {
            manifest_version: manifest.manifest_version,
            capabilities,
            authorities,
            handlers,
            view,
            settings,
        }
    }
}

impl PackageHeader {
    pub fn to_manifest(&self) -> PluginManifest {
        PluginManifest {
            manifest_version: self.runtime.manifest_version,
            plugin: PluginSection {
                id: self.plugin.id.clone(),
                abi_version: self.plugin.abi_version.clone(),
            },
            capabilities: self.runtime.capabilities.clone(),
            authorities: self.runtime.authorities.clone(),
            handlers: self.runtime.handlers.clone(),
            view: self.runtime.view.clone(),
            settings: self
                .runtime
                .settings
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }
}

pub fn build_package(input: BuildInput) -> Result<BuildOutput, PackageError> {
    input.manifest.validate()?;
    build_package_inner(input)
}

/// Build a package without manifest validation. Used only in tests that need
/// ABI-incompatible packages for testing the resolver's ABI filter.
#[doc(hidden)]
pub fn build_package_unchecked(input: BuildInput) -> Result<BuildOutput, PackageError> {
    build_package_inner(input)
}

fn build_package_inner(input: BuildInput) -> Result<BuildOutput, PackageError> {
    let mut assets = input.assets;
    assets.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    let component_digest = sha256_prefixed(&input.component);
    let mut entries = Vec::new();
    let mut payload = Vec::new();
    let mut seen_names = std::collections::BTreeSet::new();

    let mut append_entry =
        |name: String, kind: PackageEntryKind, bytes: &[u8]| -> Result<(), PackageError> {
            if !seen_names.insert(name.clone()) {
                return Err(PackageError::DuplicateEntryName(name));
            }
            let offset = payload.len() as u64;
            payload.extend_from_slice(bytes);
            entries.push(PackageEntry {
                name,
                kind,
                offset,
                length: bytes.len() as u64,
                digest: sha256_prefixed(bytes),
            });
            Ok(())
        };

    append_entry(
        input.component_entry.clone(),
        PackageEntryKind::Wasm,
        &input.component,
    )?;
    for asset in &assets {
        append_entry(asset.name.clone(), PackageEntryKind::Asset, &asset.bytes)?;
    }

    let mut header = PackageHeader {
        format: PackageFormat {
            version: FORMAT_VERSION,
        },
        package: PackageMetadata {
            name: input.package_name,
            version: input.package_version,
        },
        plugin: PluginMetadata {
            id: input.manifest.plugin.id.clone(),
            abi_version: input.manifest.plugin.abi_version.clone(),
            entry: input.component_entry.clone(),
            kind: "wasm-component".to_string(),
        },
        runtime: RuntimeMetadata::from_manifest(&input.manifest),
        entries,
        digests: PackageDigests {
            code: component_digest,
            artifact: DIGEST_PLACEHOLDER.to_string(),
        },
    };

    let header_bytes = serde_json::to_vec(&header)?;
    let preamble = encode_preamble(header_bytes.len() as u32);
    let artifact_digest = compute_artifact_digest(&preamble, &header_bytes, &payload);

    header.digests.artifact = artifact_digest;
    let final_header_bytes = serde_json::to_vec(&header)?;
    if final_header_bytes.len() != header_bytes.len() {
        return Err(PackageError::HeaderLengthChanged);
    }

    let mut bytes = Vec::with_capacity(PREAMBLE_LEN + final_header_bytes.len() + payload.len());
    bytes.extend_from_slice(&preamble);
    bytes.extend_from_slice(&final_header_bytes);
    bytes.extend_from_slice(&payload);

    Ok(BuildOutput { header, bytes })
}

pub fn write_package(path: impl AsRef<Path>, output: &BuildOutput) -> Result<(), PackageError> {
    std::fs::write(path, &output.bytes)?;
    Ok(())
}

pub fn inspect_package(bytes: &[u8]) -> Result<InspectedPackage, PackageError> {
    if bytes.len() < PREAMBLE_LEN {
        return Err(PackageError::TruncatedPreamble);
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(PackageError::InvalidMagic);
    }

    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != FORMAT_VERSION {
        return Err(PackageError::UnsupportedFormatVersion(version));
    }

    let header_len = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
    let header_start = PREAMBLE_LEN;
    let header_end = header_start + header_len;
    if bytes.len() < header_end {
        return Err(PackageError::TruncatedHeader);
    }

    let header: PackageHeader = serde_json::from_slice(&bytes[header_start..header_end])?;
    let payload_len = bytes.len() - header_end;
    let component = header
        .entries
        .iter()
        .find(|entry| entry.name == header.plugin.entry)
        .ok_or_else(|| PackageError::MissingComponentEntry(header.plugin.entry.clone()))?;
    if component.digest != header.digests.code {
        return Err(PackageError::DigestMismatch {
            subject: format!("component entry `{}`", header.plugin.entry),
            expected: header.digests.code.clone(),
            actual: component.digest.clone(),
        });
    }

    let mut seen = std::collections::BTreeSet::new();
    for entry in &header.entries {
        if !seen.insert(entry.name.clone()) {
            return Err(PackageError::DuplicateEntryName(entry.name.clone()));
        }
        let end = entry.offset as usize + entry.length as usize;
        if end > payload_len {
            return Err(PackageError::EntryOutOfBounds {
                name: entry.name.clone(),
            });
        }
    }

    Ok(InspectedPackage {
        header,
        payload_offset: header_end,
        package_len: bytes.len(),
    })
}

pub fn inspect_package_file(path: impl AsRef<Path>) -> Result<InspectedPackage, PackageError> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    inspect_package(&bytes)
}

pub fn verify_package(bytes: &[u8]) -> Result<InspectedPackage, PackageError> {
    let inspected = inspect_package(bytes)?;
    let payload = &bytes[inspected.payload_offset..];

    for entry in &inspected.header.entries {
        let start = entry.offset as usize;
        let end = start + entry.length as usize;
        let actual = sha256_prefixed(&payload[start..end]);
        if actual != entry.digest {
            return Err(PackageError::DigestMismatch {
                subject: format!("entry `{}`", entry.name),
                expected: entry.digest.clone(),
                actual,
            });
        }
    }

    let mut header = inspected.header.clone();
    let expected = header.digests.artifact.clone();
    header.digests.artifact = DIGEST_PLACEHOLDER.to_string();
    let header_bytes = serde_json::to_vec(&header)?;
    let preamble = encode_preamble(header_bytes.len() as u32);
    let actual = compute_artifact_digest(&preamble, &header_bytes, payload);
    if actual != expected {
        return Err(PackageError::DigestMismatch {
            subject: "package".to_string(),
            expected,
            actual,
        });
    }

    Ok(inspected)
}

pub fn verify_package_file(path: impl AsRef<Path>) -> Result<InspectedPackage, PackageError> {
    let bytes = std::fs::read(path)?;
    verify_package(&bytes)
}

pub fn entry_bytes<'a>(
    bytes: &'a [u8],
    inspected: &InspectedPackage,
    name: &str,
) -> Result<&'a [u8], PackageError> {
    let entry = inspected
        .header
        .entries
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| PackageError::EntryNotFound(name.to_string()))?;
    let payload = &bytes[inspected.payload_offset..];
    let start = entry.offset as usize;
    let end = start + entry.length as usize;
    Ok(&payload[start..end])
}

fn encode_preamble(header_len: u32) -> [u8; PREAMBLE_LEN] {
    let mut buf = [0u8; PREAMBLE_LEN];
    buf[..8].copy_from_slice(MAGIC);
    buf[8..10].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    buf[10..14].copy_from_slice(&header_len.to_le_bytes());
    buf
}

fn compute_artifact_digest(preamble: &[u8], header: &[u8], payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(preamble);
    hasher.update(header);
    hasher.update(payload);
    let digest = hasher.finalize();
    format!("sha256:{}", to_hex(&digest))
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("sha256:{}", to_hex(&digest))
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;

    fn manifest() -> PluginManifest {
        PluginManifest::parse(
            r#"
[plugin]
id = "demo_plugin"
abi_version = "2.0.0"

[capabilities]
wasi = ["filesystem", "process"]

[handlers]
flags = ["overlay", "input-handler"]

[view]
deps = ["buffer-content", "buffer-cursor"]

[settings.enabled]
type = "bool"
default = true
"#,
        )
        .unwrap()
    }

    fn build_output() -> BuildOutput {
        build_package(BuildInput {
            package_name: "example/demo-plugin".to_string(),
            package_version: "0.1.0".to_string(),
            component_entry: "plugin.wasm".to_string(),
            component: b"\0asmfake-component".to_vec(),
            manifest: manifest(),
            assets: vec![
                AssetInput {
                    name: "assets/icon.txt".to_string(),
                    bytes: b"icon".to_vec(),
                },
                AssetInput {
                    name: "assets/help.txt".to_string(),
                    bytes: b"help".to_vec(),
                },
            ],
        })
        .unwrap()
    }

    #[test]
    fn build_and_inspect_round_trip() {
        let built = build_output();
        let inspected = inspect_package(&built.bytes).unwrap();

        assert_eq!(inspected.header.package.name, "example/demo-plugin");
        assert_eq!(inspected.header.package.version, "0.1.0");
        assert_eq!(inspected.header.plugin.id, "demo_plugin");
        assert_eq!(inspected.header.entries.len(), 3);
        assert_eq!(
            inspected.header.digests.code,
            inspected.header.entries[0].digest
        );
    }

    #[test]
    fn build_is_deterministic() {
        let first = build_output();
        let second = build_output();

        assert_eq!(first.bytes, second.bytes);
        assert_eq!(
            first.header.digests.artifact,
            second.header.digests.artifact
        );
    }

    #[test]
    fn verify_detects_payload_corruption() {
        let mut built = build_output();
        let last = built.bytes.len() - 1;
        built.bytes[last] ^= 0xff;

        let err = verify_package(&built.bytes).unwrap_err();
        assert!(matches!(err, PackageError::DigestMismatch { .. }));
    }

    #[test]
    fn entry_bytes_returns_requested_payload() {
        let built = build_output();
        let inspected = verify_package(&built.bytes).unwrap();
        let bytes = entry_bytes(&built.bytes, &inspected, "assets/icon.txt").unwrap();
        assert_eq!(bytes, b"icon");
    }
}
