# Plugin Cookbook

Curated patterns for common Kasane plugin tasks.
Each recipe is a complete, working snippet that you can copy into a `define_plugin!` block.

For API details, see [plugin-api.md](./plugin-api.md).
For full examples, see `examples/wasm/` in the repository.

## Status Badge

Show a count in the status bar, conditionally styled.

```rust
kasane_plugin_sdk::define_plugin! {
    id: "sel_badge",

    state {
        #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
        cursor_count: u32 = 0,
    },

    slots {
        STATUS_RIGHT(dirty::BUFFER) => |_ctx| {
            status_badge(state.cursor_count > 1, &format!(" {} sel ", state.cursor_count))
        },
    },
}
```

Key points:
- `#[bind(expr, on: flags)]` auto-syncs state from host on matching dirty flags
- `status_badge(condition, text)` returns `Some(contribution)` when the condition is true, `None` otherwise
- Simple slot form `SLOT(deps) => |ctx| { ... }` auto-wraps the return in `auto_contribution()`

See: `examples/wasm/sel-badge/`

## Line Highlighter (Annotation)

Highlight the current cursor line with a background color.

```rust
kasane_plugin_sdk::define_plugin! {
    id: "cursor_line",

    state {
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        cursor_line: i32 = -1,
    },

    annotate(line, _ctx) {
        if line as i32 == state.cursor_line {
            let bg = if host_state::is_dark_background() {
                rgb(40, 40, 50)
            } else {
                rgb(230, 230, 240)
            };
            Some(LineAnnotation {
                background: Some(BackgroundLayer {
                    face: face_bg(bg),
                    z_order: 0,
                    blend_opaque: false,
                }),
                ..Default::default()
            })
        } else {
            None
        }
    },
}
```

Key points:
- `annotate(line, ctx)` is called per visible line
- Return `Some(LineAnnotation)` to decorate, `None` to skip
- Use `host_state::is_dark_background()` for theme-adaptive colors

See: `examples/wasm/cursor-line/`

## Overlay Dialog

Show a floating dialog with keyboard interaction.

```rust
kasane_plugin_sdk::define_plugin! {
    id: "my_dialog",

    state {
        visible: bool = false,
        query: String = String::new(),
    },

    impl {
        fn build_overlay(&self) -> Option<OverlayContribution> {
            if !self.visible { return None; }

            let content = text(&self.query, default_face());
            let panel = container().child(content).border_rounded().build();
            let anchor = OverlayAnchor::Absolute(AbsoluteAnchor {
                x: 10, y: 5, w: 40, h: 10,
            });

            Some(OverlayContribution {
                element: panel,
                anchor,
                z_index: 100,
            })
        }
    },

    handle_key(event) {
        if !state.visible {
            if is_ctrl(event, 'p') {
                state.visible = true;
                return Some(vec![redraw()]);
            }
            return None;
        }

        match event.key {
            KeyCode::Escape => {
                state.visible = false;
                Some(vec![redraw()])
            }
            KeyCode::Char(ch) => {
                state.query.push(char::from_u32(ch).unwrap_or('?'));
                Some(vec![redraw()])
            }
            KeyCode::Backspace => {
                state.query.pop();
                Some(vec![redraw()])
            }
            _ => Some(vec![]),
        }
    },

    overlay(ctx) {
        state.build_overlay()
    },
}
```

Key points:
- Use `impl { ... }` inside `define_plugin!` for helper methods
- `handle_key` returns `Some(commands)` to consume, `None` to pass through
- `overlay(ctx)` is called every frame when the plugin has the `OVERLAY` capability
- Return `redraw()` to trigger a repaint after state changes

See: `examples/wasm/fuzzy-finder/`

## Process Spawner

Run an external command and process its output.

### Declarative (Native plugins via HandlerRegistry)

