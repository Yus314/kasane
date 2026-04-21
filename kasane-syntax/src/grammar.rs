//! Grammar and query file loading for tree-sitter languages.

use std::collections::HashMap;
use std::path::PathBuf;

/// A loaded language entry with its associated queries.
pub struct LanguageEntry {
    pub language: tree_sitter::Language,
    /// Source text of the fold query (for creating per-provider copies).
    pub fold_query_source: Option<String>,
    /// Source text of the declaration query (for creating per-provider copies).
    pub declaration_query_source: Option<String>,
}

impl LanguageEntry {
    /// Create a fold query for this language from the stored source.
    pub fn make_fold_query(&self) -> Option<tree_sitter::Query> {
        let source = self.fold_query_source.as_ref()?;
        tree_sitter::Query::new(&self.language, source).ok()
    }

    /// Create a declaration query for this language from the stored source.
    pub fn make_declaration_query(&self) -> Option<tree_sitter::Query> {
        let source = self.declaration_query_source.as_ref()?;
        tree_sitter::Query::new(&self.language, source).ok()
    }
}

// =============================================================================
// Bundled declaration queries (fallback when no file on disk)
// =============================================================================

/// Return the bundled declaration query source for a language, if available.
fn bundled_declaration_query(lang_name: &str) -> Option<&'static str> {
    match lang_name {
        "rust" => Some(include_str!("../../data/queries/rust/declarations.scm")),
        "python" => Some(include_str!("../../data/queries/python/declarations.scm")),
        "go" => Some(include_str!("../../data/queries/go/declarations.scm")),
        "typescript" => Some(include_str!(
            "../../data/queries/typescript/declarations.scm"
        )),
        _ => None,
    }
}

/// Registry of tree-sitter grammars discovered from the filesystem.
///
/// Search paths (in priority order):
/// 1. `$XDG_DATA_HOME/kasane/grammars/`
/// 2. `$XDG_DATA_HOME/kak-tree-sitter/grammars/` (shared ecosystem)
///
/// Query files:
/// 1. `$XDG_DATA_HOME/kasane/queries/{lang}/` (folds.scm, declarations.scm)
/// 2. `$XDG_DATA_HOME/kak-tree-sitter/queries/{lang}/` (folds.scm shared)
/// 3. Bundled queries compiled into the binary (declarations.scm only)
pub struct GrammarRegistry {
    languages: HashMap<String, LanguageEntry>,
    search_paths: Vec<PathBuf>,
    query_paths: Vec<PathBuf>,
}

impl GrammarRegistry {
    /// Create a new registry using default XDG search paths.
    pub fn new() -> Self {
        let data_home = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".local").join("share")
            });

        let search_paths = vec![
            data_home.join("kasane").join("grammars"),
            data_home.join("kak-tree-sitter").join("grammars"),
        ];

        let query_paths = vec![
            data_home.join("kasane").join("queries"),
            data_home.join("kak-tree-sitter").join("queries"),
        ];

        Self {
            languages: HashMap::new(),
            search_paths,
            query_paths,
        }
    }

    /// Try to load a language grammar by name (e.g., "rust", "python").
    ///
    /// Returns the language entry if found and loaded successfully.
    pub fn get_or_load(&mut self, lang_name: &str) -> Option<&LanguageEntry> {
        if self.languages.contains_key(lang_name) {
            return self.languages.get(lang_name);
        }

        // Try to load the grammar .so
        let language = self.load_grammar_so(lang_name)?;

        // Load query source files (filesystem first, bundled fallback for declarations)
        let fold_query_source = self.load_query_source(lang_name, "folds.scm");
        let declaration_query_source = self
            .load_query_source(lang_name, "declarations.scm")
            .or_else(|| bundled_declaration_query(lang_name).map(String::from));

        // Validate queries parse correctly
        if let Some(ref src) = fold_query_source
            && let Err(e) = tree_sitter::Query::new(&language, src)
        {
            tracing::warn!("fold query for {lang_name} failed to parse: {e}");
        }
        if let Some(ref src) = declaration_query_source
            && let Err(e) = tree_sitter::Query::new(&language, src)
        {
            tracing::warn!("declaration query for {lang_name} failed to parse: {e}");
        }

        self.languages.insert(
            lang_name.to_string(),
            LanguageEntry {
                language,
                fold_query_source,
                declaration_query_source,
            },
        );

        self.languages.get(lang_name)
    }

    /// Check if a language is already loaded.
    pub fn is_loaded(&self, lang_name: &str) -> bool {
        self.languages.contains_key(lang_name)
    }

    /// Load a grammar shared library.
    fn load_grammar_so(&self, lang_name: &str) -> Option<tree_sitter::Language> {
        // Naming conventions: libtree-sitter-{lang}.so or tree-sitter-{lang}.so
        let lib_names = [
            format!("libtree-sitter-{lang_name}.so"),
            format!("tree-sitter-{lang_name}.so"),
            format!("{lang_name}.so"),
        ];

        for search_path in &self.search_paths {
            for lib_name in &lib_names {
                let path = search_path.join(lib_name);
                if path.exists() {
                    match load_language_from_so(&path, lang_name) {
                        Ok(lang) => {
                            tracing::info!(
                                "loaded grammar for {lang_name} from {}",
                                path.display()
                            );
                            return Some(lang);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "failed to load grammar {} from {}: {e}",
                                lang_name,
                                path.display()
                            );
                        }
                    }
                }
            }
        }

        tracing::debug!("no grammar found for {lang_name}");
        None
    }

    /// Load a query source file (.scm) for a language. Returns the source text.
    fn load_query_source(&self, lang_name: &str, filename: &str) -> Option<String> {
        for query_path in &self.query_paths {
            let path = query_path.join(lang_name).join(filename);
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(source) => {
                        tracing::debug!("loaded query {filename} for {lang_name}");
                        return Some(source);
                    }
                    Err(e) => {
                        tracing::warn!("failed to read {}: {e}", path.display());
                    }
                }
            }
        }
        None
    }
}

