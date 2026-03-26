//! View composition and ephemeral surface synchronization.

use crate::element::Element;

use super::*;

impl SurfaceRegistry {
    fn render_surface_outcome(
        &self,
        entry: &RegisteredSurface,
        state: &AppState,
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
        rect: Rect,
        focused: bool,
    ) -> SurfaceRenderOutcome {
        let pane_state = pane_states
            .and_then(|ps| ps.state_for_surface(entry.descriptor.surface_id))
            .unwrap_or(state);
        let ctx = ViewContext {
            state: pane_state,
            global_state: state,
            rect,
            focused,
            registry: plugin_registry,
            surface_id: entry.descriptor.surface_id,
            pane_context: crate::plugin::PaneContext::new(entry.descriptor.surface_id, focused),
        };
        let abstract_root = entry.surface.view(&ctx);
        resolve::resolve_surface_tree(
            &entry.descriptor,
            abstract_root,
            state,
            plugin_registry,
            rect,
        )
    }

    /// Compose the full Element tree from all surfaces according to workspace layout.
    ///
    /// For each surface in the workspace tree, calls `surface.view()` with the
    /// allocated rectangle, then assembles results following the tree structure:
    /// - Split -> Flex (Row or Column)
    /// - Tabs -> active tab only (tab bar rendered separately)
    /// - Float -> Stack with overlays
    /// - Leaf -> direct Surface::view() output
    pub fn compose_view(
        &self,
        state: &AppState,
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> Element {
        self.compose_base_result(state, None, plugin_registry, total)
            .base
            .unwrap_or(Element::Empty)
    }

    /// Compose the full UI: workspace content + status bar + overlays.
    ///
    /// This is the surface-based base composition path used by `view()`. It:
    /// 1. Renders the workspace tree content (buffer panes) via Surface::view()
    /// 2. Adds the StatusBarSurface output (top or bottom based on `status_at_top`)
    /// 3. Uses the existing view layer for overlay positioning (menu, info, plugin)
    ///
    /// Overlay surfaces (menu, info) are managed via `sync_ephemeral_surfaces()`
    /// for lifecycle, but their view output uses the existing positioning functions
    /// from `render::view` which correctly compute anchor positions.
    pub fn compose_full_view(
        &self,
        state: &AppState,
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> Element {
        use crate::render::view;
        let base = self
            .compose_base_result(state, None, plugin_registry, total)
            .base
            .unwrap_or(Element::Empty);

        // 4. Collect overlays using the view layer's positioning functions.
        // These correctly compute anchor positions for menus and info popups.
        let mut overlays = Vec::new();
        if let Some(overlay) = view::build_menu_section_standalone(state, plugin_registry) {
            overlays.push(overlay);
        }
        overlays.extend(view::build_info_section_standalone(state, plugin_registry));
        {
            let overlay_ctx = crate::plugin::OverlayContext {
                screen_cols: state.cols,
                screen_rows: state.rows,
                menu_rect: None,
                existing_overlays: vec![],
                focused_surface_id: Some(self.workspace.focused()),
            };
            overlays.extend(
                plugin_registry
                    .collect_overlays_with_ctx(&AppView::new(state), &overlay_ctx)
                    .into_iter()
                    .map(|oc| crate::element::Overlay {
                        element: oc.element,
                        anchor: oc.anchor,
                    }),
            );
        }

        // 5. Assemble into final tree
        if overlays.is_empty() {
            base
        } else {
            Element::stack(base, overlays)
        }
    }

    pub(crate) fn compose_base_result(
        &self,
        state: &AppState,
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> SurfaceComposeResult {
        use crate::element::FlexChild;
        let rects = self.workspace.compute_rects(total);
        let (workspace_content, mut surface_reports) = self.compose_node_with_reports(
            self.workspace.root(),
            state,
            pane_states,
            plugin_registry,
            &rects,
        );

        // In multi-pane mode, the status bar should reflect the focused pane's
        // content (prompt, mode line, etc.), not the primary session's.
        let focused = self.workspace.focused();
        let status_state = pane_states
            .and_then(|ps| ps.state_for_surface_or_focused(SurfaceId::STATUS, focused))
            .unwrap_or(state);
        let status_bar = self.surfaces.get(&SurfaceId::STATUS).map(|entry| {
            let ctx = ViewContext {
                state: status_state,
                global_state: state,
                rect: total,
                focused: focused == SurfaceId::STATUS,
                registry: plugin_registry,
                surface_id: entry.descriptor.surface_id,
                pane_context: crate::plugin::PaneContext::new(
                    entry.descriptor.surface_id,
                    focused == SurfaceId::STATUS,
                ),
            };
            let abstract_root = entry.surface.view(&ctx);
            let outcome = resolve::resolve_surface_tree(
                &entry.descriptor,
                abstract_root,
                state,
                plugin_registry,
                total,
            );
            surface_reports.push(outcome.report);
            outcome.tree.map(|tree| tree.root).unwrap_or(Element::Empty)
        });

        let base = match status_bar {
            Some(status) => {
                let mut children = Vec::new();
                if state.status_at_top {
                    children.push(FlexChild::fixed(status));
                    children.push(FlexChild::flexible(workspace_content, 1.0));
                } else {
                    children.push(FlexChild::flexible(workspace_content, 1.0));
                    children.push(FlexChild::fixed(status));
                }
                Element::column(children)
            }
            None => workspace_content,
        };

        SurfaceComposeResult {
            base: Some(base),
            surface_reports,
        }
    }

    /// Compose view decomposed into sections for per-section caching.
    ///
    /// Returns the same structure as `view::ViewSections`:
    /// - `base`: workspace content + status bar
    /// - `menu_overlay`, `info_overlays`, `plugin_overlays`: overlay sections
    pub fn compose_view_sections(
        &self,
        state: &AppState,
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> crate::render::view::ViewSections {
        use crate::render::view;

        let base_result = self.compose_base_result(state, pane_states, plugin_registry, total);
        let menu_overlay = view::build_menu_section_standalone(state, plugin_registry);
        let info_overlays = view::build_info_section_standalone(state, plugin_registry);
        let overlay_ctx = crate::plugin::OverlayContext {
            screen_cols: state.cols,
            screen_rows: state.rows,
            menu_rect: None,
            existing_overlays: vec![],
            focused_surface_id: Some(self.workspace.focused()),
        };
        let app_view = AppView::new(state);
        let plugin_overlays: Vec<crate::element::Overlay> = plugin_registry
            .collect_overlays_with_ctx(&app_view, &overlay_ctx)
            .into_iter()
            .map(|oc| crate::element::Overlay {
                element: oc.element,
                anchor: oc.anchor,
            })
            .collect();

        let display_map = plugin_registry.collect_display_map(&app_view);
        let focused = self.workspace.focused();
        let focused_pane_rect = self.workspace.compute_rects(total).get(&focused).copied();
        let focused_pane_state = pane_states
            .and_then(|ps| ps.state_for_surface(focused))
            .map(|s| Box::new(s.clone()));
        view::ViewSections {
            base: base_result.base.unwrap_or(Element::Empty),
            menu_overlay,
            info_overlays,
            plugin_overlays,
            surface_reports: base_result.surface_reports,
            display_map,
            display_scroll_offset: 0,
            focused_pane_rect,
            focused_pane_state,
        }
    }

    fn compose_node_with_reports(
        &self,
        node: &crate::workspace::WorkspaceNode,
        state: &AppState,
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
        rects: &HashMap<SurfaceId, Rect>,
    ) -> (Element, Vec<SurfaceRenderReport>) {
        use crate::element::FlexChild;
        use crate::workspace::WorkspaceNode;

        match node {
            WorkspaceNode::Leaf { surface_id } => {
                if let Some(entry) = self.surfaces.get(surface_id) {
                    let rect = rects.get(surface_id).copied().unwrap_or(Rect {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                    });
                    let outcome = self.render_surface_outcome(
                        entry,
                        state,
                        pane_states,
                        plugin_registry,
                        rect,
                        self.workspace.focused() == *surface_id,
                    );
                    (
                        outcome.tree.map(|tree| tree.root).unwrap_or(Element::Empty),
                        vec![outcome.report],
                    )
                } else {
                    (Element::Empty, vec![])
                }
            }
            WorkspaceNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let focused = self.workspace.focused();
                let is_focused_adjacent = first.has_on_trailing_edge(focused, *direction)
                    || second.has_on_leading_edge(focused, *direction);
                let divider_token = if is_focused_adjacent {
                    crate::element::StyleToken::SPLIT_DIVIDER_FOCUSED
                } else {
                    crate::element::StyleToken::SPLIT_DIVIDER
                };
                let divider =
                    Element::container(Element::Empty, crate::element::Style::Token(divider_token));
                let elem_direction = match direction {
                    crate::layout::SplitDirection::Vertical => crate::element::Direction::Row,
                    crate::layout::SplitDirection::Horizontal => crate::element::Direction::Column,
                };
                let (first_elem, mut first_reports) = self.compose_node_with_reports(
                    first,
                    state,
                    pane_states,
                    plugin_registry,
                    rects,
                );
                let (second_elem, second_reports) = self.compose_node_with_reports(
                    second,
                    state,
                    pane_states,
                    plugin_registry,
                    rects,
                );
                first_reports.extend(second_reports);
                (
                    Element::Flex {
                        direction: elem_direction,
                        children: vec![
                            FlexChild::flexible(first_elem, *ratio),
                            FlexChild {
                                element: divider,
                                flex: 0.0,
                                min_size: Some(1),
                                max_size: Some(1),
                            },
                            FlexChild::flexible(second_elem, 1.0 - *ratio),
                        ],
                        gap: 0,
                        align: crate::element::Align::Start,
                        cross_align: crate::element::Align::Start,
                    },
                    first_reports,
                )
            }
            WorkspaceNode::Tabs { tabs, active, .. } => {
                if let Some(active_tab) = tabs.get(*active) {
                    self.compose_node_with_reports(
                        active_tab,
                        state,
                        pane_states,
                        plugin_registry,
                        rects,
                    )
                } else {
                    (Element::Empty, vec![])
                }
            }
            WorkspaceNode::Float { base, floating } => {
                let (base_elem, mut surface_reports) = self.compose_node_with_reports(
                    base,
                    state,
                    pane_states,
                    plugin_registry,
                    rects,
                );
                let mut overlays = Vec::new();
                for entry in floating {
                    let (overlay_elem, overlay_reports) = self.compose_node_with_reports(
                        &entry.node,
                        state,
                        pane_states,
                        plugin_registry,
                        rects,
                    );
                    surface_reports.extend(overlay_reports);
                    overlays.push(crate::element::Overlay {
                        element: overlay_elem,
                        anchor: crate::element::OverlayAnchor::Absolute {
                            x: entry.rect.x,
                            y: entry.rect.y,
                            w: entry.rect.w,
                            h: entry.rect.h,
                        },
                    });
                }
                let composed = if overlays.is_empty() {
                    base_elem
                } else {
                    Element::stack(base_elem, overlays)
                };
                (composed, surface_reports)
            }
        }
    }

    /// Synchronize ephemeral surfaces (menu, infos) with the current AppState.
    ///
    /// Registers/removes MenuSurface and InfoSurface instances to match
    /// whether `state.menu` and `state.infos` are present.
    pub fn sync_ephemeral_surfaces(&mut self, state: &AppState) {
        // Menu surface
        if state.menu.is_some() {
            if !self.surfaces.contains_key(&SurfaceId::MENU) {
                self.register(Box::new(super::super::menu::MenuSurface));
            }
        } else {
            self.remove(SurfaceId::MENU);
        }

        // Info surfaces: one per info popup
        // Remove stale info surfaces
        let info_count = state.infos.len();
        let stale_ids: Vec<SurfaceId> = self
            .surfaces
            .keys()
            .filter(|id| {
                id.0 >= SurfaceId::INFO_BASE
                    && id.0 < SurfaceId::PLUGIN_BASE
                    && (id.0 - SurfaceId::INFO_BASE) as usize >= info_count
            })
            .copied()
            .collect();
        for id in stale_ids {
            self.remove(id);
        }
        // Add missing info surfaces
        for i in 0..info_count {
            let id = SurfaceId(SurfaceId::INFO_BASE + i as u32);
            if !self.surfaces.contains_key(&id) {
                self.register(Box::new(super::super::info::InfoSurface::new(i)));
            }
        }
    }
}
