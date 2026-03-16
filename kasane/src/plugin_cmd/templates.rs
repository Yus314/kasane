use crate::cli::PluginTemplate;

const SDK_VERSION: &str = "0.1.0";
const WIT_BINDGEN_VERSION: &str = "0.41";

/// Convert a kebab-case plugin name to a snake_case plugin ID.
///
/// `my-widget` -> `my_widget`
pub fn plugin_id_from_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Convert a kebab-case plugin name to a PascalCase struct name with `Plugin` suffix.
///
/// `my-widget` -> `MyWidgetPlugin`
pub fn struct_name_from_name(name: &str) -> String {
    let pascal: String = name
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect();
    format!("{pascal}Plugin")
}

/// Generate a Cargo.toml for a new plugin project.
pub fn cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "kasane-wasm-{name}"
version = "0.1.0"
edition = "2024"

[workspace]

[lib]
crate-type = ["cdylib"]

[dependencies]
kasane-plugin-sdk = "{SDK_VERSION}"
wit-bindgen = "{WIT_BINDGEN_VERSION}"

[profile.release]
opt-level = "s"
lto = true
"#
    )
}

/// Generate a lib.rs for a new plugin project.
pub fn lib_rs(name: &str, template: PluginTemplate) -> String {
    let id = plugin_id_from_name(name);
    let struct_name = struct_name_from_name(name);

    match template {
        PluginTemplate::Contribution => contribution_template(&id, &struct_name),
        PluginTemplate::Annotation => annotation_template(&id, &struct_name),
        PluginTemplate::Transform => transform_template(&id, &struct_name),
    }
}

fn contribution_template(id: &str, struct_name: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{{dirty, plugin}};

thread_local! {{
    static CURSOR_COUNT: Cell<u32> = const {{ Cell::new(0) }};
}}

struct {struct_name};

#[plugin]
impl Guest for {struct_name} {{
    fn get_id() -> String {{
        "{id}".to_string()
    }}

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {{
        if dirty_flags & dirty::BUFFER != 0 {{
            CURSOR_COUNT.set(host_state::get_cursor_count());
        }}
        vec![]
    }}

    fn state_hash() -> u64 {{
        CURSOR_COUNT.get() as u64
    }}

    kasane_plugin_sdk::slots! {{
        STATUS_RIGHT(dirty::BUFFER) => |_ctx| {{
            let count = CURSOR_COUNT.get();
            (count > 1).then(|| {{
                auto_contribution(text(&format!(" {{}} sel ", count), default_face()))
            }})
        }},
    }}
}}

export!({struct_name});
"#
    )
}

fn annotation_template(id: &str, struct_name: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{{dirty, plugin}};

thread_local! {{
    static ACTIVE_LINE: Cell<i32> = const {{ Cell::new(-1) }};
}}

struct {struct_name};

#[plugin]
impl Guest for {struct_name} {{
    fn get_id() -> String {{
        "{id}".to_string()
    }}

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {{
        if dirty_flags & dirty::BUFFER != 0 {{
            let line = host_state::get_cursor_line();
            ACTIVE_LINE.set(line);
        }}
        vec![]
    }}

    fn state_hash() -> u64 {{
        ACTIVE_LINE.get() as u64
    }}

    fn annotate_line(line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {{
        let active = ACTIVE_LINE.get();
        (line as i32 == active).then(|| bg_annotation(face_bg(rgb(40, 40, 50))))
    }}

    fn annotate_deps() -> u16 {{
        dirty::BUFFER
    }}
}}

export!({struct_name});
"#
    )
}

fn transform_template(id: &str, struct_name: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{{dirty, plugin}};

/// Cursor mode constants (matches host encoding).
const MODE_BUFFER: u8 = 0;
const MODE_PROMPT: u8 = 1;

thread_local! {{
    static CURSOR_MODE: Cell<u8> = const {{ Cell::new(MODE_BUFFER) }};
}}

struct {struct_name};

#[plugin]
impl Guest for {struct_name} {{
    fn get_id() -> String {{
        "{id}".to_string()
    }}

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {{
        if dirty_flags & dirty::STATUS != 0 {{
            CURSOR_MODE.set(host_state::get_cursor_mode());
        }}
        vec![]
    }}

    fn state_hash() -> u64 {{
        CURSOR_MODE.get() as u64
    }}

    fn transform_element(
        target: TransformTarget,
        element: ElementHandle,
        _ctx: TransformContext,
    ) -> ElementHandle {{
        if !matches!(target, TransformTarget::StatusBarT) {{
            return element;
        }}

        if CURSOR_MODE.get() != MODE_PROMPT {{
            return element;
        }}

        // Wrap the status bar in a container with a distinct background
        container(element)
            .style(face(named(NamedColor::Black), named(NamedColor::Yellow)))
            .build()
    }}

    fn transform_priority() -> i16 {{
        0
    }}

    fn transform_deps(target: TransformTarget) -> u16 {{
        match target {{
            TransformTarget::StatusBarT => dirty::STATUS,
            _ => 0,
        }}
    }}
}}

export!({struct_name});
"#
    )
}

/// Generate a .gitignore for a new plugin project.
pub fn gitignore() -> &'static str {
    "/target\n/Cargo.lock\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_id_from_name() {
        assert_eq!(plugin_id_from_name("my-widget"), "my_widget");
        assert_eq!(plugin_id_from_name("simple"), "simple");
        assert_eq!(plugin_id_from_name("a-b-c"), "a_b_c");
    }

    #[test]
    fn test_struct_name_from_name() {
        assert_eq!(struct_name_from_name("my-widget"), "MyWidgetPlugin");
        assert_eq!(struct_name_from_name("simple"), "SimplePlugin");
        assert_eq!(struct_name_from_name("a-b-c"), "ABCPlugin");
    }

    #[test]
    fn test_cargo_toml_contents() {
        let toml = cargo_toml("my-widget");
        assert!(toml.contains("cdylib"));
        assert!(toml.contains(&format!("kasane-plugin-sdk = \"{SDK_VERSION}\"")));
        assert!(toml.contains(&format!("wit-bindgen = \"{WIT_BINDGEN_VERSION}\"")));
        assert!(toml.contains("kasane-wasm-my-widget"));
    }

    #[test]
    fn test_lib_rs_contribution() {
        let src = lib_rs("test-plug", PluginTemplate::Contribution);
        assert!(src.contains("generate!()"));
        assert!(src.contains("#[plugin]"));
        assert!(src.contains("export!(TestPlugPlugin)"));
        assert!(src.contains("\"test_plug\""));
        assert!(src.contains("slots!"));
    }

    #[test]
    fn test_lib_rs_annotation() {
        let src = lib_rs("test-plug", PluginTemplate::Annotation);
        assert!(src.contains("generate!()"));
        assert!(src.contains("#[plugin]"));
        assert!(src.contains("export!(TestPlugPlugin)"));
        assert!(src.contains("annotate_line"));
    }

    #[test]
    fn test_lib_rs_transform() {
        let src = lib_rs("test-plug", PluginTemplate::Transform);
        assert!(src.contains("generate!()"));
        assert!(src.contains("#[plugin]"));
        assert!(src.contains("export!(TestPlugPlugin)"));
        assert!(src.contains("transform_element"));
    }
}
