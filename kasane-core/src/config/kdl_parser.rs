//! KDL → Config parsing.

use std::collections::HashMap;

use compact_str::CompactString;
use kasane_plugin_model::SettingValue;

use super::{
    BorderStyleConfig, ClipboardConfig, ColorsConfig, Config, FontConfig, ImageProtocolConfig,
    LogConfig, MenuConfig, MenuPosition, MouseConfig, PluginSelection, PluginsConfig, ScrollConfig,
    SearchConfig, StatusPosition, ThemeConfig, ThemeValue, UiConfig, WindowConfig,
};

/// Parse config sections from pre-filtered KDL nodes.
///
/// Nodes whose names are not recognised config sections are silently ignored
/// (the caller is expected to route those to the widget parser).
pub fn parse_config_from_nodes(nodes: &[kdl::KdlNode]) -> Config {
    let mut config = Config::default();
    for node in nodes {
        match node.name().value() {
            "ui" => config.ui = parse_ui(node),
            "scroll" => config.scroll = parse_scroll(node),
            "log" => config.log = parse_log(node),
            "theme" => config.theme = parse_theme(node),
            "menu" => config.menu = parse_menu(node),
            "search" => config.search = parse_search(node),
            "clipboard" => config.clipboard = parse_clipboard(node),
            "mouse" => config.mouse = parse_mouse(node),
            "window" => config.window = parse_window(node),
            "font" => config.font = parse_font(node),
            "colors" => config.colors = parse_colors(node),
            "plugins" => config.plugins = parse_plugins(node),
            "settings" => config.settings = parse_settings(node),
            _ => {}
        }
    }
    config
}

// ── Helpers ──────────────────────────────────────────────────────────

fn first_bool(node: &kdl::KdlNode) -> Option<bool> {
    node.entry(0).and_then(|e| e.value().as_bool())
}

fn first_string(node: &kdl::KdlNode) -> Option<&str> {
    node.entry(0).and_then(|e| e.value().as_string())
}

fn first_i64(node: &kdl::KdlNode) -> Option<i64> {
    node.entry(0)
        .and_then(|e| e.value().as_integer().map(|i| i as i64))
}

fn first_f64(node: &kdl::KdlNode) -> Option<f64> {
    node.entry(0).and_then(|e| {
        e.value()
            .as_float()
            .or_else(|| e.value().as_integer().map(|i| i as f64))
    })
}

fn all_strings(node: &kdl::KdlNode) -> Vec<String> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| e.value().as_string().map(String::from))
        .collect()
}

/// Read a bool from either a child node or a property on the parent.
fn child_or_prop_bool(
    node: &kdl::KdlNode,
    children: Option<&kdl::KdlDocument>,
    key: &str,
) -> Option<bool> {
    if let Some(doc) = children
        && let Some(child) = doc.get(key)
        && let Some(v) = first_bool(child)
    {
        return Some(v);
    }
    node.entry(key).and_then(|e| e.value().as_bool())
}

fn child_or_prop_string<'a>(
    node: &'a kdl::KdlNode,
    children: Option<&'a kdl::KdlDocument>,
    key: &str,
) -> Option<&'a str> {
    if let Some(doc) = children
        && let Some(child) = doc.get(key)
        && let Some(v) = first_string(child)
    {
        return Some(v);
    }
    node.entry(key).and_then(|e| e.value().as_string())
}

fn child_or_prop_i64(
    node: &kdl::KdlNode,
    children: Option<&kdl::KdlDocument>,
    key: &str,
) -> Option<i64> {
    if let Some(doc) = children
        && let Some(child) = doc.get(key)
        && let Some(v) = first_i64(child)
    {
        return Some(v);
    }
    node.entry(key)
        .and_then(|e| e.value().as_integer().map(|i| i as i64))
}