```rust
use kasane_core::plugin_prelude::*;

struct FileListPlugin;

impl Plugin for FileListPlugin {
    type State = FileListState;
    fn id(&self) -> PluginId { PluginId("file_list".into()) }
    fn register(&self, r: &mut HandlerRegistry<FileListState>) {
        r.on_process_task(
            "file_list",
            ProcessTaskSpec::new("fd", &["--type", "f"])
                .fallback(ProcessTaskSpec::new("find", &[".", "-type", "f"])),
            |state, result, _app| match result {
                ProcessTaskResult::Completed { stdout, .. } => {
                    let files: Vec<String> = String::from_utf8_lossy(stdout)
                        .lines()
                        .map(String::from)
                        .collect();
                    (FileListState { files, ..state.clone() }, Effects::none())
                }
                ProcessTaskResult::Failed(msg) => {
                    tracing::warn!("file_list failed: {msg}");
                    (state.clone(), Effects::none())
                }
                _ => (state.clone(), Effects::none()),
            },
        );
    }
}
```

Key points:
- `ProcessTaskSpec::new(program, args)` defines the command
- `.fallback(spec)` chains a fallback if the primary fails to spawn
- The framework manages job IDs, stdout buffering, and fallback state machines
- Start the task with `Command::StartProcessTask { task_name: "file_list".into() }`

### Manual (WASM plugins)

```rust
kasane_plugin_sdk::define_plugin! {
    id: "file_finder",
    capabilities: [Process],

    state {
        job_id: u64 = 1,
        files: Vec<String> = Vec::new(),
    },

    on_session_ready() {
        Effects {
            commands: vec![spawn_process(SpawnProcessConfig {
                job_id: state.job_id,
                program: "fd".into(),
                args: vec!["--type".into(), "f".into()],
                stdin_mode: StdinMode::NullStdin,
            })],
            ..Default::default()
        }
    },

    on_io_event(event) {
        let IoEvent::Process(pe) = event;
        if pe.job_id != state.job_id { return Effects::default(); }

        match pe.kind {
            ProcessEventKind::Stdout(data) => {
                if let Ok(text) = String::from_utf8(data) {
                    state.files.extend(text.lines().map(String::from));
                }
                Effects { redraw: dirty::ALL, ..Default::default() }
            }
            ProcessEventKind::Exited(_) => {
                Effects { redraw: dirty::ALL, ..Default::default() }
            }
            _ => Effects::default(),
        }
    },
}
```

Key points:
- Declare `capabilities: [Process]` to allow spawning
- Spawn in `on_session_ready()` (after Kakoune connection is established)
- Handle stdout/exit in `on_io_event()`

See: `examples/wasm/fuzzy-finder/`

## Declarative Transform (Native)

Use `ElementPatch` for Salsa-cacheable transforms.

```rust
use kasane_core::plugin_prelude::*;

struct MyTransformPlugin;

impl Plugin for MyTransformPlugin {
    type State = MyState;
    fn id(&self) -> PluginId { PluginId("my_transform".into()) }
    fn register(&self, r: &mut HandlerRegistry<MyState>) {
        r.on_transform(0, |_state, target, _app, _ctx| {
            match target.target_type() {
                TransformTargetType::StatusBar => {
                    ElementPatch::Append {
                        element: Element::text("extra", Face::default()),
                    }
                }
                _ => ElementPatch::Identity,
            }
        });
    }
}
```

Key points:
- `ElementPatch` forms a monoid: `Identity`, `Prepend`, `Append`, `Replace`, `WrapContainer`, `ModifyFace`, `Compose`
- Pure patches (no `Custom` or `ModifyAnchor`) are Salsa-memoizable
- WASM plugins can also return `list<element-patch-op>` from `transform-patch` for declarative transforms

## Declarative Transform (WASM)

```rust
kasane_plugin_sdk::define_plugin! {
    id: "status_prefix",

    // Return a declarative patch instead of an imperative transform.
    // The host caches this via Salsa when the patch is pure.
    transform_patch(target, ctx) {
        match target {
            TransformTarget::StatusBarT => {
                vec![ElementPatchOp::Prepend(text("[K] ", default_face()))]
            }
            _ => vec![],  // empty = no patch, fall back to imperative
        }
    },
}
```

