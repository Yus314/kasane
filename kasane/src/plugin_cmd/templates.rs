use crate::cli::PluginTemplate;

const SDK_VERSION: &str = "0.3.0";
const WIT_BINDGEN_VERSION: &str = "0.53";
const HOST_ABI_VERSION: &str = "3.0.0";

/// Convert a kebab-case plugin name to a snake_case plugin ID.
///
/// `my-widget` -> `my_widget`
pub fn plugin_id_from_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Convert a kebab-case plugin name to a PascalCase struct name with `Plugin` suffix.
///
/// `my-widget` -> `MyWidgetPlugin`
#[cfg(test)]
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
name = "{name}"
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

/// Generate a kasane-plugin.toml for a new plugin project.
pub fn plugin_manifest_toml(name: &str, template: PluginTemplate) -> String {
    let id = plugin_id_from_name(name);
    let (handlers, view_deps, wasi_capabilities) = match template {
        PluginTemplate::Hello => (&["contributor"][..], &[][..], &[][..]),
        PluginTemplate::Contribution => (
            &["contributor"][..],
            &["buffer-content", "buffer-cursor"][..],
            &[][..],
        ),
        PluginTemplate::Annotation => (
            &["display-transform"][..],
            &["buffer-content", "buffer-cursor"][..],
            &[][..],
        ),
        PluginTemplate::Transform => (&["transformer"][..], &["status"][..], &[][..]),
        PluginTemplate::Overlay => (&["overlay", "input-handler"][..], &[][..], &[][..]),
        PluginTemplate::Process => (
            &["overlay", "input-handler", "io-handler"][..],
            &[][..],
            &["process"][..],
        ),
    };

    let mut out = format!(
        r#"[plugin]
id = "{id}"
abi_version = "{HOST_ABI_VERSION}"
"#
    );

    if !wasi_capabilities.is_empty() {
        let values = wasi_capabilities
            .iter()
            .map(|value| format!(r#""{value}""#))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("\n[capabilities]\nwasi = [{values}]\n"));
    }

    if !handlers.is_empty() {
        let values = handlers
            .iter()
            .map(|value| format!(r#""{value}""#))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("\n[handlers]\nflags = [{values}]\n"));
    }

    if !view_deps.is_empty() {
        let values = view_deps
            .iter()
            .map(|value| format!(r#""{value}""#))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("\n[view]\ndeps = [{values}]\n"));
    }

    out
}

/// Generate a lib.rs for a new plugin project.
pub fn lib_rs(name: &str, template: PluginTemplate) -> String {
    let id = plugin_id_from_name(name);

    match template {
        PluginTemplate::Hello => hello_template(&id),
        PluginTemplate::Contribution => contribution_template(&id),
        PluginTemplate::Annotation => annotation_template(&id),
        PluginTemplate::Transform => transform_template(&id),
        PluginTemplate::Overlay => overlay_template(&id),
        PluginTemplate::Process => process_template(&id),
    }
}

fn hello_template(id: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::define_plugin! {{
    manifest: "kasane-plugin.toml",
    slots {{
        STATUS_RIGHT => plain(" Hello from {id}! "),
    }},
}}
"#
    )
}

#[allow(clippy::useless_format)]
fn contribution_template(_id: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::define_plugin! {{
    manifest: "kasane-plugin.toml",

    state {{
        #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
        cursor_count: u32 = 0,
    }},

    slots {{
        STATUS_RIGHT(dirty::BUFFER) => |_ctx| {{
            (state.cursor_count > 1).then(|| {{
                auto_contribution(text(&format!(" {{}} sel ", state.cursor_count), default_style()))
            }})
        }},
    }},
}}
"#
    )
}

#[allow(clippy::useless_format)]
fn annotation_template(_id: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::define_plugin! {{
    manifest: "kasane-plugin.toml",

    state {{
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        active_line: i32 = -1,
    }},

    display() {{
        if state.active_line < 0 {{
            return vec![];
        }}
        vec![style_line(state.active_line as u32, style_bg(rgb(40, 40, 50)))]
    }},
}}
"#
    )
}

#[allow(clippy::useless_format)]
fn transform_template(_id: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::define_plugin! {{
    manifest: "kasane-plugin.toml",

    state {{
        #[bind(host_state::get_cursor_mode(), on: dirty::STATUS)]
        cursor_mode: u8 = 0,
    }},

    transform(target, subject, _ctx) {{
        if target != "kasane.status-bar" {{
            return subject;
        }}
        if state.cursor_mode != 1 {{
            return subject;
        }}
        // Wrap the status bar in a container with a distinct background
        match subject {{
            TransformSubject::ElementS(element) => {{
                TransformSubject::ElementS(
                    container(element)
                        .style(style_with(named(NamedColor::Black), named(NamedColor::Yellow)))
                        .build(),
                )
            }}
            other => other,
        }}
    }},

    transform_priority: 0,
}}
"#
    )
}

#[allow(clippy::useless_format)]
fn overlay_template(_id: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::define_plugin! {{
    manifest: "kasane-plugin.toml",

    state {{
        open: bool = false,
        selected: usize = 0,
    }},

    handle_key(event) {{
        if !state.open {{
            if is_ctrl(&event, 'o') {{
                state.open = true;
                state.selected = 0;
                return consumed_redraw();
            }}
            return None;
        }}

        match &event.key {{
            KeyCode::Escape => {{
                state.open = false;
                consumed_redraw()
            }}
            KeyCode::Up => nav_up(&mut state.selected),
            KeyCode::Down => nav_down(&mut state.selected, 3),
            KeyCode::Enter => {{
                state.open = false;
                consumed_redraw()
            }}
            _ => consumed(),
        }}
    }},

    overlay(ctx) {{
        if !state.open {{
            return None;
        }}

        let items = ["Item 1", "Item 2", "Item 3"];
        let highlight = style_with(named(NamedColor::White), rgb(4, 57, 94));
        let anchor = centered_overlay(ctx.screen_cols, ctx.screen_rows, 40, 30, 20, 8);
        let mut children: Vec<ElementHandle> = Vec::new();

        for (i, item) in items.iter().enumerate() {{
            let f = if i == state.selected {{ highlight.clone() }} else {{ default_style() }};
            let prefix = if i == state.selected {{ "> " }} else {{ "  " }};
            let label = format!("{{prefix}}{{item}}");
            children.push(text(&label, f));
        }}

        let inner = column(&children);
        let el = container(inner)
            .border(BorderLineStyle::Rounded)
            .shadow()
            .padding(padding_h(1))
            .title_text(" Select Item ")
            .build();

        Some(OverlayContribution {{
            element: el,
            anchor: OverlayAnchor::Absolute(anchor),
            z_index: 100,
        }})
    }},
}}
"#
    )
}

#[allow(clippy::useless_format)]
fn process_template(_id: &str) -> String {
    format!(
        r#"kasane_plugin_sdk::define_plugin! {{
    manifest: "kasane-plugin.toml",

    state {{
        active: bool = false,
        output: Vec<String> = Vec::new(),
    }},

    handle_key(event) {{
        if !state.active {{
            if is_ctrl_shift(&event, 'P') {{
                state.active = true;
                state.output.clear();
                return Some(vec![
                    Command::SpawnProcess(SpawnProcessConfig {{
                        job_id: 1,
                        program: "echo".to_string(),
                        args: vec!["Hello from process!".to_string()],
                        stdin_mode: StdinMode::NullStdin,
                    }}),
                    Command::RequestRedraw(dirty::ALL),
                ]);
            }}
            return None;
        }}

        match &event.key {{
            KeyCode::Escape => {{
                state.active = false;
                Some(vec![
                    Command::KillProcess(1),
                    Command::RequestRedraw(dirty::ALL),
                ])
            }}
            _ => consumed(),
        }}
    }},

    on_io_event_effects(event) {{
        let IoEvent::Process(pe) = event else {{
            return effects(vec![]);
        }};
        match pe.kind {{
            ProcessEventKind::Stdout(data) => {{
                let text_data = String::from_utf8_lossy(&data);
                for line in text_data.lines() {{
                    if !line.is_empty() {{
                        state.output.push(line.to_string());
                    }}
                }}
                just_redraw()
            }}
            ProcessEventKind::Exited(_) => just_redraw(),
            _ => effects(vec![]),
        }}
    }},

    overlay(ctx) {{
        if !state.active {{
            return None;
        }}

        let dim = style_fg(named(NamedColor::BrightBlack));
        let anchor = centered_overlay(ctx.screen_cols, ctx.screen_rows, 50, 40, 30, 8);
        let mut children: Vec<ElementHandle> = Vec::new();

        if state.output.is_empty() {{
            children.push(text("Running...", dim));
        }} else {{
            for line in &state.output {{
                children.push(text(line, default_style()));
            }}
        }}

        let inner = column(&children);
        let el = container(inner)
            .border(BorderLineStyle::Rounded)
            .shadow()
            .padding(padding_h(1))
            .title_text(" Process Output ")
            .build();

        Some(OverlayContribution {{
            element: el,
            anchor: OverlayAnchor::Absolute(anchor),
            z_index: 100,
        }})
    }},
}}
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
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kasane-template-{name}-{}-{ts}",
            std::process::id()
        ))
    }

    fn local_sdk_dependency_line() -> String {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("kasane crate should live under workspace root")
            .to_path_buf();
        let sdk_dir = repo_root.join("kasane-plugin-sdk");
        format!("kasane-plugin-sdk = {{ path = {:?} }}", sdk_dir)
    }

    fn compile_generated_template(name: &str, template: PluginTemplate) {
        let project_dir = temp_project_dir(name);
        let src_dir = project_dir.join("src");
        fs::create_dir_all(&src_dir).expect("temp project src dir should be created");

        let mut manifest = cargo_toml(name);
        manifest = manifest.replace(
            &format!("kasane-plugin-sdk = \"{SDK_VERSION}\""),
            &local_sdk_dependency_line(),
        );
        fs::write(project_dir.join("Cargo.toml"), manifest).expect("Cargo.toml should be written");
        fs::write(
            project_dir.join("kasane-plugin.toml"),
            plugin_manifest_toml(name, template),
        )
        .expect("kasane-plugin.toml should be written");
        fs::write(src_dir.join("lib.rs"), lib_rs(name, template))
            .expect("lib.rs should be written");

        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .arg("check")
            .arg("--quiet")
            .arg("--target")
            .arg("wasm32-wasip2")
            .arg("--manifest-path")
            .arg(project_dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", project_dir.join("target"))
            .output()
            .expect("generated plugin should be checked");

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("generated template failed to compile\nstdout:\n{stdout}\nstderr:\n{stderr}");
        }

        let _ = fs::remove_dir_all(&project_dir);
    }

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
        assert!(toml.contains("name = \"my-widget\""));
    }

    #[test]
    fn test_plugin_manifest_toml_contents() {
        let manifest = plugin_manifest_toml("my-widget", PluginTemplate::Process);
        assert!(manifest.contains("id = \"my_widget\""));
        assert!(manifest.contains(&format!("abi_version = \"{HOST_ABI_VERSION}\"")));
        assert!(manifest.contains("flags = [\"overlay\", \"input-handler\", \"io-handler\"]"));
        assert!(manifest.contains("wasi = [\"process\"]"));
    }

    #[test]
    fn test_lib_rs_template_markers() {
        let cases: &[(PluginTemplate, &[&str])] = &[
            (
                PluginTemplate::Hello,
                &["manifest:", "STATUS_RIGHT", "plain("],
            ),
            (
                PluginTemplate::Contribution,
                &["manifest:", "#[bind(", "cursor_count", "slots"],
            ),
            (
                PluginTemplate::Annotation,
                &["manifest:", "display", "style_line", "active_line"],
            ),
            (
                PluginTemplate::Transform,
                &[
                    "manifest:",
                    "transform",
                    "transform_priority",
                    "cursor_mode",
                ],
            ),
            (
                PluginTemplate::Overlay,
                &[
                    "manifest:",
                    "overlay",
                    "handle_key",
                    "is_ctrl",
                    "OverlayContribution",
                ],
            ),
            (
                PluginTemplate::Process,
                &[
                    "manifest:",
                    "on_io_event_effects",
                    "just_redraw",
                    "SpawnProcess",
                    "is_ctrl_shift",
                ],
            ),
        ];

        for (template, markers) in cases {
            let src = lib_rs("test-plug", *template);
            for marker in *markers {
                assert!(
                    src.contains(marker),
                    "template {:?} should contain marker `{marker}`",
                    template
                );
            }
        }
    }

    #[test]
    fn test_generated_process_template_compiles() {
        compile_generated_template("compile-process", PluginTemplate::Process);
    }

    #[test]
    fn test_generated_hello_template_compiles() {
        compile_generated_template("compile-hello", PluginTemplate::Hello);
    }

    #[test]
    fn test_generated_contribution_template_compiles() {
        compile_generated_template("compile-contribution", PluginTemplate::Contribution);
    }

    #[test]
    fn test_generated_annotation_template_compiles() {
        compile_generated_template("compile-annotation", PluginTemplate::Annotation);
    }

    #[test]
    fn test_generated_transform_template_compiles() {
        compile_generated_template("compile-transform", PluginTemplate::Transform);
    }

    #[test]
    fn test_generated_overlay_template_compiles() {
        compile_generated_template("compile-overlay", PluginTemplate::Overlay);
    }

    #[test]
    fn test_key_map_actions_template_compiles() {
        let src = r#"kasane_plugin_sdk::define_plugin! {
    id: "key_map_test",

    state {
        open: bool = false,
        query: String = String::new(),
    },

    key_map {
        when(state.open) {
            key(Escape)  => "close",
            key(Up)      => "nav_up",
            any_char()   => "append_char",
        },
        when(!state.open) {
            ctrl('p')    => "activate",
        },
        chord(ctrl('w')) {
            char('v') => "split_v",
            char('s') => "split_h",
        },
    },

    actions {
        "activate" => |_event| {
            state.open = true;
            KeyResponse::ConsumeRedraw
        },
        "close" => |_event| {
            state.open = false;
            state.query.clear();
            KeyResponse::ConsumeRedraw
        },
        "nav_up" => |_event| {
            KeyResponse::Consume
        },
        "append_char" => |event| {
            if let KeyCode::Char(cp) = event.key {
                if let Some(c) = char::from_u32(cp) {
                    state.query.push(c);
                }
            }
            KeyResponse::ConsumeRedraw
        },
        "split_v" => |_event| {
            KeyResponse::Consume
        },
        "split_h" => |_event| {
            KeyResponse::Consume
        },
    },
}
"#;
        let project_dir = temp_project_dir("compile-keymap");
        let src_dir = project_dir.join("src");
        fs::create_dir_all(&src_dir).expect("temp project src dir should be created");

        let mut manifest = cargo_toml("key-map-test");
        manifest = manifest.replace(
            &format!("kasane-plugin-sdk = \"{SDK_VERSION}\""),
            &local_sdk_dependency_line(),
        );
        fs::write(project_dir.join("Cargo.toml"), manifest).expect("Cargo.toml should be written");
        fs::write(src_dir.join("lib.rs"), src).expect("lib.rs should be written");

        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .arg("check")
            .arg("--quiet")
            .arg("--target")
            .arg("wasm32-wasip2")
            .arg("--manifest-path")
            .arg(project_dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", project_dir.join("target"))
            .output()
            .expect("key_map template should be checked");

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("key_map template failed to compile\nstdout:\n{stdout}\nstderr:\n{stderr}");
        }

        let _ = fs::remove_dir_all(&project_dir);
    }
}