fn child_or_prop_f64(
    node: &kdl::KdlNode,
    children: Option<&kdl::KdlDocument>,
    key: &str,
) -> Option<f64> {
    if let Some(doc) = children
        && let Some(child) = doc.get(key)
        && let Some(v) = first_f64(child)
    {
        return Some(v);
    }
    node.entry(key).and_then(|e| {
        e.value()
            .as_float()
            .or_else(|| e.value().as_integer().map(|i| i as f64))
    })
}

// ── Per-section parsers ──────────────────────────────────────────────

fn parse_ui(node: &kdl::KdlNode) -> UiConfig {
    let mut ui = UiConfig::default();
    let children = node.children();

    if let Some(v) = child_or_prop_bool(node, children, "shadow") {
        ui.shadow = v;
    }
    if let Some(v) = child_or_prop_string(node, children, "padding_char") {
        ui.padding_char = v.to_string();
    }
    if let Some(v) = child_or_prop_string(node, children, "border_style") {
        ui.border_style = match v {
            "single" => BorderStyleConfig::Single,
            "rounded" => BorderStyleConfig::Rounded,
            "double" => BorderStyleConfig::Double,
            "heavy" => BorderStyleConfig::Heavy,
            "ascii" => BorderStyleConfig::Ascii,
            _ => ui.border_style,
        };
    }
    if let Some(v) = child_or_prop_string(node, children, "status_position") {
        ui.status_position = match v {
            "top" => StatusPosition::Top,
            "bottom" => StatusPosition::Bottom,
            _ => ui.status_position,
        };
    }
    if let Some(v) = child_or_prop_string(node, children, "backend") {
        ui.backend = v.to_string();
    }
    if let Some(v) = child_or_prop_bool(node, children, "scene_renderer") {
        ui.scene_renderer = Some(v);
    }
    if let Some(v) = child_or_prop_string(node, children, "image_protocol") {
        ui.image_protocol = match v {
            "auto" => ImageProtocolConfig::Auto,
            "halfblock" => ImageProtocolConfig::Halfblock,
            "kitty" => ImageProtocolConfig::Kitty,
            _ => ui.image_protocol,
        };
    }
    ui
}

fn parse_scroll(node: &kdl::KdlNode) -> ScrollConfig {
    let mut s = ScrollConfig::default();
    let children = node.children();

    if let Some(v) = child_or_prop_i64(node, children, "lines_per_scroll") {
        s.lines_per_scroll = v as i32;
    }
    if let Some(v) = child_or_prop_bool(node, children, "smooth") {
        s.smooth = v;
    }
    if let Some(v) = child_or_prop_bool(node, children, "inertia") {
        s.inertia = v;
    }
    s
}

fn parse_log(node: &kdl::KdlNode) -> LogConfig {
    let mut l = LogConfig::default();
    let children = node.children();

    if let Some(v) = child_or_prop_string(node, children, "level") {
        l.level = v.to_string();
    }
    if let Some(v) = child_or_prop_string(node, children, "file") {
        l.file = Some(v.to_string());
    }
    l
}

fn parse_theme_value(spec: &str) -> ThemeValue {
    if let Some(name) = spec.strip_prefix('@') {
        ThemeValue::TokenRef(name.to_string())
    } else {
        ThemeValue::FaceSpec(spec.to_string())
    }
}

fn parse_theme(node: &kdl::KdlNode) -> ThemeConfig {
    let mut t = ThemeConfig::default();
    if let Some(doc) = node.children() {
        for child in doc.nodes() {
            let name = child.name().value();

            if name == "variant" {
                // variant "dark" { accent "cyan" }
                if let Some(variant_name) = first_string(child) {
                    let mut faces = HashMap::new();
                    if let Some(inner) = child.children() {
                        for entry in inner.nodes() {
                            let key = entry.name().value().to_string();
                            if let Some(spec) = first_string(entry) {
                                faces.insert(key, parse_theme_value(spec));
                            }
                        }
                    }
                    t.variants.insert(variant_name.to_string(), faces);
                }
            } else if let Some(spec) = first_string(child) {
                t.faces.insert(name.to_string(), parse_theme_value(spec));
            }
        }
    }
    t
}

