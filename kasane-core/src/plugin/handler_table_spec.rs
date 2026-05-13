//! Macro-driven `HandlerTable` spec module (γ-3.2.3 scaffolding).
//!
//! γ-3.2.3a lands the parallel-implementation infrastructure: this file
//! invokes [`kasane_macros::handler_table!`] to generate a parallel
//! `HandlerTable` / `HandlerRegistry` / `EXPECTED_HANDLER_NAMES` from a
//! declarative spec module. γ-3.2.3b … z incrementally migrate the 70
//! canonical handler entries (one per logical extension point) into
//! the spec; γ-3.3 then deletes the manual implementations once
//! [`assert_generated_names_subset`] reaches full parity with
//! `plugin_bridge::tests::EXPECTED_HANDLER_NAMES`.
//!
//! This bootstrap landing covers three representative entries — one
//! Lifecycle, one Observer, one Dispatcher — to prove the macro
//! integrates with `kasane-core` (no cyclic-build problem; the macro
//! crate is already a regular `[dependencies]` member). Adding more
//! entries is a mechanical extension of the `handler_table!{}` body.
//!
//! See `docs/handler-table-dsl.md` for the spec; `docs/roadmap.md`
//! Phase γ-3.2.3 row tracks migration progress.

use kasane_macros::handler_table;