impl Default for GrammarRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Load a tree-sitter language from a shared library.
///
/// Looks for a function named `tree_sitter_{lang_name}` in the .so file.
fn load_language_from_so(
    path: &std::path::Path,
    lang_name: &str,
) -> Result<tree_sitter::Language, String> {
    // The function name in the .so follows tree-sitter convention:
    // tree_sitter_{name} where hyphens become underscores.
    let func_name = format!("tree_sitter_{}", lang_name.replace('-', "_"));

    // SAFETY: We are loading a shared library that follows the tree-sitter ABI.
    // This is the standard mechanism used by tree-sitter CLI and editors.
    unsafe {
        let lib = libloading::Library::new(path)
            .map_err(|e| format!("failed to load {}: {e}", path.display()))?;
        let func: libloading::Symbol<unsafe extern "C" fn() -> *const ()> = lib
            .get(func_name.as_bytes())
            .map_err(|e| format!("symbol {func_name} not found: {e}"))?;
        let raw_fn = *func;
        // Leak the library handle — grammars are loaded for the process lifetime.
        std::mem::forget(lib);
        let language_fn = tree_sitter_language::LanguageFn::from_raw(raw_fn);
        Ok(tree_sitter::Language::new(language_fn))
    }
}

/// Map file extension to tree-sitter language name.
pub fn language_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "jsx" => Some("javascript"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" => Some("cpp"),
        "java" => Some("java"),
        "rb" => Some("ruby"),
        "lua" => Some("lua"),
        "sh" | "bash" => Some("bash"),
        "toml" => Some("toml"),
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "md" | "markdown" => Some("markdown"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "zig" => Some("zig"),
        "nix" => Some("nix"),
        "ml" | "mli" => Some("ocaml"),
        "hs" => Some("haskell"),
        "ex" | "exs" => Some("elixir"),
        "erl" | "hrl" => Some("erlang"),
        "kt" | "kts" => Some("kotlin"),
        "swift" => Some("swift"),
        "cs" => Some("c_sharp"),
        "scala" | "sc" => Some("scala"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_mapping() {
        assert_eq!(language_for_extension("rs"), Some("rust"));
        assert_eq!(language_for_extension("py"), Some("python"));
        assert_eq!(language_for_extension("go"), Some("go"));
        assert_eq!(language_for_extension("ts"), Some("typescript"));
        assert_eq!(language_for_extension("unknown"), None);
    }

    #[test]
    fn registry_new_does_not_panic() {
        let registry = GrammarRegistry::new();
        assert!(!registry.is_loaded("rust"));
    }

    #[test]
    fn bundled_declarations_available() {
        for lang in ["rust", "python", "go", "typescript"] {
            let src = bundled_declaration_query(lang);
            assert!(src.is_some(), "bundled query missing for {lang}");
            assert!(
                src.unwrap().contains("@declaration"),
                "bundled query for {lang} missing @declaration capture"
            );
        }
        assert!(bundled_declaration_query("unknown").is_none());
    }
}
