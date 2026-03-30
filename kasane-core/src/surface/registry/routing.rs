//! State change notification and event routing.

use super::*;

impl SurfaceRegistry {
    /// Notify all surfaces of a state change.
    pub fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        self.on_state_changed_with_sources(state, dirty)
            .into_iter()
            .flat_map(|entry| entry.commands)
            .collect()
    }

    /// Notify all surfaces of a state change and preserve source plugins.
    pub fn on_state_changed_with_sources(
        &mut self,
        state: &AppState,
        dirty: DirtyFlags,
    ) -> Vec<SourcedSurfaceCommands> {
        let mut results = Vec::new();
        for entry in self.surfaces.values_mut() {
            let commands = entry.surface.on_state_changed(state, dirty);
            if !commands.is_empty() {
                results.push(SourcedSurfaceCommands {
                    source_plugin: entry.owner_plugin.clone(),
                    commands,
                });
            }
        }
        results
    }

    /// Route an event to the appropriate surface.
    pub fn route_event(
        &mut self,
        event: SurfaceEvent,
        state: &AppState,
        total: Rect,
    ) -> Vec<Command> {
        self.route_event_with_sources(event, state, total)
            .into_iter()
            .flat_map(|entry| entry.commands)
            .collect()
    }

    /// Route an event and preserve the source plugin for each surface-local command batch.
    pub fn route_event_with_sources(
        &mut self,
        event: SurfaceEvent,
        state: &AppState,
        total: Rect,
    ) -> Vec<SourcedSurfaceCommands> {
        match &event {
            SurfaceEvent::Key(_) | SurfaceEvent::FocusGained | SurfaceEvent::FocusLost => {
                // Route to focused surface
                let focused = self.workspace.focused();
                if let Some(entry) = self.surfaces.get_mut(&focused) {
                    let rect = self
                        .workspace
                        .compute_rects(total)
                        .get(&focused)
                        .copied()
                        .unwrap_or(Rect {
                            x: 0,
                            y: 0,
                            w: 0,
                            h: 0,
                        });
                    let ctx = EventContext {
                        state,
                        rect,
                        focused: !matches!(event, SurfaceEvent::FocusLost),
                    };
                    let commands = entry.surface.handle_event(event, &ctx);
                    if commands.is_empty() {
                        vec![]
                    } else {
                        vec![SourcedSurfaceCommands {
                            source_plugin: entry.owner_plugin.clone(),
                            commands,
                        }]
                    }
                } else {
                    vec![]
                }
            }
            SurfaceEvent::Drop(drop_event) => {
                // Route to surface under drop position
                let target = self
                    .workspace
                    .surface_at(drop_event.col, drop_event.row, total);
                if let Some(surface_id) = target {
                    if let Some(entry) = self.surfaces.get_mut(&surface_id) {
                        let rect = self
                            .workspace
                            .compute_rects(total)
                            .get(&surface_id)
                            .copied()
                            .unwrap_or(Rect {
                                x: 0,
                                y: 0,
                                w: 0,
                                h: 0,
                            });
                        let ctx = EventContext {
                            state,
                            rect,
                            focused: surface_id == self.workspace.focused(),
                        };
                        let commands = entry.surface.handle_event(event, &ctx);
                        if commands.is_empty() {
                            vec![]
                        } else {
                            vec![SourcedSurfaceCommands {
                                source_plugin: entry.owner_plugin.clone(),
                                commands,
                            }]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            SurfaceEvent::Mouse(mouse_event) => {
                // Route to surface under cursor
                let target = self.workspace.surface_at(
                    mouse_event.column as u16,
                    mouse_event.line as u16,
                    total,
                );
                if let Some(surface_id) = target {
                    if let Some(entry) = self.surfaces.get_mut(&surface_id) {
                        let rect = self
                            .workspace
                            .compute_rects(total)
                            .get(&surface_id)
                            .copied()
                            .unwrap_or(Rect {
                                x: 0,
                                y: 0,
                                w: 0,
                                h: 0,
                            });
                        let ctx = EventContext {
                            state,
                            rect,
                            focused: surface_id == self.workspace.focused(),
                        };
                        let commands = entry.surface.handle_event(event, &ctx);
                        if commands.is_empty() {
                            vec![]
                        } else {
                            vec![SourcedSurfaceCommands {
                                source_plugin: entry.owner_plugin.clone(),
                                commands,
                            }]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            SurfaceEvent::Resize(_) => {
                let rects = self.workspace.compute_rects(total);
                let focused = self.workspace.focused();
                let mut results = Vec::new();
                for (surface_id, rect) in rects {
                    if let Some(entry) = self.surfaces.get_mut(&surface_id) {
                        let ctx = EventContext {
                            state,
                            rect,
                            focused: surface_id == focused,
                        };
                        let commands = entry.surface.handle_event(SurfaceEvent::Resize(rect), &ctx);
                        if !commands.is_empty() {
                            results.push(SourcedSurfaceCommands {
                                source_plugin: entry.owner_plugin.clone(),
                                commands,
                            });
                        }
                    }
                }
                results
            }
        }
    }
}