## Inter-Plugin Communication (Native)

Use typed pub/sub topics for compile-time safe messaging.

```rust
use kasane_core::plugin_prelude::*;

// Publisher plugin
struct CursorPublisher;
impl Plugin for CursorPublisher {
    type State = CursorPubState;
    fn id(&self) -> PluginId { PluginId("cursor_pub".into()) }
    fn register(&self, r: &mut HandlerRegistry<CursorPubState>) {
        let topic: Topic<u32> = r.publish_typed("cursor.line", |state, _app| state.line);
        // `topic` is phantom-typed — subscribers get compile-time type checking
    }
}

// Subscriber plugin
struct CursorConsumer;
impl Plugin for CursorConsumer {
    type State = ConsumerState;
    fn id(&self) -> PluginId { PluginId("cursor_consumer".into()) }
    fn register(&self, r: &mut HandlerRegistry<ConsumerState>) {
        // Type mismatch here would be a compile error
        r.subscribe_typed::<u32>("cursor.line", |state, value| {
            ConsumerState { last_line: *value, ..state.clone() }
        });
    }
}
```

Key points:
- `publish_typed<T>()` returns a `Topic<T>` phantom handle
- `subscribe_typed<T>()` enforces type safety at compile time
- Untyped `publish()`/`subscribe()` remain for WASM cross-boundary use

## Cell Decoration

Apply per-cell face styling (highlights, markers).

```rust
kasane_plugin_sdk::define_plugin! {
    id: "column_marker",

    state {
        #[bind(host_state::get_widget_columns(), on: dirty::BUFFER)]
        cols: u16 = 80,
    },

    decorate_cells() {
        vec![CellDecoration {
            target: DecorationTarget::Column(80),
            face: Face {
                bg: Color::Rgb(RgbColor { r: 40, g: 40, b: 40 }),
                ..Default::default()
            },
            merge: 1, // Overlay
            priority: 0,
        }]
    },
}
```

## Display Directives (Code Folding)

Hide or fold ranges of buffer lines.

```rust
kasane_plugin_sdk::define_plugin! {
    id: "fold_imports",

    state {
        fold_start: Option<u32> = None,
        fold_end: Option<u32> = None,
    },

    display_directives() {
        match (state.fold_start, state.fold_end) {
            (Some(start), Some(end)) => vec![
                DisplayDirective::Fold(FoldDirective {
                    range_start: start,
                    range_end: end,
                    summary: vec![Atom {
                        face: face_fg(rgb(128, 128, 128)),
                        contents: format!("... ({} lines folded)", end - start + 1),
                    }],
                }),
            ],
            _ => vec![],
        }
    },
}
```

## Testing with TestHarness

Unit-test plugin logic without the full WASM runtime.
Enable the `test-harness` feature in your plugin's `Cargo.toml`:

```toml
[dev-dependencies]
kasane-plugin-sdk = { version = "0.3", features = ["test-harness"] }
```

```rust
#[cfg(test)]
mod tests {
    use kasane_plugin_sdk::test::*;

    #[test]
    fn test_cursor_state() {
        let mut h = TestHarness::new();
        h.set_cursor_line(42);
        h.set_selection_count(3);

        // Call your plugin's state sync logic
        assert_eq!(mock_host_state::get_cursor_line(), 42);
        assert_eq!(mock_host_state::get_selection_count(), 3);

        // Test element creation
        let handle = mock_element_builder::create_text("hello", "default");
        let arena = h.arena();
        assert_eq!(arena.len(), 1);
        assert!(arena.get(handle).unwrap().contains("hello"));
    }
}
```

Key points:
- `TestHarness::new()` resets thread-local mock state
- Set host state with `h.set_*()` methods
- Use `mock_host_state::*` to verify what the plugin would see
- Inspect created elements via `h.arena()`
- Check log output with `h.drain_logs()`
- Tests using the harness share thread-local state; use serial execution if needed