handler_table! {
    pub mod generated {
        // The macro emits `pub(crate) struct HandlerTable`,
        // `pub(crate) struct HandlerRegistry<S>`, and
        // `pub(crate) const EXPECTED_HANDLER_NAMES: &[&str]` inside this
        // module. All three are `pub(crate)` so the parallel-impl test
        // below can access them, but external users continue to consume
        // the manual `crate::plugin::HandlerTable` / `HandlerRegistry`
        // surface until γ-3.3 deletes the manual side.
        //
        // Migration progress: γ-3.2.3b-1 (Lifecycle 17) ✅,
        // γ-3.2.3b-2 (Observer 7 + Dispatcher 5) ✅,
        // γ-3.2.3b-3 (View 28) ✅, γ-3.2.3b-4 (Config 12) pending.
        use crate::display::content_annotation::ContentAnnotation;
        use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
        use crate::display::unit::DisplayUnit;
        use crate::element::{Element, InteractiveId, Overlay};
        use crate::input::{DropEvent, KeyEvent, MouseEvent};
        use crate::layout::Rect;
        use crate::plugin::algebra::element_patch::ElementPatch;
        use crate::plugin::effect::error_attribution::PluginErrorEvent;
        use crate::plugin::pubsub::TopicId;
        use crate::plugin::{
            AnnotateContext, AppView, BackgroundLayer, ChannelValue, Command, ContributeContext,
            Contribution, DisplayDirective, Effects, GutterSide, IoEvent, KakouneSideEffects,
            KeyHandleResult, KeyPreDispatchResult, KeyResponse, LineAnnotation,
            MousePreDispatchResult, OrnamentBatch, OverlayContext, OverlayContribution, PluginState,
            PluginView, ProcessCapableEffects, RecoveryWitness, RenderOrnamentContext,
            SafeDisplayDirective, SlotId, TextInputPreDispatchResult, TransformContext,
            TransformTarget, Transparency, VirtualEditContext, VirtualTextItem,
        };
        use crate::protocol::Atom;
        use crate::render::InlineDecoration;
        use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
        use crate::state::DirtyFlags;
        use crate::state::shadow_cursor::{BufferEdit, BufferEditVerdict};
        use crate::workspace::WorkspaceQuery;

        // -------------------- Lifecycle (17 entries) --------------------

        handler init(_app: &AppView<'_>): Lifecycle<Effects>(tier1, transparent);
        handler session_ready(_app: &AppView<'_>): Lifecycle<Effects>(tier1, transparent);
        handler state_changed(_app: &AppView<'_>, _dirty: DirtyFlags):
            Lifecycle<Effects>(tier1, transparent);
        handler io_event(_event: &IoEvent, _app: &AppView<'_>):
            Lifecycle<Effects>(tier2, transparent);
        handler update(_msg: &mut dyn ::std::any::Any, _app: &AppView<'_>):
            Lifecycle<Effects>(tier2, transparent);
        handler command_error(_event: &PluginErrorEvent, _app: &AppView<'_>):
            Lifecycle<Effects>(transparent);
        handler subscription(_topic: &str, _values: &[ChannelValue], _app: &AppView<'_>):
            Lifecycle<Effects>(transparent);
        handler key_middleware(_key: &KeyEvent, _app: &AppView<'_>):
            Lifecycle<KeyHandleResult>(transparent);
        handler key_pre_dispatch(_key: &KeyEvent, _app: &AppView<'_>):
            Lifecycle<KeyPreDispatchResult>;
        handler mouse_pre_dispatch(_event: &MouseEvent, _app: &AppView<'_>):
            Lifecycle<MousePreDispatchResult>;
        handler text_input_pre_dispatch(_text: &str, _app: &AppView<'_>):
            Lifecycle<TextInputPreDispatchResult>;
        handler mouse_fallback(_event: &MouseEvent, _scroll: i32, _app: &AppView<'_>):
            Lifecycle<::core::option::Option<::std::vec::Vec<Command>>>;
        handler action(_id: &str, _key: &KeyEvent, _app: &AppView<'_>):
            Lifecycle<KeyResponse>;
        handler navigation_action(_unit: &DisplayUnit, _action: NavigationAction):
            Lifecycle<ActionResult>;
        handler virtual_edit(_ctx: &VirtualEditContext, _app: &AppView<'_>):
            Lifecycle<::std::vec::Vec<Command>>;
        handler buffer_edit_intercept(_edit: &BufferEdit, _app: &AppView<'_>):
            Lifecycle<BufferEditVerdict>;

        // `process_task` is a documented carve-out (spec §9.5). Its
        // registration shape `(name, ProcessTaskSpec, handler) → Vec<
        // ProcessTaskEntry { name, spec, handler, streaming, transparent }>`
        // is a Vec-of-metadata-Lifecycle storage that does not generalize
        // through the macro's per_slot or has_metadata_storage paths.
        // Hand-authored in `handler_registry/lifecycle.rs` (the
        // `on_process_task_tier2` / `on_process_task_streaming_tier2`
        // setters) alongside the macro-generated members.

        // -------------------- Observer (8 entries) --------------------

        handler workspace_changed(_query: &WorkspaceQuery<'_>): Observer;
        handler workspace_restore(_data: &::serde_json::Value): Observer;
        handler observe_key(_event: &KeyEvent, _app: &AppView<'_>): Observer;
        handler observe_text_input(_text: &str, _app: &AppView<'_>): Observer;
        handler observe_mouse(_event: &MouseEvent, _app: &AppView<'_>): Observer;
        handler observe_drop(_event: &DropEvent, _app: &AppView<'_>): Observer;
        handler shutdown(): Observer(void);
        handler subscribe(_value: &ChannelValue): Observer(per_slot = TopicId);

        // -------------------- Dispatcher (5 entries) --------------------

        handler key(_event: &KeyEvent, _app: &AppView<'_>):
            Dispatcher<::std::vec::Vec<Command>>(transparent);
        handler text_input(_text: &str, _app: &AppView<'_>):
            Dispatcher<::std::vec::Vec<Command>>(transparent);
        handler handle_mouse(_event: &MouseEvent, _id: InteractiveId, _app: &AppView<'_>):
            Dispatcher<::std::vec::Vec<Command>>(transparent);
        handler handle_drop(_event: &DropEvent, _id: InteractiveId, _app: &AppView<'_>):
            Dispatcher<::std::vec::Vec<Command>>(transparent);
        handler default_scroll(_candidate: DefaultScrollCandidate, _app: &AppView<'_>):
            Dispatcher<ScrollPolicyResult>;

        // -------------------- View (28 entries) --------------------

        handler contribute(_app: &AppView<'_>, _ctx: &ContributeContext):
            View<::core::option::Option<Contribution>>(per_slot = SlotId);
        handler contribute_any(_slot: &SlotId, _app: &AppView<'_>, _ctx: &ContributeContext):
            View<::core::option::Option<Contribution>>;
        handler transform(
            _target: &TransformTarget,
            _app: &AppView<'_>,
            _ctx: &TransformContext,
        ): View<ElementPatch>(prioritized, targets = ::std::vec::Vec<TransformTarget>);
        handler gutter(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<::core::option::Option<Element>>(per_slot = GutterSide, prioritized);
        handler background(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<::core::option::Option<BackgroundLayer>>;
        handler inline(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<::core::option::Option<InlineDecoration>>;
        handler virtual_text(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<::std::vec::Vec<VirtualTextItem>>(default = ::std::vec::Vec::new());
        handler annotate_line(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<::core::option::Option<LineAnnotation>>(
                unified,
                suppresses = [gutter, background, inline, virtual_text],
            );
        handler overlay(_app: &AppView<'_>, _ctx: &OverlayContext):
            View<::core::option::Option<OverlayContribution>>;
        handler display(_app: &AppView<'_>): View<::std::vec::Vec<DisplayDirective>>(recovery);
        handler unified_display(_app: &AppView<'_>):
            View<::std::vec::Vec<DisplayDirective>>(unified, recovery, suppresses = [display]);
        handler content_annotation(_app: &AppView<'_>, _ctx: &AnnotateContext):
            View<::std::vec::Vec<ContentAnnotation>>(default = ::std::vec::Vec::new());
        handler render_ornament(_app: &AppView<'_>, _ctx: &RenderOrnamentContext):
            View<OrnamentBatch>(default = OrnamentBatch::default());
        handler menu_transform(_atoms: &[Atom], _row: usize, _is_prompt: bool, _app: &AppView<'_>):
            View<::core::option::Option<::std::vec::Vec<Atom>>>;
        handler menu_renderer(_app: &AppView<'_>, _view: &PluginView<'_>):
            View<::core::option::Option<Overlay>>;
        handler info_renderer(
            _app: &AppView<'_>,
            _rects: &[Rect],
            _view: &PluginView<'_>,
        ): View<::core::option::Option<::std::vec::Vec<Overlay>>>;
        handler display_scroll_offset(
            _cursor_y: usize,
            _viewport_h: usize,
            _default_off: usize,
            _app: &AppView<'_>,
        ): View<::core::option::Option<usize>>;
        handler navigation_policy(_unit: &DisplayUnit):
            View<NavigationPolicy>(default = NavigationPolicy::Normal);
        handler paint_inline_box(_box_id: u64, _app: &AppView<'_>):
            View<::core::option::Option<Element>>;
        handler workspace_save(): View<::core::option::Option<::serde_json::Value>>;
        handler persist_state(): View<::core::option::Option<::std::vec::Vec<u8>>>;
        handler restore_state(_bytes: &[u8]): View<bool>;
        handler key_map_builder(): View<crate::input::CompiledKeyMap>;
        handler group_refresh(_app: &AppView<'_>, _map: &mut crate::input::CompiledKeyMap):
            View<()>;
        handler surfaces():
            View<::std::vec::Vec<::std::boxed::Box<dyn crate::surface::Surface>>>(
                default = ::std::vec::Vec::new(),
            );
        handler lenses():
            View<::std::vec::Vec<::std::sync::Arc<dyn crate::lens::Lens>>>(
                stateless,
                default = ::std::vec::Vec::new(),
            );
        handler publish(_app: &AppView<'_>):
            View<::core::option::Option<ChannelValue>>(per_slot = TopicId);
        handler projection(_app: &AppView<'_>):
            View<::std::vec::Vec<DisplayDirective>>(
                per_slot = crate::display::projection::ProjectionDescriptor,
                recovery,
            );

        // -------------------- Config (10 entries) --------------------
        //
        // Configuration metadata declared via `declare_*` setters on
        // HandlerRegistry. Do not appear in EXPECTED_HANDLER_NAMES (no
        // dispatch site). The `transparency` and `recovery` config
        // sub-entries from spec §8.6 are auto-generated by the macro
        // from `transparent` / `recovery` modifiers — not declared here.

        config interests: DirtyFlags = DirtyFlags::ALL;
        config authorities: crate::plugin::PluginAuthorities =
            crate::plugin::PluginAuthorities::empty();
        config allows_process_spawn: bool = true;
        config display_priority: i16 = 0;
        config workspace_request: ::core::option::Option<crate::workspace::Placement>;
        config capabilities_override:
            ::core::option::Option<crate::plugin::PluginCapabilities>;
        config capability_descriptor_override:
            ::core::option::Option<crate::plugin::CapabilityDescriptor>;
        config state_hash:
            ::core::option::Option<
                ::std::boxed::Box<dyn ::core::ops::Fn() -> u64 + ::core::marker::Send + ::core::marker::Sync>,
            >;
        config suppressed_builtins:
            ::std::collections::HashSet<crate::plugin::BuiltinTarget>;
        config key_map: ::core::option::Option<crate::input::CompiledKeyMap>;
    }
}

#[cfg(test)]
mod tests {
    use super::generated;

    /// γ-3.2.3 parallel-impl gate (manual ⊆ generated direction).
    ///
    /// Asserts every name in the manual `EXPECTED_HANDLER_NAMES`
    /// (the 36-entry const inside
    /// `plugin_bridge::tests::exhaustive_handler_dispatch_coverage`)
    /// has a corresponding spec entry. This is the direction that
    /// matters during incremental migration — it catches the case
    /// where a handler exists in the manual implementation but the
    /// spec module forgets to declare it.
    ///
    /// The reverse direction (generated ⊆ manual) is intentionally NOT
    /// asserted: the spec module deliberately includes entries that
    /// the manual test under-covers (`action`, `navigation_action`,
    /// `virtual_edit` were noted as missing in the γ-3.1 spec survey
    /// §5). γ-3.3 brings the manual side up to parity by deleting it
    /// entirely. `process_task` was originally on this list but moved
    /// to the carve-out (spec §9.5) in γ-3.3b-4 — its registration
    /// shape does not generalize through the macro's storage variants.
    ///
    /// The manual names are reproduced inline rather than imported
    /// because the manual list lives inside a `#[test] fn` body and is
    /// not module-visible. γ-3.3 deletes the manual list and this
    /// duplication disappears with it.
    #[test]
    fn manual_dispatch_coverage_is_subset_of_generated() {
        // Reproduced from `plugin_bridge.rs` `EXPECTED_HANDLER_NAMES`.
        // Update both lists in lock-step until γ-3.3 retires the manual
        // side (the duplication is intentional — the test catches drift).
        const MANUAL: &[&str] = &[
            "init",
            "session_ready",
            "state_changed",
            "io_event",
            "workspace_changed",
            "shutdown",
            "update",
            "key",
            "key_pre_dispatch",
            "key_middleware",
            "observe_key",
            "text_input",
            "text_input_pre_dispatch",
            "observe_text_input",
            "observe_mouse",
            "handle_mouse",
            "mouse_pre_dispatch",
            "mouse_fallback",
            "default_scroll",
            "contribute",
            "contribute_any",
            "transform",
            "gutter",
            "background",
            "inline",
            "virtual_text",
            "annotate_line",
            "overlay",
            "display",
            "menu_transform",
            "publish",
            "subscribe",
            "subscription",
            "command_error",
            "paint_inline_box",
            "buffer_edit_intercept",
        ];

        for name in MANUAL {
            assert!(
                generated::EXPECTED_HANDLER_NAMES.contains(name),
                "manual handler name `{name}` (from `plugin_bridge.rs` EXPECTED_HANDLER_NAMES) \
                 has no corresponding entry in the generated spec module. Add a `handler {name}(…): …;` \
                 line to `handler_table_spec.rs`."
            );
        }
    }

    /// Sanity check: the generated registry can build an empty table.
    #[test]
    fn empty_table_compiles() {
        let table = generated::HandlerTable::empty();
        assert!(table.init_handler.is_none());
        assert!(table.observe_key_handler.is_none());
        assert!(table.shutdown_handler.is_none());
        // 16 Lifecycle (γ-3.2.3b-1, less `process_task` retired to
        // carve-out in γ-3.3b-4) + 8 Observer (γ-3.2.3b-2)
        // + 5 Dispatcher (γ-3.2.3b-2) + 28 View (γ-3.2.3b-3) = 57.
        // Config entries (γ-3.2.3b-4) do not appear in EXPECTED_HANDLER_NAMES.
        assert_eq!(generated::EXPECTED_HANDLER_NAMES.len(), 57);
    }

    /// γ-3.2.3b-1: tier-narrowed setters round-trip transparency.
    /// Confirms `transparent` modifier on Lifecycle fires `IS_TRANSPARENT`
    /// detection at registration. Uses `command_error` (transparent
    /// without tier) since its base setter is the one with the
    /// `Transparency` bound.
    #[test]
    fn transparent_lifecycle_sets_flag_on_registration() {
        use crate::plugin::KakouneTransparentEffects;

        let mut registry: generated::HandlerRegistry<u32> = generated::HandlerRegistry::new();
        registry
            .on_command_error(|state, _event, _app| (*state, KakouneTransparentEffects::default()));
        let table = registry.into_table();
        assert!(table.transparency.command_error);
    }

    /// γ-3.3c-3: `per_slot + recovery` entries (currently only
    /// `projection`) carry recovery on the per-entry struct rather than
    /// at table level. The macro now generates `ProjectionEntry { key,
    /// handler, recovery }` matching the manual side, and the generated
    /// `on_projection` setter takes a `recovery: DisplayRecoveryStatus`
    /// argument. The previously-orphaned table-level `projection_recovery`
    /// field is no longer emitted — `define_projection` (carve-out spec
    /// §9.1) wraps the generated `on_projection` and derives recovery
    /// from the descriptor's `Structural` / `Additive` category.
    ///
    /// This test confirms (a) the generated `ProjectionEntry` accepts a
    /// `recovery` field at construction, and (b) the generated
    /// `on_projection` setter accepts a recovery argument and pushes
    /// the resulting entry into `projection_handlers`.
    #[test]
    fn projection_recovery_lives_per_entry() {
        use crate::display::projection::{ProjectionCategory, ProjectionDescriptor, ProjectionId};

        let descriptor = ProjectionDescriptor {
            id: ProjectionId::new("test.projection"),
            name: "Test Projection".to_string(),
            category: ProjectionCategory::Structural,
            priority: 0,
        };
        let mut registry: generated::HandlerRegistry<()> = generated::HandlerRegistry::new();
        registry.on_projection(
            descriptor.clone(),
            generated::DisplayRecoveryStatus::Witnessed(crate::plugin::RecoveryWitness {
                mechanism: crate::plugin::RecoveryMechanism::Declared {
                    description: "test",
                },
            }),
            |_state, _app| Vec::new(),
        );
        let table = registry.into_table();
        assert_eq!(table.projection_handlers.len(), 1);
        // Per-entry recovery is reachable on the entry struct.
        assert!(matches!(
            table.projection_handlers[0].recovery,
            generated::DisplayRecoveryStatus::Witnessed(_),
        ));
    }

    /// γ-3.3b-3: per-handler-name recovery fields agree across manual
    /// and generated tables.
    ///
    /// Manual `recovery: RecoveryFlags { display }` was a single shared
    /// slot that the unified setter accidentally clobbered. The macro
    /// emits one `<name>_recovery: DisplayRecoveryStatus` field per
    /// `recovery`-marked entry; γ-3.3b-3 split the manual side to match
    /// the singleton entries (`display`, `unified_display`) and left
    /// `projection` per-entry on `ProjectionEntry` (carve-out, spec §9.1).
    /// The `DisplayRecoveryStatus` enum itself is `pub(crate)` and the
    /// macro emits a structural mirror inside the spec module — the
    /// generated mirror also carries the same `Default::default() ==
    /// NotRegistered` invariant; `NotRegistered`-tagged status from a
    /// fresh table is the canonical proof of structural agreement.
    #[test]
    fn recovery_fields_default_to_not_registered() {
        let manual_table = crate::plugin::handler_table::HandlerTable::empty();
        assert!(matches!(
            manual_table.display_recovery,
            crate::plugin::handler_table::DisplayRecoveryStatus::NotRegistered
        ));
        assert!(matches!(
            manual_table.unified_display_recovery,
            crate::plugin::handler_table::DisplayRecoveryStatus::NotRegistered
        ));

        let generated_table = generated::HandlerTable::empty();
        assert!(matches!(
            generated_table.display_recovery,
            generated::DisplayRecoveryStatus::NotRegistered
        ));
        assert!(matches!(
            generated_table.unified_display_recovery,
            generated::DisplayRecoveryStatus::NotRegistered
        ));
    }

    /// γ-3.2.3b-4: config entries thread their declared defaults through
    /// `HandlerTable::empty()`. The macro generates a `<name>: <expr>`
    /// initializer for every `config <name>: <T> = <expr>;` line; entries
    /// without an explicit default fall through to `Default::default()`.
    #[test]
    fn config_defaults_match_spec() {
        use crate::plugin::PluginAuthorities;
        use crate::state::DirtyFlags;

        let table = generated::HandlerTable::empty();
        assert_eq!(table.interests, DirtyFlags::ALL);
        assert_eq!(table.authorities, PluginAuthorities::empty());
        assert!(table.allows_process_spawn);
        assert_eq!(table.display_priority, 0);
        assert!(table.workspace_request.is_none());
        assert!(table.capabilities_override.is_none());
        assert!(table.capability_descriptor_override.is_none());
        assert!(table.state_hash.is_none());
        assert!(table.suppressed_builtins.is_empty());
        assert!(table.key_map.is_none());
    }
}
