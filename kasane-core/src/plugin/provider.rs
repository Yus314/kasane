use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use super::diagnostics::PluginDiagnostic;
use super::setting::SettingValue;
use super::{PluginBackend, PluginId};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PluginSource {
    BundledWasm { name: String },
    FilesystemWasm { path: PathBuf },
    Host { provider: String },
    Builtin { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginRevision(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginRank {
    pub layer: u16,
    pub priority: i16,
}

impl PluginRank {
    pub const BUILTIN: Self = Self {
        layer: 0,
        priority: 0,
    };
    pub const BUNDLED_WASM: Self = Self {
        layer: 100,
        priority: 0,
    };
    pub const FILESYSTEM_WASM: Self = Self {
        layer: 200,
        priority: 0,
    };
    pub const HOST: Self = Self {
        layer: 300,
        priority: 0,
    };
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginDescriptor {
    pub id: PluginId,
    pub source: PluginSource,
    pub revision: PluginRevision,
    pub rank: PluginRank,
}

pub trait PluginFactory: Send + Sync {
    fn descriptor(&self) -> &PluginDescriptor;
    fn create(&self) -> Result<Box<dyn PluginBackend>>;
}

#[derive(Default)]
pub struct PluginCollect {
    pub factories: Vec<Arc<dyn PluginFactory>>,
    pub diagnostics: Vec<PluginDiagnostic>,
    /// Per-plugin initial settings resolved from manifest defaults + config overrides.
    pub initial_settings: HashMap<PluginId, HashMap<String, SettingValue>>,
}

impl PluginCollect {
    pub fn extend(&mut self, other: PluginCollect) {
        self.factories.extend(other.factories);
        self.diagnostics.extend(other.diagnostics);
        self.initial_settings.extend(other.initial_settings);
    }
}

pub trait PluginProvider: Send + Sync {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn collect(&self) -> Result<PluginCollect>;
}

struct ClosurePluginFactory<F> {
    descriptor: PluginDescriptor,
    create: F,
}

impl<F> PluginFactory for ClosurePluginFactory<F>
where
    F: Fn() -> Result<Box<dyn PluginBackend>> + Send + Sync + 'static,
{
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn create(&self) -> Result<Box<dyn PluginBackend>> {
        (self.create)()
    }
}

pub fn plugin_factory(
    descriptor: PluginDescriptor,
    create: impl Fn() -> Result<Box<dyn PluginBackend>> + Send + Sync + 'static,
) -> Arc<dyn PluginFactory> {
    Arc::new(ClosurePluginFactory { descriptor, create })
}

pub fn host_plugin<P, F>(id: impl Into<String>, create: F) -> Arc<dyn PluginFactory>
where
    P: PluginBackend + 'static,
    F: Fn() -> P + Send + Sync + 'static,
{
    host_plugin_with_provider("host", id, create)
}

pub fn host_plugin_with_provider<P, F>(
    provider: impl Into<String>,
    id: impl Into<String>,
    create: F,
) -> Arc<dyn PluginFactory>
where
    P: PluginBackend + 'static,
    F: Fn() -> P + Send + Sync + 'static,
{
    let descriptor = PluginDescriptor {
        id: PluginId(id.into()),
        source: PluginSource::Host {
            provider: provider.into(),
        },
        revision: PluginRevision("static".to_string()),
        rank: PluginRank::HOST,
    };
    plugin_factory(descriptor, move || Ok(Box::new(create())))
}

pub fn builtin_plugin<P, F>(
    name: impl Into<String>,
    id: impl Into<String>,
    create: F,
) -> Arc<dyn PluginFactory>
where
    P: PluginBackend + 'static,
    F: Fn() -> P + Send + Sync + 'static,
{
    let descriptor = PluginDescriptor {
        id: PluginId(id.into()),
        source: PluginSource::Builtin { name: name.into() },
        revision: PluginRevision("static".to_string()),
        rank: PluginRank::BUILTIN,
    };
    plugin_factory(descriptor, move || Ok(Box::new(create())))
}

pub struct StaticPluginProvider {
    factories: Vec<Arc<dyn PluginFactory>>,
}

impl StaticPluginProvider {
    pub fn new(factories: impl IntoIterator<Item = Arc<dyn PluginFactory>>) -> Self {
        Self {
            factories: factories.into_iter().collect(),
        }
    }
}

impl PluginProvider for StaticPluginProvider {
    fn collect(&self) -> Result<PluginCollect> {
        Ok(PluginCollect {
            factories: self.factories.clone(),
            diagnostics: vec![],
            initial_settings: HashMap::new(),
        })
    }
}

pub struct CompositePluginProvider {
    providers: Vec<Box<dyn PluginProvider>>,
}

impl CompositePluginProvider {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn push<P>(&mut self, provider: P)
    where
        P: PluginProvider + 'static,
    {
        self.providers.push(Box::new(provider));
    }
}

impl Default for CompositePluginProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginProvider for CompositePluginProvider {
    fn collect(&self) -> Result<PluginCollect> {
        let mut collect = PluginCollect::default();
        for provider in &self.providers {
            match provider.collect() {
                Ok(provider_collect) => collect.extend(provider_collect),
                Err(err) => collect
                    .diagnostics
                    .push(PluginDiagnostic::provider_collect_failed(
                        provider.name(),
                        err.to_string(),
                    )),
            }
        }
        Ok(collect)
    }
}