fn parse_menu(node: &kdl::KdlNode) -> MenuConfig {
    let mut m = MenuConfig::default();
    let children = node.children();

    if let Some(v) = child_or_prop_string(node, children, "position") {
        m.position = match v {
            "auto" => MenuPosition::Auto,
            "above" => MenuPosition::Above,
            "below" => MenuPosition::Below,
            _ => m.position,
        };
    }
    if let Some(v) = child_or_prop_i64(node, children, "max_height") {
        m.max_height = v as u16;
    }
    m
}

fn parse_search(node: &kdl::KdlNode) -> SearchConfig {
    let mut s = SearchConfig::default();
    let children = node.children();
    if let Some(v) = child_or_prop_bool(node, children, "dropdown") {
        s.dropdown = v;
    }
    s
}

fn parse_clipboard(node: &kdl::KdlNode) -> ClipboardConfig {
    let mut c = ClipboardConfig::default();
    let children = node.children();
    if let Some(v) = child_or_prop_bool(node, children, "enabled") {
        c.enabled = v;
    }
    c
}

fn parse_mouse(node: &kdl::KdlNode) -> MouseConfig {
    let mut m = MouseConfig::default();
    let children = node.children();
    if let Some(v) = child_or_prop_bool(node, children, "drag_scroll") {
        m.drag_scroll = v;
    }
    m
}

fn parse_window(node: &kdl::KdlNode) -> WindowConfig {
    let mut w = WindowConfig::default();
    let children = node.children();

    if let Some(v) = child_or_prop_i64(node, children, "initial_cols") {
        w.initial_cols = v as u16;
    }
    if let Some(v) = child_or_prop_i64(node, children, "initial_rows") {
        w.initial_rows = v as u16;
    }
    if let Some(v) = child_or_prop_bool(node, children, "fullscreen") {
        w.fullscreen = v;
    }
    if let Some(v) = child_or_prop_bool(node, children, "maximized") {
        w.maximized = v;
    }
    if let Some(v) = child_or_prop_string(node, children, "present_mode") {
        w.present_mode = Some(v.to_string());
    }
    w
}

fn parse_font(node: &kdl::KdlNode) -> FontConfig {
    let mut f = FontConfig::default();
    let children = node.children();

    if let Some(v) = child_or_prop_string(node, children, "family") {
        f.family = v.to_string();
    }
    if let Some(v) = child_or_prop_f64(node, children, "size") {
        f.size = v as f32;
    }
    if let Some(v) = child_or_prop_string(node, children, "style") {
        f.style = v.to_string();
    }
    // fallback_list: multiple positional args on a child node
    if let Some(doc) = children
        && let Some(child) = doc.get("fallback_list")
    {
        f.fallback_list = all_strings(child);
    }
    if let Some(v) = child_or_prop_f64(node, children, "line_height") {
        f.line_height = v as f32;
    }
    if let Some(v) = child_or_prop_f64(node, children, "letter_spacing") {
        f.letter_spacing = v as f32;
    }
    f
}

fn parse_colors(node: &kdl::KdlNode) -> ColorsConfig {
    let mut c = ColorsConfig::default();
    let children = node.children();

    macro_rules! color_field {
        ($field:ident) => {
            if let Some(v) = child_or_prop_string(node, children, stringify!($field)) {
                c.$field = v.to_string();
            }
        };
    }

    color_field!(default_fg);
    color_field!(default_bg);
    color_field!(black);
    color_field!(red);
    color_field!(green);
    color_field!(yellow);
    color_field!(blue);
    color_field!(magenta);
    color_field!(cyan);
    color_field!(white);
    color_field!(bright_black);
    color_field!(bright_red);
    color_field!(bright_green);
    color_field!(bright_yellow);
    color_field!(bright_blue);
    color_field!(bright_magenta);
    color_field!(bright_cyan);
    color_field!(bright_white);

    c
}

