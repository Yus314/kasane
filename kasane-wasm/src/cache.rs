use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use wasmtime::Engine;
use wasmtime::component::Component;

/// Caches precompiled WASM components on disk to avoid repeated cranelift compilation.
///
/// Cache key: `{engine_compat_hex}/{sha256_of_wasm_bytes}.cwasm`
///
/// All failures fall back to full compilation — the cache is an optimization, not a requirement.
pub(crate) struct ComponentCache {
    /// Engine-specific cache directory: `{base}/{engine_compat_hex}/`
    cache_dir: PathBuf,
}

impl ComponentCache {
    /// Create a new cache backed by `$XDG_CACHE_HOME/kasane/wasm/{engine_hex}/`.
    ///
    /// Returns `None` if the cache directory cannot be resolved or created.
    pub(crate) fn new(engine: &Engine) -> Option<Self> {
        let base = cache_base_dir()?;
        Self::new_with_base(engine, &base)
    }

    /// Create a cache under an explicit base directory.
    pub(crate) fn new_with_base(engine: &Engine, base: &std::path::Path) -> Option<Self> {
        let engine_hex = engine_compat_hex(engine);
        let cache_dir = base.join(&engine_hex);
        if let Err(err) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!(
                "failed to create WASM cache directory {}: {err}",
                cache_dir.display()
            );
            return None;
        }
        Some(Self { cache_dir })
    }

    /// Look up a precompiled component for `wasm_bytes`.
    pub(crate) fn get(&self, wasm_bytes: &[u8], engine: &Engine) -> Option<Component> {
        let hash = content_hash(wasm_bytes);
        let path = self.cache_dir.join(format!("{hash}.cwasm"));
        if !path.exists() {
            return None;
        }
        // SAFETY: The .cwasm file was produced by our own `Component::serialize()` and
        // written atomically. The file name is derived from the SHA-256 of the original
        // WASM bytes, so a tampered file would not match the expected name. The cache
        // directory lives under `$XDG_CACHE_HOME` with user-owned permissions.
        // On deserialization failure we fall back to full compilation.
        match unsafe { Component::deserialize_file(engine, &path) } {
            Ok(component) => {
                tracing::trace!("WASM component cache hit: {hash}");
                Some(component)
            }
            Err(err) => {
                tracing::warn!("WASM component cache deserialize failed for {hash}: {err}");
                // Remove corrupted file
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }

    /// Store a precompiled component for `wasm_bytes`.
    pub(crate) fn put(&self, wasm_bytes: &[u8], component: &Component) {
        let hash = content_hash(wasm_bytes);
        let final_path = self.cache_dir.join(format!("{hash}.cwasm"));
        if final_path.exists() {
            return; // Already cached
        }
        let serialized = match component.serialize() {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!("WASM component serialize failed: {err}");
                return;
            }
        };
        let tmp_path = self
            .cache_dir
            .join(format!("{hash}.cwasm.tmp.{}", std::process::id()));
        if let Err(err) = std::fs::write(&tmp_path, &serialized) {
            tracing::warn!("WASM component cache write failed: {err}");
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }
        if let Err(err) = std::fs::rename(&tmp_path, &final_path) {
            tracing::warn!("WASM component cache rename failed: {err}");
            let _ = std::fs::remove_file(&tmp_path);
        }
    }
}

fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn engine_compat_hex(engine: &Engine) -> String {
    let compat = engine.precompile_compatibility_hash();
    let mut hasher = DefaultHasher::new();
    compat.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn cache_base_dir() -> Option<PathBuf> {
    // Respect $XDG_CACHE_HOME; fall back to ~/.cache
    let base = if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg)
    } else {
        dirs_home()?.join(".cache")
    };
    Some(base.join("kasane").join("wasm"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> Engine {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        Engine::new(&config).unwrap()
    }

    fn test_wasm_bytes() -> Vec<u8> {
        crate::load_wasm_fixture("cursor-line.wasm").unwrap()
    }

    fn test_cache(engine: &Engine, tmp: &std::path::Path) -> ComponentCache {
        ComponentCache::new_with_base(engine, tmp).expect("cache creation failed")
    }

    #[test]
    fn content_hash_deterministic() {
        let data = b"hello world";
        assert_eq!(content_hash(data), content_hash(data));
        assert_ne!(content_hash(b"hello"), content_hash(b"world"));
    }

    #[test]
    fn cache_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = test_engine();
        let wasm_bytes = test_wasm_bytes();
        let cache = test_cache(&engine, tmp.path());

        // Miss
        assert!(cache.get(&wasm_bytes, &engine).is_none());

        // Compile and store
        let component = Component::new(&engine, &wasm_bytes).unwrap();
        cache.put(&wasm_bytes, &component);

        // Hit
        let cached = cache.get(&wasm_bytes, &engine);
        assert!(cached.is_some(), "expected cache hit after put");
    }

    #[test]
    fn cache_miss_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = test_engine();
        let cache = test_cache(&engine, tmp.path());

        assert!(cache.get(b"not-a-real-wasm-module", &engine).is_none());
    }

    #[test]
    fn corrupted_file_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = test_engine();
        let cache = test_cache(&engine, tmp.path());

        let wasm_bytes = b"some wasm content";
        let hash = content_hash(wasm_bytes);
        let corrupt_path = cache.cache_dir.join(format!("{hash}.cwasm"));
        std::fs::write(&corrupt_path, b"not a valid cwasm").unwrap();

        // Should return None and not panic
        assert!(cache.get(wasm_bytes, &engine).is_none());
        // Corrupted file should be removed
        assert!(!corrupt_path.exists());
    }

    #[test]
    fn existing_file_skips_write() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = test_engine();
        let wasm_bytes = test_wasm_bytes();
        let cache = test_cache(&engine, tmp.path());

        let component = Component::new(&engine, &wasm_bytes).unwrap();
        cache.put(&wasm_bytes, &component);

        let hash = content_hash(&wasm_bytes);
        let path = cache.cache_dir.join(format!("{hash}.cwasm"));
        let mtime1 = std::fs::metadata(&path).unwrap().modified().unwrap();

        // Second put should be a no-op
        cache.put(&wasm_bytes, &component);
        let mtime2 = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(mtime1, mtime2);
    }
}
