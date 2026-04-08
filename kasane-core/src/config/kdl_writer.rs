//! Config → KDL serialization and format-preserving edit.

use std::collections::HashMap;

use super::unified::CONFIG_SECTIONS;
use super::{
    BorderStyleConfig, ColorsConfig, Config, FontConfig, ImageProtocolConfig, LogConfig,
    MenuConfig, MenuPosition, MouseConfig, PluginSelection, PluginsConfig, ScrollConfig,
    SearchConfig, StatusPosition, ThemeConfig, ThemeValue, UiConfig, WindowConfig,
};

/// Generate KDL nodes for the config sections of a [`Config`] value.
///
/// Only sections whose values differ from defaults are emitted, keeping
/// the output concise.  Widget nodes are **not** included — the caller
/// is responsible for combining config and widget nodes.
pub fn config_to_kdl_nodes(config: &Config) -> Vec<kdl::KdlNode> {
    let mut nodes = Vec::new();

    if config.ui != UiConfig::default() {
        nodes.push(ui_to_kdl(&config.ui));
    }
    if config.scroll != ScrollConfig::default() {
        nodes.push(scroll_to_kdl(&config.scroll));
    }
    if config.log != LogConfig::default() {
        nodes.push(log_to_kdl(&config.log));
    }
    if !config.theme.faces.is_empty() || !config.theme.variants.is_empty() {
        nodes.push(theme_to_kdl(&config.theme));
    }
    if config.menu != MenuConfig::default() {
        nodes.push(menu_to_kdl(&config.menu));
    }
    if config.search != SearchConfig::default() {
        nodes.push(search_to_kdl(&config.search));
    }
    if config.clipboard != super::ClipboardConfig::default() {
        nodes.push(clipboard_to_kdl(&config.clipboard));
    }
    if config.mouse != MouseConfig::default() {
        nodes.push(mouse_to_kdl(&config.mouse));
    }
    if config.window != WindowConfig::default() {
        nodes.push(window_to_kdl(&config.window));
    }
    if config.font != FontConfig::default() {
        nodes.push(font_to_kdl(&config.font));
    }
    if config.colors != ColorsConfig::default() {
        nodes.push(colors_to_kdl(&config.colors));
    }
    if config.plugins != PluginsConfig::default() {
        nodes.push(plugins_to_kdl(&config.plugins));
    }
    if !config.settings.is_empty() {
        nodes.push(settings_to_kdl(&config.settings));
    }

    nodes
}

/// Replace config sections in an existing KDL document, preserving widget nodes.
///
/// Returns the updated document as a string.
pub fn patch_config_in_document(
    existing_source: &str,
    config: &Config,
) -> Result<String, kdl::KdlError> {
    let mut doc: kdl::KdlDocument = existing_source.parse()?;

    // Remove existing config section nodes, preserving `widgets` block
    doc.nodes_mut()
        .retain(|n| !CONFIG_SECTIONS.contains(&n.name().value()) || n.name().value() == "widgets");

    // Generate fresh config nodes
    let config_nodes = config_to_kdl_nodes(config);

    // Prepend config nodes before widget nodes
    let widget_nodes = std::mem::take(doc.nodes_mut());
    let mut new_nodes = config_nodes;
    // Autoformat generated nodes for clean output
    for n in &mut new_nodes {
        n.autoformat();
    }
    new_nodes.extend(widget_nodes);
    *doc.nodes_mut() = new_nodes;

    Ok(doc.to_string())
}

// ── Section serialisers ──────────────────────────────────────────────

fn make_node(name: &str) -> kdl::KdlNode {
    kdl::KdlNode::new(name)
}

fn bool_child(name: &str, value: bool) -> kdl::KdlNode {
    let mut n = make_node(name);
    n.push(kdl::KdlEntry::new(kdl::KdlValue::Bool(value)));
    n
}

fn str_child(name: &str, value: &str) -> kdl::KdlNode {
    let mut n = make_node(name);
    n.push(kdl::KdlEntry::new(value));
    n
}

fn i64_child(name: &str, value: i64) -> kdl::KdlNode {
    let mut n = make_node(name);
    n.push(kdl::KdlEntry::new(kdl::KdlValue::Integer(value as i128)));
    n
}

fn f64_child(name: &str, value: f64) -> kdl::KdlNode {
    let mut n = make_node(name);
    n.push(kdl::KdlEntry::new(kdl::KdlValue::Float(value)));
    n
}

fn string_list_child(name: &str, values: &[String]) -> kdl::KdlNode {
    let mut n = make_node(name);
    for v in values {
        n.push(kdl::KdlEntry::new(v.as_str()));
    }
    n
}

fn section_with_children(name: &str, children: Vec<kdl::KdlNode>) -> kdl::KdlNode {
    let mut node = make_node(name);
    let mut doc = kdl::KdlDocument::new();
    *doc.nodes_mut() = children;
    node.set_children(doc);
    node
}

