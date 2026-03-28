//! Pane layout persistence: save/restore workspace structure across sessions.
//!
//! Uses mirror types (`SavedLayout`, `SavedNode`) to avoid polluting runtime
//! types with serde derives. Conversion goes through `project` / `build_restored_tree`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::layout::SplitDirection;
use crate::session::{SessionId, SessionStateStore};
use crate::state::AppState;
use crate::surface::{SurfaceId, SurfaceRegistry};

use super::{Workspace, WorkspaceNode};

// ---------------------------------------------------------------------------
// Serialization types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedLayout {
    pub root: SavedNode,
    pub focused_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SavedNode {
    Leaf {
        surface_key: String,
        buffer_name: Option<String>,
    },
    Split {
        direction: SavedSplitDirection,
        ratio: f32,
        first: Box<SavedNode>,
        second: Box<SavedNode>,
    },
    Tabs {
        tabs: Vec<SavedNode>,
        active: usize,
        labels: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SavedSplitDirection {
    Horizontal,
    Vertical,
}

impl From<SplitDirection> for SavedSplitDirection {
    fn from(d: SplitDirection) -> Self {
        match d {
            SplitDirection::Horizontal => SavedSplitDirection::Horizontal,
            SplitDirection::Vertical => SavedSplitDirection::Vertical,
        }
    }
}

impl From<SavedSplitDirection> for SplitDirection {
    fn from(d: SavedSplitDirection) -> SplitDirection {
        match d {
            SavedSplitDirection::Horizontal => SplitDirection::Horizontal,
            SavedSplitDirection::Vertical => SplitDirection::Vertical,
        }
    }
}

// ---------------------------------------------------------------------------
// Step 1: project — WorkspaceNode → SavedLayout
// ---------------------------------------------------------------------------

/// Project the live workspace tree into a serializable `SavedLayout`.
///
/// Returns `None` if the layout is a single primary buffer (nothing to restore).
pub fn project(
    workspace: &Workspace,
    surface_registry: &SurfaceRegistry,
    session_states: &SessionStateStore,
    active_state: &AppState,
    active_session_id: Option<SessionId>,
) -> Option<SavedLayout> {
    let root = project_node(
        workspace.root(),
        surface_registry,
        session_states,
        active_state,
        active_session_id,
    )?;

    // Single primary buffer leaf → nothing to restore
    if matches!(&root, SavedNode::Leaf { surface_key, .. } if surface_key == "kasane.buffer") {
        return None;
    }

    let focused_key = surface_registry
        .descriptor(workspace.focused())
        .map(|d| d.surface_key.to_string());

    Some(SavedLayout { root, focused_key })
}

fn project_node(
    node: &WorkspaceNode,
    surface_registry: &SurfaceRegistry,
    session_states: &SessionStateStore,
    active_state: &AppState,
    active_session_id: Option<SessionId>,
) -> Option<SavedNode> {
    match node {
        WorkspaceNode::Leaf { surface_id } => {
            let descriptor = surface_registry.descriptor(*surface_id)?;
            let surface_key = descriptor.surface_key.to_string();

            let buffer_name = if surface_key == "kasane.buffer" {
                None
            } else {
                extract_buffer_name(
                    *surface_id,
                    surface_registry,
                    session_states,
                    active_state,
                    active_session_id,
                )
            };

            Some(SavedNode::Leaf {
                surface_key,
                buffer_name,
            })
        }
        WorkspaceNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let first_saved = project_node(
                first,
                surface_registry,
                session_states,
                active_state,
                active_session_id,
            );
            let second_saved = project_node(
                second,
                surface_registry,
                session_states,
                active_state,
                active_session_id,
            );

            match (first_saved, second_saved) {
                (Some(f), Some(s)) => Some(SavedNode::Split {
                    direction: (*direction).into(),
                    ratio: *ratio,
                    first: Box::new(f),
                    second: Box::new(s),
                }),
                (Some(f), None) => Some(f),
                (None, Some(s)) => Some(s),
                (None, None) => None,
            }
        }
        WorkspaceNode::Tabs {
            tabs,
            active,
            labels,
        } => {
            let mut saved_tabs = Vec::new();
            let mut saved_labels = Vec::new();
            let mut new_active = *active;

            for (i, tab) in tabs.iter().enumerate() {
                if let Some(saved) = project_node(
                    tab,
                    surface_registry,
                    session_states,
                    active_state,
                    active_session_id,
                ) {
                    if saved_tabs.len() <= new_active && i > *active {
                        // Active tab was skipped; adjust
                        new_active = saved_tabs.len().saturating_sub(1);
                    }
                    saved_tabs.push(saved);
                    saved_labels.push(labels.get(i).cloned().unwrap_or_else(|| format!("tab-{i}")));
                } else if i <= *active && new_active > 0 {
                    new_active = new_active.saturating_sub(1);
                }
            }

            match saved_tabs.len() {
                0 => None,
                1 => Some(saved_tabs.remove(0)),
                _ => {
                    new_active = new_active.min(saved_tabs.len() - 1);
                    Some(SavedNode::Tabs {
                        tabs: saved_tabs,
                        active: new_active,
                        labels: saved_labels,
                    })
                }
            }
        }
        WorkspaceNode::Float { base, .. } => {
            // Float: project base only, discard floating entries
            project_node(
                base,
                surface_registry,
                session_states,
                active_state,
                active_session_id,
            )
        }
    }
}

/// Extract buffer name for a pane surface, using the same logic as
/// `enriched_session_descriptors`: `ui_options["kasane_bufname"]` → status_content.
fn extract_buffer_name(
    surface_id: SurfaceId,
    surface_registry: &SurfaceRegistry,
    session_states: &SessionStateStore,
    active_state: &AppState,
    active_session_id: Option<SessionId>,
) -> Option<String> {
    let session_id = surface_registry.session_for_surface(surface_id)?;
    let is_active = active_session_id == Some(session_id);
    let app_state = if is_active {
        Some(active_state)
    } else {
        session_states.get(&session_id)
    };
    app_state.and_then(|s| {
        s.ui_options
            .get("kasane_bufname")
            .filter(|v| !v.is_empty())
            .cloned()
            .or_else(|| {
                let text: String = s
                    .status_content
                    .iter()
                    .map(|a| a.contents.as_str())
                    .collect();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
    })
}

// ---------------------------------------------------------------------------
// Step 2: File I/O
// ---------------------------------------------------------------------------

fn layout_dir() -> PathBuf {
    let base = if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        PathBuf::from(xdg).join("kasane")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local/state/kasane")
    } else {
        PathBuf::from("kasane-state")
    };
    base.join("layout")
}

/// Save the current workspace layout to disk (or delete the file if single-pane).
pub fn save_layout(
    session_name: &str,
    workspace: &Workspace,
    surface_registry: &SurfaceRegistry,
    session_states: &SessionStateStore,
    active_state: &AppState,
    active_session_id: Option<SessionId>,
) {
    let saved = project(
        workspace,
        surface_registry,
        session_states,
        active_state,
        active_session_id,
    );

    let dir = layout_dir();
    let path = dir.join(format!("{session_name}.json"));

    match saved {
        Some(layout) => {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                tracing::warn!("failed to create layout dir: {e}");
                return;
            }
            let Ok(json) = serde_json::to_string_pretty(&layout) else {
                tracing::warn!("failed to serialize layout");
                return;
            };
            let tmp = dir.join(format!("{session_name}.json.tmp"));
            if let Err(e) = std::fs::write(&tmp, &json) {
                tracing::warn!("failed to write layout file: {e}");
                return;
            }
            if let Err(e) = std::fs::rename(&tmp, &path) {
                tracing::warn!("failed to rename layout file: {e}");
            }
        }
        None => {
            // Single pane or empty — remove stale file
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Load a saved layout from disk.
pub fn load_layout(session_name: &str) -> Option<SavedLayout> {
    let path = layout_dir().join(format!("{session_name}.json"));
    let json = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}

/// Delete a saved layout file.  No-op if the file does not exist.
pub fn delete_layout(session_name: &str) {
    let path = layout_dir().join(format!("{session_name}.json"));
    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Step 3: plan_restore
// ---------------------------------------------------------------------------

/// A pane to spawn during restore.
#[derive(Debug, Clone)]
pub struct PaneToSpawn {
    pub pane_key: String,
    pub buffer_name: Option<String>,
}

/// Plan for restoring a saved layout: the saved tree plus panes to spawn.
#[derive(Debug)]
pub struct RestorePlan {
    pub saved: SavedLayout,
    pub panes: Vec<PaneToSpawn>,
}

/// Walk the saved tree and collect all non-primary panes that need spawning.
///
/// Returns `None` if there are no panes to spawn beyond the primary buffer.
pub fn plan_restore(saved: SavedLayout) -> Option<RestorePlan> {
    let mut panes = Vec::new();
    collect_panes(&saved.root, &mut panes);
    if panes.is_empty() {
        return None;
    }
    Some(RestorePlan { saved, panes })
}

fn collect_panes(node: &SavedNode, panes: &mut Vec<PaneToSpawn>) {
    match node {
        SavedNode::Leaf {
            surface_key,
            buffer_name,
        } => {
            if surface_key != "kasane.buffer" {
                panes.push(PaneToSpawn {
                    pane_key: surface_key.clone(),
                    buffer_name: buffer_name.clone(),
                });
            }
        }
        SavedNode::Split { first, second, .. } => {
            collect_panes(first, panes);
            collect_panes(second, panes);
        }
        SavedNode::Tabs { tabs, .. } => {
            for tab in tabs {
                collect_panes(tab, panes);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Step 4: build_restored_tree
// ---------------------------------------------------------------------------

/// Result of building a restored workspace tree.
pub struct RestoredTree {
    pub root: WorkspaceNode,
    pub next_id_min: u32,
}

/// Build a `WorkspaceNode` tree from a saved layout and an ID mapping.
///
/// `id_map` maps pane surface keys to their newly allocated `SurfaceId`s.
/// Missing keys degrade gracefully: if one child of a split is missing,
/// the other is promoted; if all children are missing, returns `None`.
pub fn build_restored_tree(
    saved: &SavedNode,
    id_map: &HashMap<String, SurfaceId>,
) -> Option<RestoredTree> {
    let root = build_node(saved, id_map)?;
    let mut max_id = 0u32;
    collect_max_id(&root, &mut max_id);
    Some(RestoredTree {
        root,
        next_id_min: max_id + 1,
    })
}

fn build_node(saved: &SavedNode, id_map: &HashMap<String, SurfaceId>) -> Option<WorkspaceNode> {
    match saved {
        SavedNode::Leaf { surface_key, .. } => {
            if surface_key == "kasane.buffer" {
                Some(WorkspaceNode::Leaf {
                    surface_id: SurfaceId::BUFFER,
                })
            } else {
                let surface_id = id_map.get(surface_key)?;
                Some(WorkspaceNode::Leaf {
                    surface_id: *surface_id,
                })
            }
        }
        SavedNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let first_node = build_node(first, id_map);
            let second_node = build_node(second, id_map);

            match (first_node, second_node) {
                (Some(f), Some(s)) => Some(WorkspaceNode::Split {
                    direction: (*direction).into(),
                    ratio: *ratio,
                    first: Box::new(f),
                    second: Box::new(s),
                }),
                (Some(f), None) => Some(f),
                (None, Some(s)) => Some(s),
                (None, None) => None,
            }
        }
        SavedNode::Tabs {
            tabs,
            active,
            labels,
        } => {
            let mut built_tabs = Vec::new();
            let mut built_labels = Vec::new();
            let mut new_active = *active;

            for (i, tab) in tabs.iter().enumerate() {
                if let Some(node) = build_node(tab, id_map) {
                    if built_tabs.len() <= new_active && i > *active {
                        new_active = built_tabs.len().saturating_sub(1);
                    }
                    built_tabs.push(node);
                    built_labels.push(labels.get(i).cloned().unwrap_or_else(|| format!("tab-{i}")));
                } else if i <= *active && new_active > 0 {
                    new_active = new_active.saturating_sub(1);
                }
            }

            match built_tabs.len() {
                0 => None,
                1 => Some(built_tabs.remove(0)),
                _ => {
                    new_active = new_active.min(built_tabs.len() - 1);
                    Some(WorkspaceNode::Tabs {
                        tabs: built_tabs,
                        active: new_active,
                        labels: built_labels,
                    })
                }
            }
        }
    }
}

fn collect_max_id(node: &WorkspaceNode, max: &mut u32) {
    match node {
        WorkspaceNode::Leaf { surface_id } => {
            *max = (*max).max(surface_id.0);
        }
        WorkspaceNode::Split { first, second, .. } => {
            collect_max_id(first, max);
            collect_max_id(second, max);
        }
        WorkspaceNode::Tabs { tabs, .. } => {
            for tab in tabs {
                collect_max_id(tab, max);
            }
        }
        WorkspaceNode::Float { base, floating } => {
            collect_max_id(base, max);
            for entry in floating {
                collect_max_id(&entry.node, max);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Step 11: Kakoune quoting
// ---------------------------------------------------------------------------

/// Quote a string for Kakoune: `'...'` with `'` → `''`.
pub fn kak_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_loop::register_builtin_surfaces;
    use crate::layout::SplitDirection;
    use crate::session::SessionId;
    use crate::surface::buffer::ClientBufferSurface;
    /// Helper: build a surface registry with builtins and optionally add pane surfaces.
    fn test_registry() -> SurfaceRegistry {
        let mut r = SurfaceRegistry::new();
        register_builtin_surfaces(&mut r);
        r
    }

    fn add_pane(surface_registry: &mut SurfaceRegistry, surface_id: SurfaceId, key: &str) {
        surface_registry.register(Box::new(ClientBufferSurface::with_key(surface_id, key)));
    }

    // 1. Single pane returns None
    #[test]
    fn test_project_single_pane_returns_none() {
        let registry = test_registry();
        let state = AppState::default();
        let store = SessionStateStore::new();

        let result = project(registry.workspace(), &registry, &store, &state, None);
        assert!(result.is_none());
    }

    // 2. Simple split
    #[test]
    fn test_project_simple_split() {
        let mut registry = test_registry();
        let state = AppState::default();
        let store = SessionStateStore::new();

        let pane_id = SurfaceId(100);
        add_pane(&mut registry, pane_id, "pane-0");

        // Split the workspace
        let ws = registry.workspace_mut();
        ws.root_mut()
            .split(SurfaceId::BUFFER, SplitDirection::Vertical, 0.5, pane_id);

        let result = project(registry.workspace(), &registry, &store, &state, None);
        assert!(result.is_some());

        let layout = result.unwrap();
        match &layout.root {
            SavedNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                assert_eq!(*direction, SavedSplitDirection::Vertical);
                assert!((ratio - 0.5).abs() < f32::EPSILON);
                match first.as_ref() {
                    SavedNode::Leaf { surface_key, .. } => {
                        assert_eq!(surface_key, "kasane.buffer");
                    }
                    other => panic!("expected Leaf, got {other:?}"),
                }
                match second.as_ref() {
                    SavedNode::Leaf { surface_key, .. } => {
                        assert_eq!(surface_key, "pane-0");
                    }
                    other => panic!("expected Leaf, got {other:?}"),
                }
            }
            other => panic!("expected Split, got {other:?}"),
        }
    }

    // 3. Float is stripped to base
    #[test]
    fn test_project_strips_float() {
        let mut registry = test_registry();
        let state = AppState::default();
        let store = SessionStateStore::new();

        let pane_id = SurfaceId(100);
        add_pane(&mut registry, pane_id, "pane-0");

        // Split, then float one
        let ws = registry.workspace_mut();
        ws.root_mut()
            .split(SurfaceId::BUFFER, SplitDirection::Vertical, 0.5, pane_id);
        let float_id = SurfaceId(101);
        add_pane(&mut registry, float_id, "float-0");
        registry.workspace_mut().add_floating(
            float_id,
            crate::layout::Rect {
                x: 5,
                y: 5,
                w: 20,
                h: 10,
            },
        );

        let result = project(registry.workspace(), &registry, &store, &state, None);
        assert!(result.is_some());

        // The float should be stripped, leaving only the split
        let layout = result.unwrap();
        assert!(matches!(&layout.root, SavedNode::Split { .. }));
    }

    // 4. Unknown surface collapses parent
    #[test]
    fn test_project_collapses_orphan_leaf() {
        let mut registry = test_registry();
        let state = AppState::default();
        let store = SessionStateStore::new();

        // Manually construct a split with an unregistered surface
        let unknown_id = SurfaceId(999);
        let ws = registry.workspace_mut();
        ws.root_mut()
            .split(SurfaceId::BUFFER, SplitDirection::Vertical, 0.5, unknown_id);

        let result = project(registry.workspace(), &registry, &store, &state, None);
        // Should collapse to just BUFFER → None
        assert!(result.is_none());
    }

    // 5. Round-trip simple
    #[test]
    fn test_round_trip_simple() {
        let mut registry = test_registry();
        let state = AppState::default();
        let store = SessionStateStore::new();

        let pane_id = SurfaceId(100);
        add_pane(&mut registry, pane_id, "pane-0");

        let ws = registry.workspace_mut();
        ws.root_mut()
            .split(SurfaceId::BUFFER, SplitDirection::Vertical, 0.5, pane_id);

        let layout = project(registry.workspace(), &registry, &store, &state, None).unwrap();

        let json = serde_json::to_string_pretty(&layout).unwrap();
        let loaded: SavedLayout = serde_json::from_str(&json).unwrap();

        let mut id_map = HashMap::new();
        id_map.insert("pane-0".to_string(), SurfaceId(200));

        let restored = build_restored_tree(&loaded.root, &id_map).unwrap();

        // Structurally equivalent: Split with BUFFER and pane-0
        let ids = restored.root.collect_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&SurfaceId::BUFFER));
        assert!(ids.contains(&SurfaceId(200)));
    }

    // 6. Round-trip three-way split
    #[test]
    fn test_round_trip_three_way_split() {
        let mut registry = test_registry();
        let state = AppState::default();
        let store = SessionStateStore::new();

        let pane1 = SurfaceId(100);
        let pane2 = SurfaceId(101);
        add_pane(&mut registry, pane1, "pane-1");
        add_pane(&mut registry, pane2, "pane-2");

        // BUFFER | (pane-1 / pane-2)
        let ws = registry.workspace_mut();
        ws.root_mut()
            .split(SurfaceId::BUFFER, SplitDirection::Vertical, 0.5, pane1);
        ws.root_mut()
            .split(pane1, SplitDirection::Horizontal, 0.5, pane2);

        let layout = project(registry.workspace(), &registry, &store, &state, None).unwrap();

        let json = serde_json::to_string_pretty(&layout).unwrap();
        let loaded: SavedLayout = serde_json::from_str(&json).unwrap();

        let mut id_map = HashMap::new();
        id_map.insert("pane-1".to_string(), SurfaceId(300));
        id_map.insert("pane-2".to_string(), SurfaceId(301));

        let restored = build_restored_tree(&loaded.root, &id_map).unwrap();
        let ids = restored.root.collect_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&SurfaceId::BUFFER));
        assert!(ids.contains(&SurfaceId(300)));
        assert!(ids.contains(&SurfaceId(301)));
    }

    // 7. Missing pane degrades
    #[test]
    fn test_build_restored_tree_missing_pane_degrades() {
        let saved = SavedNode::Split {
            direction: SavedSplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(SavedNode::Leaf {
                surface_key: "kasane.buffer".to_string(),
                buffer_name: None,
            }),
            second: Box::new(SavedNode::Leaf {
                surface_key: "pane-0".to_string(),
                buffer_name: None,
            }),
        };

        // Empty id_map: pane-0 is missing
        let id_map = HashMap::new();
        let restored = build_restored_tree(&saved, &id_map).unwrap();

        // Should degrade to just the BUFFER leaf
        let ids = restored.root.collect_ids();
        assert_eq!(ids, vec![SurfaceId::BUFFER]);
    }

    // 8. All panes missing returns primary
    #[test]
    fn test_build_restored_tree_all_missing_returns_primary() {
        let saved = SavedNode::Split {
            direction: SavedSplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(SavedNode::Leaf {
                surface_key: "pane-a".to_string(),
                buffer_name: None,
            }),
            second: Box::new(SavedNode::Leaf {
                surface_key: "pane-b".to_string(),
                buffer_name: None,
            }),
        };

        let id_map = HashMap::new();
        let result = build_restored_tree(&saved, &id_map);
        // Both missing → None
        assert!(result.is_none());
    }

    // 9. Serialization round-trip
    #[test]
    fn test_serialization_round_trip() {
        let layout = SavedLayout {
            root: SavedNode::Split {
                direction: SavedSplitDirection::Vertical,
                ratio: 0.4,
                first: Box::new(SavedNode::Leaf {
                    surface_key: "kasane.buffer".to_string(),
                    buffer_name: None,
                }),
                second: Box::new(SavedNode::Tabs {
                    tabs: vec![
                        SavedNode::Leaf {
                            surface_key: "pane-0".to_string(),
                            buffer_name: Some("main.rs".to_string()),
                        },
                        SavedNode::Leaf {
                            surface_key: "pane-1".to_string(),
                            buffer_name: None,
                        },
                    ],
                    active: 0,
                    labels: vec!["tab0".to_string(), "tab1".to_string()],
                }),
            },
            focused_key: Some("pane-0".to_string()),
        };

        let json = serde_json::to_string_pretty(&layout).unwrap();
        let round_tripped: SavedLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, round_tripped);
    }

    // 10. project returns None for single pane → save_layout deletes file
    #[test]
    fn test_save_delete_on_single_pane() {
        // Verify the precondition: project returns None for a single-pane workspace
        let registry = test_registry();
        let state = AppState::default();
        let store = SessionStateStore::new();

        let result = project(registry.workspace(), &registry, &store, &state, None);
        assert!(
            result.is_none(),
            "single-pane workspace must project to None"
        );
    }

    // 11. Buffer name extraction
    #[test]
    fn test_buffer_name_extraction() {
        let mut registry = test_registry();
        let pane_id = SurfaceId(100);
        add_pane(&mut registry, pane_id, "pane-0");

        let ws = registry.workspace_mut();
        ws.root_mut()
            .split(SurfaceId::BUFFER, SplitDirection::Vertical, 0.5, pane_id);

        // Bind session and set ui_options
        let session_id = SessionId(1);
        registry.bind_session(pane_id, session_id);

        let mut pane_state = AppState::default();
        pane_state
            .ui_options
            .insert("kasane_bufname".into(), "lib.rs".into());

        let mut store = SessionStateStore::new();
        store.sync_from_active(session_id, &pane_state);

        let active_state = AppState::default();
        let layout = project(registry.workspace(), &registry, &store, &active_state, None).unwrap();

        // Find the pane-0 leaf and check buffer name
        match &layout.root {
            SavedNode::Split { second, .. } => match second.as_ref() {
                SavedNode::Leaf { buffer_name, .. } => {
                    assert_eq!(buffer_name.as_deref(), Some("lib.rs"));
                }
                other => panic!("expected Leaf, got {other:?}"),
            },
            other => panic!("expected Split, got {other:?}"),
        }
    }

    // 12. kak_quote
    #[test]
    fn test_kak_quote() {
        assert_eq!(kak_quote("hello"), "'hello'");
        assert_eq!(kak_quote("it's"), "'it''s'");
        assert_eq!(kak_quote("a'b'c"), "'a''b''c'");
        assert_eq!(kak_quote(""), "''");
    }

    // plan_restore returns None for single buffer
    #[test]
    fn test_plan_restore_single_buffer_returns_none() {
        let saved = SavedLayout {
            root: SavedNode::Leaf {
                surface_key: "kasane.buffer".to_string(),
                buffer_name: None,
            },
            focused_key: None,
        };
        assert!(plan_restore(saved).is_none());
    }

    // plan_restore collects panes
    #[test]
    fn test_plan_restore_collects_panes() {
        let saved = SavedLayout {
            root: SavedNode::Split {
                direction: SavedSplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(SavedNode::Leaf {
                    surface_key: "kasane.buffer".to_string(),
                    buffer_name: None,
                }),
                second: Box::new(SavedNode::Leaf {
                    surface_key: "pane-0".to_string(),
                    buffer_name: Some("main.rs".to_string()),
                }),
            },
            focused_key: Some("pane-0".to_string()),
        };

        let plan = plan_restore(saved).unwrap();
        assert_eq!(plan.panes.len(), 1);
        assert_eq!(plan.panes[0].pane_key, "pane-0");
        assert_eq!(plan.panes[0].buffer_name.as_deref(), Some("main.rs"));
    }
}