fn parse_plugins(node: &kdl::KdlNode) -> PluginsConfig {
    let mut p = PluginsConfig::default();
    let Some(doc) = node.children() else {
        return p;
    };

    if let Some(child) = doc.get("path")
        && let Some(v) = first_string(child)
    {
        p.path = Some(v.to_string());
    }
    if let Some(child) = doc.get("enabled") {
        p.enabled = all_strings(child);
    }
    if let Some(child) = doc.get("disabled") {
        p.disabled = all_strings(child);
    }

    // deny_capabilities { plugin_id "cap1" "cap2" ; ... }
    if let Some(child) = doc.get("deny_capabilities")
        && let Some(inner) = child.children()
    {
        for entry_node in inner.nodes() {
            let id = entry_node.name().value().to_string();
            let caps = all_strings(entry_node);
            p.deny_capabilities.insert(id, caps);
        }
    }

    // deny_authorities { plugin_id "auth1" ; ... }
    if let Some(child) = doc.get("deny_authorities")
        && let Some(inner) = child.children()
    {
        for entry_node in inner.nodes() {
            let id = entry_node.name().value().to_string();
            let auths = all_strings(entry_node);
            p.deny_authorities.insert(id, auths);
        }
    }

    // selection { plugin_id mode="pin-digest" digest="sha256:abc" ; ... }
    if let Some(child) = doc.get("selection")
        && let Some(inner) = child.children()
    {
        for entry_node in inner.nodes() {
            let id = entry_node.name().value().to_string();
            let sel = parse_plugin_selection(entry_node);
            p.selection.insert(id, sel);
        }
    }

    p
}

fn parse_plugin_selection(node: &kdl::KdlNode) -> PluginSelection {
    match node.entry("mode").and_then(|e| e.value().as_string()) {
        Some("pin-digest") => {
            let digest = node
                .entry("digest")
                .and_then(|e| e.value().as_string())
                .unwrap_or("")
                .to_string();
            PluginSelection::PinDigest { digest }
        }
        Some("pin-package") => {
            let package = node
                .entry("package")
                .and_then(|e| e.value().as_string())
                .unwrap_or("")
                .to_string();
            let version = node
                .entry("version")
                .and_then(|e| e.value().as_string())
                .map(String::from);
            PluginSelection::PinPackage { package, version }
        }
        _ => PluginSelection::Auto,
    }
}

fn parse_settings(node: &kdl::KdlNode) -> HashMap<String, HashMap<String, SettingValue>> {
    let mut settings = HashMap::new();
    let Some(doc) = node.children() else {
        return settings;
    };

    for plugin_node in doc.nodes() {
        let plugin_id = plugin_node.name().value().to_string();
        let mut plugin_settings = HashMap::new();

        if let Some(inner) = plugin_node.children() {
            for setting_node in inner.nodes() {
                let key = setting_node.name().value().to_string();
                if let Some(sv) = kdl_value_to_setting(setting_node) {
                    plugin_settings.insert(key, sv);
                }
            }
        }

        if !plugin_settings.is_empty() {
            settings.insert(plugin_id, plugin_settings);
        }
    }

    settings
}

fn kdl_value_to_setting(node: &kdl::KdlNode) -> Option<SettingValue> {
    let entry = node.entry(0)?;
    let value = entry.value();
    value
        .as_bool()
        .map(SettingValue::Bool)
        .or_else(|| value.as_integer().map(|i| SettingValue::Integer(i as i64)))
        .or_else(|| value.as_float().map(SettingValue::Float))
        .or_else(|| {
            value
                .as_string()
                .map(|s| SettingValue::Str(CompactString::from(s)))
        })
}