fn ui_to_kdl(ui: &UiConfig) -> kdl::KdlNode {
    let mut children = vec![
        bool_child("shadow", ui.shadow),
        str_child("padding_char", &ui.padding_char),
        str_child(
            "border_style",
            match ui.border_style {
                BorderStyleConfig::Single => "single",
                BorderStyleConfig::Rounded => "rounded",
                BorderStyleConfig::Double => "double",
                BorderStyleConfig::Heavy => "heavy",
                BorderStyleConfig::Ascii => "ascii",
            },
        ),
        str_child(
            "status_position",
            match ui.status_position {
                StatusPosition::Top => "top",
                StatusPosition::Bottom => "bottom",
            },
        ),
        str_child("backend", &ui.backend),
    ];
    if let Some(sr) = ui.scene_renderer {
        children.push(bool_child("scene_renderer", sr));
    }
    children.push(str_child(
        "image_protocol",
        match ui.image_protocol {
            ImageProtocolConfig::Auto => "auto",
            ImageProtocolConfig::Halfblock => "halfblock",
            ImageProtocolConfig::Kitty => "kitty",
        },
    ));
    section_with_children("ui", children)
}

fn scroll_to_kdl(s: &ScrollConfig) -> kdl::KdlNode {
    section_with_children(
        "scroll",
        vec![
            i64_child("lines_per_scroll", s.lines_per_scroll as i64),
            bool_child("smooth", s.smooth),
            bool_child("inertia", s.inertia),
        ],
    )
}

fn log_to_kdl(l: &LogConfig) -> kdl::KdlNode {
    let mut children = vec![str_child("level", &l.level)];
    if let Some(ref f) = l.file {
        children.push(str_child("file", f));
    }
    section_with_children("log", children)
}

fn theme_value_to_kdl_string(tv: &ThemeValue) -> String {
    match tv {
        ThemeValue::FaceSpec(s) => s.clone(),
        ThemeValue::TokenRef(name) => format!("@{name}"),
    }
}

fn theme_to_kdl(t: &ThemeConfig) -> kdl::KdlNode {
    let mut children: Vec<kdl::KdlNode> = t
        .faces
        .iter()
        .map(|(name, tv)| str_child(name, &theme_value_to_kdl_string(tv)))
        .collect();
    children.sort_by(|a, b| a.name().value().cmp(b.name().value()));

    // Serialize variants
    let mut variant_names: Vec<&String> = t.variants.keys().collect();
    variant_names.sort();
    for vname in variant_names {
        let vfaces = &t.variants[vname];
        let mut vchildren: Vec<kdl::KdlNode> = vfaces
            .iter()
            .map(|(name, tv)| str_child(name, &theme_value_to_kdl_string(tv)))
            .collect();
        vchildren.sort_by(|a, b| a.name().value().cmp(b.name().value()));
        let mut vnode = make_node("variant");
        vnode.push(kdl::KdlEntry::new(vname.as_str()));
        let mut vdoc = kdl::KdlDocument::new();
        *vdoc.nodes_mut() = vchildren;
        vnode.set_children(vdoc);
        children.push(vnode);
    }

    section_with_children("theme", children)
}

fn menu_to_kdl(m: &MenuConfig) -> kdl::KdlNode {
    section_with_children(
        "menu",
        vec![
            str_child(
                "position",
                match m.position {
                    MenuPosition::Auto => "auto",
                    MenuPosition::Above => "above",
                    MenuPosition::Below => "below",
                },
            ),
            i64_child("max_height", m.max_height as i64),
        ],
    )
}

fn search_to_kdl(s: &SearchConfig) -> kdl::KdlNode {
    section_with_children("search", vec![bool_child("dropdown", s.dropdown)])
}

fn clipboard_to_kdl(c: &super::ClipboardConfig) -> kdl::KdlNode {
    section_with_children("clipboard", vec![bool_child("enabled", c.enabled)])
}

fn mouse_to_kdl(m: &MouseConfig) -> kdl::KdlNode {
    section_with_children("mouse", vec![bool_child("drag_scroll", m.drag_scroll)])
}

fn window_to_kdl(w: &WindowConfig) -> kdl::KdlNode {
    let mut children = vec![
        i64_child("initial_cols", w.initial_cols as i64),
        i64_child("initial_rows", w.initial_rows as i64),
        bool_child("fullscreen", w.fullscreen),
        bool_child("maximized", w.maximized),
    ];
    if let Some(ref pm) = w.present_mode {
        children.push(str_child("present_mode", pm));
    }
    section_with_children("window", children)
}

fn font_to_kdl(f: &FontConfig) -> kdl::KdlNode {
    let mut children = vec![
        str_child("family", &f.family),
        f64_child("size", f.size as f64),
        str_child("style", &f.style),
    ];
    if !f.fallback_list.is_empty() {
        children.push(string_list_child("fallback_list", &f.fallback_list));
    }
    children.push(f64_child("line_height", f.line_height as f64));
    children.push(f64_child("letter_spacing", f.letter_spacing as f64));
    section_with_children("font", children)
}

fn colors_to_kdl(c: &ColorsConfig) -> kdl::KdlNode {
    section_with_children(
        "colors",
        vec![
            str_child("default_fg", &c.default_fg),
            str_child("default_bg", &c.default_bg),
            str_child("black", &c.black),
            str_child("red", &c.red),
            str_child("green", &c.green),
            str_child("yellow", &c.yellow),
            str_child("blue", &c.blue),
            str_child("magenta", &c.magenta),
            str_child("cyan", &c.cyan),
            str_child("white", &c.white),
            str_child("bright_black", &c.bright_black),
            str_child("bright_red", &c.bright_red),
            str_child("bright_green", &c.bright_green),
            str_child("bright_yellow", &c.bright_yellow),
            str_child("bright_blue", &c.bright_blue),
            str_child("bright_magenta", &c.bright_magenta),
            str_child("bright_cyan", &c.bright_cyan),
            str_child("bright_white", &c.bright_white),
        ],
    )
}

fn plugins_to_kdl(p: &PluginsConfig) -> kdl::KdlNode {
    let mut children = Vec::new();

    if let Some(ref path) = p.path {
        children.push(str_child("path", path));
    }
    if !p.enabled.is_empty() {
        children.push(string_list_child("enabled", &p.enabled));
    }
    if !p.disabled.is_empty() {
        children.push(string_list_child("disabled", &p.disabled));
    }

    if !p.deny_capabilities.is_empty() {
        let mut cap_children: Vec<kdl::KdlNode> = p
            .deny_capabilities
            .iter()
            .map(|(id, caps)| {
                let caps_owned: Vec<String> = caps.clone();
                string_list_child(id, &caps_owned)
            })
            .collect();
        cap_children.sort_by(|a, b| a.name().value().cmp(b.name().value()));
        children.push(section_with_children("deny_capabilities", cap_children));
    }

    if !p.deny_authorities.is_empty() {
        let mut auth_children: Vec<kdl::KdlNode> = p
            .deny_authorities
            .iter()
            .map(|(id, auths)| {
                let auths_owned: Vec<String> = auths.clone();
                string_list_child(id, &auths_owned)
            })
            .collect();
        auth_children.sort_by(|a, b| a.name().value().cmp(b.name().value()));
        children.push(section_with_children("deny_authorities", auth_children));
    }

    if !p.selection.is_empty() {
        let mut sel_children: Vec<kdl::KdlNode> = p
            .selection
            .iter()
            .map(|(id, sel)| selection_to_kdl(id, sel))
            .collect();
        sel_children.sort_by(|a, b| a.name().value().cmp(b.name().value()));
        children.push(section_with_children("selection", sel_children));
    }

    section_with_children("plugins", children)
}

fn selection_to_kdl(id: &str, sel: &PluginSelection) -> kdl::KdlNode {
    let mut node = make_node(id);
    match sel {
        PluginSelection::Auto => {
            node.push(kdl::KdlEntry::new_prop("mode", "auto"));
        }
        PluginSelection::PinDigest { digest } => {
            node.push(kdl::KdlEntry::new_prop("mode", "pin-digest"));
            node.push(kdl::KdlEntry::new_prop("digest", digest.as_str()));
        }
        PluginSelection::PinPackage { package, version } => {
            node.push(kdl::KdlEntry::new_prop("mode", "pin-package"));
            node.push(kdl::KdlEntry::new_prop("package", package.as_str()));
            if let Some(v) = version {
                node.push(kdl::KdlEntry::new_prop("version", v.as_str()));
            }
        }
    }
    node
}

fn settings_to_kdl(
    settings: &HashMap<String, HashMap<String, kasane_plugin_model::SettingValue>>,
) -> kdl::KdlNode {
    let mut plugin_children: Vec<kdl::KdlNode> = settings
        .iter()
        .map(|(id, values)| {
            let mut kv_nodes: Vec<kdl::KdlNode> = values
                .iter()
                .map(|(key, sv)| setting_value_to_kdl(key, sv))
                .collect();
            kv_nodes.sort_by(|a, b| a.name().value().cmp(b.name().value()));
            section_with_children(id, kv_nodes)
        })
        .collect();
    plugin_children.sort_by(|a, b| a.name().value().cmp(b.name().value()));
    section_with_children("settings", plugin_children)
}

fn setting_value_to_kdl(key: &str, sv: &kasane_plugin_model::SettingValue) -> kdl::KdlNode {
    let mut n = make_node(key);
    match sv {
        kasane_plugin_model::SettingValue::Bool(b) => {
            n.push(kdl::KdlEntry::new(kdl::KdlValue::Bool(*b)));
        }
        kasane_plugin_model::SettingValue::Integer(i) => {
            n.push(kdl::KdlEntry::new(kdl::KdlValue::Integer(*i as i128)));
        }
        kasane_plugin_model::SettingValue::Float(f) => {
            n.push(kdl::KdlEntry::new(kdl::KdlValue::Float(*f)));
        }
        kasane_plugin_model::SettingValue::Str(s) => {
            n.push(kdl::KdlEntry::new(s.as_str()));
        }
    }
    n
}
