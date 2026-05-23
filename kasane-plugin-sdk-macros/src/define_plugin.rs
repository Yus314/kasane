use proc_macro2::{Group, TokenStream, TokenTree};
use quote::quote;
use syn::ImplItemFn;

use crate::key_map::{
    ActionDef, KeyMapDef, generate_invoke_action, generate_is_group_active,
    generate_key_map_declare, parse_actions_def, parse_key_map_def,
};
use crate::manifest::{ManifestDef, parse_manifest_at_compile_time};
use crate::sdk_helpers::generate_sdk_helpers;

// ---------------------------------------------------------------------------
// define_plugin! DSL type definitions
// ---------------------------------------------------------------------------

pub(crate) struct PluginDef {
    manifest: Option<ManifestDef>,
    id: syn::LitStr,
    state: Option<StateDef>,
    on_init_effects: Option<proc_macro2::TokenStream>,
    on_active_session_ready_effects: Option<proc_macro2::TokenStream>,
    on_state_changed_effects: Option<OnStateChanged>,
    on_workspace_changed: Option<ParamBodyDef>,
    update_effects: Option<ParamBodyDef>,
    slots: Option<Vec<SlotEntry>>,
    annotate: Option<AnnotateDef>,
    display_directives: Option<proc_macro2::TokenStream>,
    display: Option<proc_macro2::TokenStream>,
    transform: Option<TransformDef>,
    transform_patch: Option<TransformPatchDef>,
    transform_priority: Option<proc_macro2::TokenStream>,
    overlay: Option<ParamBodyDef>,
    handle_key: Option<ParamBodyDef>,
    handle_key_middleware: Option<ParamBodyDef>,
    handle_mouse: Option<HandleMouseDef>,
    handle_drop: Option<HandleDropDef>,
    handle_default_scroll: Option<ParamBodyDef>,
    paint_inline_box: Option<ParamBodyDef>,
    capabilities: Option<proc_macro2::TokenStream>,
    authorities: Option<proc_macro2::TokenStream>,
    on_io_event_effects: Option<ParamBodyDef>,
    view_deps: Option<proc_macro2::TokenStream>,
    key_map: Option<KeyMapDef>,
    actions: Option<Vec<ActionDef>>,
    settings: Option<Vec<SettingFieldDef>>,
    impl_block: Option<Vec<ImplItemFn>>,
    effects_blocks: Option<Vec<EffectsBlock>>,
}

/// ADR-053: a single `effects on <Trigger>(...) { ... }` section.
struct EffectsBlock {
    trigger: EffectsTrigger,
    /// Parameter name(s) the user wrote inside `Trigger(...)`. Bound as
    /// locals at the top of the lowered handler body.
    params: Vec<syn::Ident>,
    /// Raw body tokens; the CPS lowerer rewrites `yield Effect::X(...)`
    /// to `__yielder.emit(Effect::X(...))?` before the body is spliced
    /// into the generated Guest method.
    body: TokenStream,
}

/// Triggers an `effects on ...` block can name. Chunk 2 of ADR-053 ships
/// the three tier-1 handlers; key / mouse / IO triggers follow when the
/// migration target requires them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectsTrigger {
    /// `effects on StateChanged(flags) { ... }` → `on_state_changed_effects`.
    StateChanged,
    /// `effects on Init() { ... }` → `on_init_effects`.
    Init,
    /// `effects on SessionReady() { ... }` → `on_active_session_ready_effects`.
    SessionReady,
}

impl EffectsTrigger {
    fn from_ident(ident: &syn::Ident) -> syn::Result<Self> {
        match ident.to_string().as_str() {
            "StateChanged" => Ok(Self::StateChanged),
            "Init" => Ok(Self::Init),
            "SessionReady" => Ok(Self::SessionReady),
            other => Err(syn::Error::new(
                ident.span(),
                format!(
                    "unknown trigger `{other}` in `effects on ...`; \
                     supported triggers are StateChanged, Init, SessionReady"
                ),
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::StateChanged => "StateChanged",
            Self::Init => "Init",
            Self::SessionReady => "SessionReady",
        }
    }

    fn expected_arity(self) -> usize {
        match self {
            Self::StateChanged => 1,
            Self::Init | Self::SessionReady => 0,
        }
    }
}

struct TransformPatchDef {
    target_param: syn::Ident,
    ctx_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct StateDef {
    fields: Vec<StateField>,
}

struct StateField {
    name: syn::Ident,
    ty: syn::Type,
    default: syn::Expr,
    bind: Option<BindDef>,
    persist: bool,
}

struct BindDef {
    expr: proc_macro2::TokenStream,
    dirty_flag: proc_macro2::TokenStream,
}

enum SlotName {
    WellKnown(syn::Ident),
    Named(syn::LitStr),
}

struct SlotEntry {
    name: SlotName,
    has_closure: bool,
    ctx_param: Option<syn::Ident>,
    body: proc_macro2::TokenStream,
}

struct OnStateChanged {
    param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct AnnotateDef {
    line_param: syn::Ident,
    ctx_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct TransformDef {
    target_param: syn::Ident,
    element_param: syn::Ident,
    ctx_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct ParamBodyDef {
    param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct HandleMouseDef {
    event_param: syn::Ident,
    id_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

struct HandleDropDef {
    event_param: syn::Ident,
    id_param: syn::Ident,
    body: proc_macro2::TokenStream,
}

/// Supported setting types in the `settings { ... }` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingType {
    Bool,
    Integer,
    Float,
    Str,
}

impl SettingType {
    fn from_ident(ident: &syn::Ident) -> syn::Result<Self> {
        match ident.to_string().as_str() {
            "bool" => Ok(Self::Bool),
            "i64" => Ok(Self::Integer),
            "f64" => Ok(Self::Float),
            "String" => Ok(Self::Str),
            other => Err(syn::Error::new(
                ident.span(),
                format!("unsupported setting type `{other}`; expected bool, i64, f64, or String"),
            )),
        }
    }

    /// The manifest `[settings.<key>].type` string this maps to.
    fn manifest_type_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Str => "string",
        }
    }
}

struct SettingFieldDef {
    name: syn::Ident,
    ty: SettingType,
    default: syn::Expr,
}

// ---------------------------------------------------------------------------
// define_plugin! implementation
// ---------------------------------------------------------------------------

pub(crate) fn define_plugin_impl(
    input: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let def: PluginDef = syn::parse2(input)?;

    // 1. generate!()
    let wit_content = kasane_wit::WIT;
    let wit_bindings = quote! {
        wit_bindgen::generate!({
            world: "kasane-plugin",
            inline: #wit_content,
        });
    };
    let sdk_helpers = generate_sdk_helpers();

    // 2. State definition (if present)
    let user_impl_methods: Vec<_> = def
        .impl_block
        .as_ref()
        .map(|methods| methods.iter().collect())
        .unwrap_or_default();

    let state_tokens = if let Some(ref state_def) = def.state {
        let fields: Vec<_> = state_def
            .fields
            .iter()
            .map(|f| {
                let name = &f.name;
                let ty = &f.ty;
                quote! { #name: #ty }
            })
            .collect();
        let defaults: Vec<_> = state_def
            .fields
            .iter()
            .map(|f| {
                let name = &f.name;
                let default = &f.default;
                quote! { #name: #default }
            })
            .collect();
        quote! {
            #[derive(Debug)]
            struct __KasanePluginState {
                #( #fields, )*
                generation: u64,
            }

            impl Default for __KasanePluginState {
                fn default() -> Self {
                    Self {
                        #( #defaults, )*
                        generation: 0,
                    }
                }
            }

            impl __KasanePluginState {
                fn bump_generation(&mut self) {
                    self.generation = self.generation.wrapping_add(1);
                }

                #( #user_impl_methods )*
            }

            ::std::thread_local! {
                static STATE: ::std::cell::RefCell<__KasanePluginState> =
                    ::std::cell::RefCell::new(<__KasanePluginState>::default());
            }

            /// RAII guard that auto-bumps generation on drop if state was mutated
            /// but bump_generation() was not called manually.
            struct __KasaneStateMutGuard<'a> {
                inner: ::std::cell::RefMut<'a, __KasanePluginState>,
                old_generation: u64,
                mutated: bool,
            }

            impl ::std::ops::Deref for __KasaneStateMutGuard<'_> {
                type Target = __KasanePluginState;
                fn deref(&self) -> &Self::Target { &self.inner }
            }

            impl ::std::ops::DerefMut for __KasaneStateMutGuard<'_> {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    self.mutated = true;
                    &mut self.inner
                }
            }

            impl Drop for __KasaneStateMutGuard<'_> {
                fn drop(&mut self) {
                    if self.mutated && self.inner.generation == self.old_generation {
                        self.inner.generation = self.inner.generation.wrapping_add(1);
                    }
                }
            }

            #[doc(hidden)]
            #[allow(dead_code)]
            fn __kasane_auto_state_hash() -> u64 {
                STATE.with(|s| s.borrow().generation)
            }
        }
    } else {
        // No state: provide a minimal state_hash
        quote! {
            #[doc(hidden)]
            #[allow(dead_code)]
            fn __kasane_auto_state_hash() -> u64 { 0 }
        }
    };

    // 3. Build Guest methods
    let id_str = &def.id;
    let get_id = quote! {
        fn get_id() -> String {
            #id_str.to_string()
        }
    };

    let has_state = def.state.is_some();

    // Helper: wrap body with STATE.with + StateMutGuard if state is present (mutable access)
    let wrap_state = |body: &proc_macro2::TokenStream| -> proc_macro2::TokenStream {
        if has_state {
            quote! {
                STATE.with(|__s| {
                    let __old_gen = __s.borrow().generation;
                    let mut state = __KasaneStateMutGuard {
                        inner: __s.borrow_mut(),
                        old_generation: __old_gen,
                        mutated: false,
                    };
                    #body
                })
            }
        } else {
            body.clone()
        }
    };

    let wrap_state_shared = |body: &proc_macro2::TokenStream| -> proc_macro2::TokenStream {
        if has_state {
            quote! {
                STATE.with(|__s| {
                    let state = __s.borrow();
                    #body
                })
            }
        } else {
            body.clone()
        }
    };

    // ADR-053 chunk 2: pull `effects on <Trigger>` blocks out, partitioned
    // by trigger. Each block is lowered to its corresponding Guest method
    // below; the legacy handler block for the same trigger is mutually
    // exclusive (validated during parse).
    let effects_block_for = |trigger: EffectsTrigger| -> Option<&EffectsBlock> {
        def.effects_blocks
            .as_ref()?
            .iter()
            .find(|b| b.trigger == trigger)
    };

    let on_init_method = if let Some(block) = effects_block_for(EffectsTrigger::Init) {
        lower_effects_block(block, has_state)
    } else if let Some(ref body) = def.on_init_effects {
        let wrapped = wrap_state(body);
        // ADR-044: init is tier-1; the body must evaluate to
        // `KakouneSideEffects` so it projects cleanly to the narrower
        // `BootstrapEffects` (redraw-only) wire shape.
        quote! { fn on_init_effects() -> BootstrapEffects { let __effects: KakouneSideEffects = { #wrapped }; __effects.into() } }
    } else {
        quote! {}
    };

    let on_active_session_ready_method = if let Some(block) =
        effects_block_for(EffectsTrigger::SessionReady)
    {
        lower_effects_block(block, has_state)
    } else if let Some(ref body) = def.on_active_session_ready_effects {
        let wrapped = wrap_state(body);
        // ADR-044: session-ready is tier-1; the body must evaluate to
        // `KakouneSideEffects` so the host's `From<KakouneSideEffects>
        // for SessionReadyEffects` filter passes only the admissible
        // session-ready commands through.
        quote! {
            fn on_active_session_ready_effects() -> SessionReadyEffects { let __effects: KakouneSideEffects = { #wrapped }; __effects.into() }
        }
    } else {
        quote! {}
    };

    let state_changed_effects_block = effects_block_for(EffectsTrigger::StateChanged);
    let has_osc = def.on_state_changed_effects.is_some() || state_changed_effects_block.is_some();

    // Determine the dirty-flags parameter name. Priority:
    // 1. `effects on StateChanged(<param>)` — ADR-053 chunk 2 form.
    // 2. `on_state_changed_effects(<param>) { ... }` — legacy form.
    // 3. Auto-bindings-only emission picks `__flags`.
    let osc_param_name = if let Some(block) = state_changed_effects_block {
        block.params[0].clone()
    } else if let Some(osc) = def.on_state_changed_effects.as_ref() {
        osc.param.clone()
    } else {
        syn::Ident::new("__flags", proc_macro2::Span::call_site())
    };

    // Generate auto-binding code from #[bind] attributes
    let auto_bindings: Vec<proc_macro2::TokenStream> = if let Some(ref state_def) = def.state {
        state_def
            .fields
            .iter()
            .filter_map(|f| {
                f.bind.as_ref().map(|b| {
                    let name = &f.name;
                    let expr = &b.expr;
                    let flag = &b.dirty_flag;
                    let pname = &osc_param_name;
                    quote! {
                        if #pname & #flag != 0 {
                            state.#name = #expr;
                        }
                    }
                })
            })
            .collect()
    } else {
        vec![]
    };

    // ADR-044 Phase B-5: `on_state_changed_effects` is the single
    // tier-1 export. `#[bind]` auto-bindings always run inside this
    // emission (the transitional B-2 dual-export path is gone). When
    // emitted from bindings alone (no user handler block), the body
    // defaults to `KakouneSideEffects::default()` so the bindings still
    // mutate STATE per tick and the host receives an empty but
    // well-formed tier-1 result.
    //
    // ADR-053 chunk 2: an `effects on StateChanged(...)` block replaces
    // the legacy sync body with a CPS-lowered runner that drives the
    // `__KasaneYielder` bridge. The auto-binding loop still runs first
    // so plugin authors keep the same `#[bind]` ergonomics.
    let on_state_changed_method = if has_osc || !auto_bindings.is_empty() {
        let param_name = &osc_param_name;
        let sync_body = if let Some(block) = state_changed_effects_block {
            let lowered = cps_lower_yields(block.body.clone());
            quote! {
                #[allow(unused_imports)]
                use ::kasane_plugin_sdk::effects::Yielder as _;
                let mut __yielder = __KasaneYielder::new();
                let __result: ::core::result::Result<
                    (),
                    ::kasane_plugin_sdk::effects::EffectError,
                > = (|| {
                    #lowered
                    Ok(())
                })();
                let _ = __result;
                __yielder.into_side_effects()
            }
        } else {
            def.on_state_changed_effects
                .as_ref()
                .map(|osc| osc.body.clone())
                .unwrap_or_else(|| quote! { KakouneSideEffects::default() })
        };
        let wrapped = if has_state {
            quote! {
                STATE.with(|__s| {
                    let __old_gen = __s.borrow().generation;
                    let mut state = __KasaneStateMutGuard {
                        inner: __s.borrow_mut(),
                        old_generation: __old_gen,
                        mutated: false,
                    };
                    #( #auto_bindings )*
                    { #sync_body }
                })
            }
        } else {
            quote! { { #sync_body } }
        };
        quote! {
            fn on_state_changed_effects(#param_name: u16) -> KakouneSideEffects {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let on_workspace_changed_method = if let Some(ref workspace_changed) = def.on_workspace_changed
    {
        let snapshot_param = &workspace_changed.param;
        let body = &workspace_changed.body;
        let wrapped = wrap_state(body);
        quote! {
            fn on_workspace_changed(#snapshot_param: WorkspaceSnapshot) {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let slots_method = if let Some(ref slots) = def.slots {
        let slot_arms: Vec<_> = slots
            .iter()
            .map(|entry| {
                let pattern = slot_name_to_pattern(&entry.name);
                let body = &entry.body;

                let wrapped_body = if entry.has_closure {
                    // Full form: body returns Option<Contribution>
                    let ctx_param = entry.ctx_param.as_ref().unwrap();
                    if has_state {
                        quote! {
                            STATE.with(|__s| {
                                let state = __s.borrow();
                                let #ctx_param = &__ctx;
                                #body
                            })
                        }
                    } else {
                        quote! { let #ctx_param = &__ctx; #body }
                    }
                } else {
                    // Simple form: body is an ElementHandle expression, auto-wrap
                    if has_state {
                        quote! {
                            STATE.with(|__s| {
                                let state = __s.borrow();
                                Some(auto_contribution(#body))
                            })
                        }
                    } else {
                        quote! { Some(auto_contribution(#body)) }
                    }
                };

                quote! { #pattern => { #wrapped_body } }
            })
            .collect();

        quote! {
            fn contribute_to(__region: SlotId, __ctx: ContributeContext) -> Option<Contribution> {
                match &__region {
                    #( #slot_arms, )*
                    _ => None,
                }
            }
        }
    } else {
        quote! {}
    };

    let annotate_method = if let Some(ref ann) = def.annotate {
        let line_param = &ann.line_param;
        let ctx_param = &ann.ctx_param;
        let body = &ann.body;
        let wrapped = wrap_state(body);
        quote! {
            fn annotate_line(#line_param: u32, #ctx_param: AnnotateContext) -> Option<LineAnnotation> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let display_directives_method = if let Some(ref body) = def.display_directives {
        let wrapped = wrap_state_shared(body);
        quote! {
            fn display_directives() -> Vec<DisplayDirective> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let display_method = if let Some(ref body) = def.display {
        let wrapped = wrap_state_shared(body);
        quote! {
            fn display() -> Vec<DisplayDirective> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let transform_method = if let Some(ref tr) = def.transform {
        let target_param = &tr.target_param;
        let element_param = &tr.element_param;
        let ctx_param = &tr.ctx_param;
        let body = &tr.body;
        let wrapped = wrap_state(body);
        quote! {
            fn transform(
                #target_param: TransformTarget,
                #element_param: TransformSubject,
                #ctx_param: TransformContext,
            ) -> TransformSubject {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let transform_patch_method = if let Some(ref tp) = def.transform_patch {
        let target_param = &tp.target_param;
        let ctx_param = &tp.ctx_param;
        let body = &tp.body;
        let wrapped = wrap_state_shared(body);
        quote! {
            fn transform_patch(
                #target_param: TransformTarget,
                #ctx_param: TransformContext,
            ) -> Vec<ElementPatchOp> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let transform_priority_method = if let Some(ref tp) = def.transform_priority {
        quote! { fn transform_priority() -> i16 { #tp } }
    } else {
        quote! {}
    };

    let overlay_method = if let Some(ref ov) = def.overlay {
        let ctx_param = &ov.param;
        let body = &ov.body;
        let wrapped = wrap_state(body);
        quote! {
            fn contribute_overlay_v2(#ctx_param: OverlayContext) -> Option<OverlayContribution> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_key_method = if let Some(ref hk) = def.handle_key {
        let event_param = &hk.param;
        let body = &hk.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_key(#event_param: KeyEvent) -> Option<Vec<Command>> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_key_middleware_method = if let Some(ref hk) = def.handle_key_middleware {
        let event_param = &hk.param;
        let body = &hk.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_key_middleware(#event_param: KeyEvent) -> KeyHandleResult {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_mouse_method = if let Some(ref hm) = def.handle_mouse {
        let event_param = &hm.event_param;
        let id_param = &hm.id_param;
        let body = &hm.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_mouse(#event_param: MouseEvent, #id_param: InteractiveId) -> Option<Vec<Command>> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_drop_method = if let Some(ref hd) = def.handle_drop {
        let event_param = &hd.event_param;
        let id_param = &hd.id_param;
        let body = &hd.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_drop(#event_param: DropEvent, #id_param: InteractiveId) -> Option<Vec<Command>> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let handle_default_scroll_method = if let Some(ref hs) = def.handle_default_scroll {
        let candidate_param = &hs.param;
        let body = &hs.body;
        let wrapped = wrap_state(body);
        quote! {
            fn handle_default_scroll(
                #candidate_param: DefaultScrollCandidate
            ) -> Option<ScrollPolicyResult> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let paint_inline_box_method = if let Some(ref pib) = def.paint_inline_box {
        let box_id_param = &pib.param;
        let body = &pib.body;
        let wrapped = wrap_state(body);
        quote! {
            fn paint_inline_box(#box_id_param: u64) -> Option<ElementHandle> {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let capabilities_method = if let Some(ref m) = def.manifest {
        let variants = &m.capability_variants;
        quote! {
            fn requested_capabilities() -> Vec<Capability> {
                vec![ #( #variants ),* ]
            }
        }
    } else if let Some(ref caps) = def.capabilities {
        quote! {
            fn requested_capabilities() -> Vec<Capability> {
                vec![ #caps ]
            }
        }
    } else {
        quote! {}
    };

    let authorities_method = if let Some(ref m) = def.manifest {
        let variants = &m.authority_variants;
        quote! {
            fn requested_authorities() -> Vec<PluginAuthority> {
                vec![ #( #variants ),* ]
            }
        }
    } else if let Some(ref authorities) = def.authorities {
        quote! {
            fn requested_authorities() -> Vec<PluginAuthority> {
                vec![ #authorities ]
            }
        }
    } else {
        quote! {}
    };

    let update_effects_method = if let Some(ref upd) = def.update_effects {
        let payload_param = &upd.param;
        let body = &upd.body;
        let wrapped = wrap_state(body);
        quote! {
            fn update_effects(#payload_param: Vec<u8>) -> ProcessCapableEffects {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    let on_io_event_method = if let Some(ref io) = def.on_io_event_effects {
        let event_param = &io.param;
        let body = &io.body;
        let wrapped = wrap_state(body);
        quote! {
            fn on_io_event_effects(#event_param: IoEvent) -> ProcessCapableEffects {
                #wrapped
            }
        }
    } else {
        quote! {}
    };

    // Generate view_deps method.
    // Priority: manifest view.deps > explicit view_deps > auto-infer from #[bind] flags > default (ALL).
    let view_deps_method = if let Some(ref m) = def.manifest {
        if m.has_view_deps {
            let mask = m.view_deps_mask;
            quote! { fn view_deps() -> u16 { #mask } }
        } else {
            quote! {} // Empty deps in manifest → fall through to default (ALL)
        }
    } else if let Some(ref vd) = def.view_deps {
        quote! { fn view_deps() -> u16 { #vd } }
    } else if let Some(ref state_def) = def.state {
        // Auto-infer from #[bind(expr, on: flag)] declarations
        let bind_flags: Vec<&proc_macro2::TokenStream> = state_def
            .fields
            .iter()
            .filter_map(|f| f.bind.as_ref().map(|b| &b.dirty_flag))
            .collect();
        if !bind_flags.is_empty() && !has_osc {
            // Only infer when there's no custom on_state_changed_effects
            // (custom handler may observe flags not declared in #[bind])
            quote! { fn view_deps() -> u16 { #( #bind_flags )|* } }
        } else {
            quote! {} // Fall through to default stub (ALL)
        }
    } else {
        quote! {} // No state, no view_deps — use default
    };

    // Key map protocol methods (Phase 4)
    let key_map_methods = if let Some(ref km) = def.key_map {
        let declare_groups = generate_key_map_declare(km);
        let is_active_arms = generate_is_group_active(km, has_state);
        let action_arms = generate_invoke_action(&def.actions, has_state, &wrap_state);
        quote! {
            fn declare_key_map() -> Vec<KeyGroupDecl> {
                #declare_groups
            }
            fn is_group_active(group_name: String) -> bool {
                #is_active_arms
            }
            #action_arms
        }
    } else {
        quote! {}
    };

    // state_hash: always connect to __kasane_auto_state_hash() (defined in state_tokens)
    let state_hash_method = quote! {
        fn state_hash() -> u64 { __kasane_auto_state_hash() }
    };

    // persist_state / restore_state: generated from #[persist] fields
    let persist_methods = if let Some(ref state_def) = def.state {
        let persist_fields: Vec<_> = state_def.fields.iter().filter(|f| f.persist).collect();
        if persist_fields.is_empty() {
            quote! {}
        } else {
            let ser_exprs: Vec<_> = persist_fields
                .iter()
                .map(|f| {
                    let name = &f.name;
                    quote! { state.#name.clone() }
                })
                .collect();
            let field_types: Vec<_> = persist_fields
                .iter()
                .map(|f| {
                    let ty = &f.ty;
                    quote! { #ty }
                })
                .collect();
            let restore_assigns: Vec<_> = persist_fields
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let name = &f.name;
                    let idx = syn::Index::from(i);
                    quote! { state.#name = restored.#idx; }
                })
                .collect();
            quote! {
                fn persist_state() -> Vec<u8> {
                    STATE.with(|s| {
                        let state = s.borrow();
                        let tuple: ( #( #field_types, )* ) = ( #( #ser_exprs, )* );
                        postcard::to_allocvec(&tuple).unwrap_or_default()
                    })
                }

                fn restore_state(data: Vec<u8>) -> bool {
                    STATE.with(|s| {
                        let restored: ( #( #field_types, )* ) = match postcard::from_bytes(&data) {
                            Ok(v) => v,
                            Err(_) => return false,
                        };
                        let mut state = s.borrow_mut();
                        #( #restore_assigns )*
                        state.bump_generation();
                        true
                    })
                }
            }
        }
    } else {
        quote! {}
    };

    // Generate register_capabilities from manifest handler_caps_mask if available.
    // This takes precedence over the auto-inference in #[plugin].
    let register_capabilities_method = if let Some(ref m) = def.manifest {
        if let Some(mask) = m.handler_caps_mask {
            quote! { fn register_capabilities() -> u32 { #mask } }
        } else {
            quote! {} // No handler flags in manifest → fall through to auto-inference
        }
    } else {
        quote! {} // No manifest → fall through to auto-inference
    };

    // Generate typed setting getters from `settings { ... }` block
    let settings_getters = if let Some(ref settings_fields) = def.settings {
        let getters: Vec<_> = settings_fields
            .iter()
            .map(|field| {
                let fn_name = syn::Ident::new(
                    &format!("__setting_{}", field.name),
                    field.name.span(),
                );
                let key_str = field.name.to_string();
                let default = &field.default;
                match field.ty {
                    SettingType::Bool => quote! {
                        #[doc(hidden)]
                        #[allow(dead_code)]
                        fn #fn_name() -> bool {
                            host_state::get_setting_bool(#key_str).unwrap_or(#default)
                        }
                    },
                    SettingType::Integer => quote! {
                        #[doc(hidden)]
                        #[allow(dead_code)]
                        fn #fn_name() -> i64 {
                            host_state::get_setting_integer(#key_str).unwrap_or(#default)
                        }
                    },
                    SettingType::Float => quote! {
                        #[doc(hidden)]
                        #[allow(dead_code)]
                        fn #fn_name() -> f64 {
                            host_state::get_setting_float(#key_str).unwrap_or(#default)
                        }
                    },
                    SettingType::Str => quote! {
                        #[doc(hidden)]
                        #[allow(dead_code)]
                        fn #fn_name() -> String {
                            host_state::get_setting_string(#key_str).unwrap_or_else(|| (#default).to_string())
                        }
                    },
                }
            })
            .collect();
        quote! { #( #getters )* }
    } else {
        quote! {}
    };

    // ADR-052 chunk 4: surface declared service capabilities as a
    // const so plugin code and tooling can introspect requested
    // authority without re-parsing the manifest. Identity-reflects-
    // authority (ADR-055) holds via the .kpk package hash — the
    // manifest is part of the artifact — so this constant is
    // informational, not a security boundary.
    let service_capabilities_const = if let Some(ref m) = def.manifest {
        let names = &m.service_capabilities;
        if names.is_empty() {
            quote! {
                #[allow(dead_code)]
                pub const REQUESTED_SERVICES: &[&str] = &[];
            }
        } else {
            let entries = names.iter().map(|n| quote! { #n });
            quote! {
                #[allow(dead_code)]
                pub const REQUESTED_SERVICES: &[&str] = &[ #( #entries ),* ];
            }
        }
    } else {
        quote! {
            #[allow(dead_code)]
            pub const REQUESTED_SERVICES: &[&str] = &[];
        }
    };

    // ADR-053 chunk 2: emit the yielder bridge once if any
    // `effects on <Trigger>` block was declared.
    let yielder_bridge = if def.effects_blocks.as_ref().is_some_and(|v| !v.is_empty()) {
        emit_yielder_bridge()
    } else {
        quote! {}
    };

    // ADR-053 chunk 3: project the effect set onto required capabilities
    // by scanning every yield site across every `effects on ...` block.
    // The emitted `__KasaneEffectSet` is a marker type implementing
    // `EffectSet`, with `REQUIRED_CAPABILITIES` populated. Chunk 5 wires
    // a manifest cross-check; today the const is consumed by the chunk-4
    // mock harness and is informational at runtime.
    let effect_set_marker = if let Some(blocks) = def.effects_blocks.as_ref() {
        let mut all_variants: Vec<(String, proc_macro2::Span)> = Vec::new();
        let mut unresolved_spans: Vec<proc_macro2::Span> = Vec::new();
        for block in blocks {
            let scan = scan_yield_sites(&block.body);
            for (name, span) in scan.variants {
                if !all_variants.iter().any(|(n, _)| n == &name) {
                    all_variants.push((name, span));
                }
            }
            unresolved_spans.extend(scan.unresolved);
        }
        if let Some(span) = unresolved_spans.first().copied() {
            return Err(syn::Error::new(
                span,
                "ADR-053 chunk 3: each `yield` site must literally name an \
                 `Effect::<Variant>(...)` constructor so the macro can \
                 project the required capability set at compile time. \
                 Indirect yields (function calls, variable references) \
                 are not supported.",
            ));
        }
        let caps = union_capabilities(&all_variants)?;
        let cap_literals = caps.iter().map(|name| {
            quote! { ::kasane_plugin_sdk::effects::CapabilityName(#name) }
        });
        quote! {
            #[doc(hidden)]
            #[allow(dead_code)]
            struct __KasaneEffectSet;

            impl ::kasane_plugin_sdk::effects::Sealed for __KasaneEffectSet {}

            impl ::kasane_plugin_sdk::effects::EffectSet for __KasaneEffectSet {
                type Yielder = __KasaneYielder;
                const REQUIRED_CAPABILITIES:
                    &'static [::kasane_plugin_sdk::effects::CapabilityName] =
                    &[ #( #cap_literals ),* ];
            }
        }
    } else {
        quote! {}
    };

    // Combine everything
    Ok(quote! {
        #wit_bindings
        #sdk_helpers

        #[allow(unused_imports)]
        use kasane_plugin_sdk::{dirty, modifiers, keys, attributes};

        #state_tokens
        #settings_getters
        #service_capabilities_const
        #yielder_bridge
        #effect_set_marker

        struct __KasanePlugin;

        #[kasane_plugin_sdk::plugin]
        impl Guest for __KasanePlugin {
            #get_id
            #on_init_method
            #on_active_session_ready_method
            #on_state_changed_method
            #on_workspace_changed_method
            #update_effects_method
            #slots_method
            #annotate_method
            #display_directives_method
            #display_method
            #transform_method
            #transform_patch_method
            #transform_priority_method
            #overlay_method
            #handle_key_method
            #handle_key_middleware_method
            #handle_mouse_method
            #handle_drop_method
            #handle_default_scroll_method
            #paint_inline_box_method
            #capabilities_method
            #authorities_method
            #on_io_event_method
            #view_deps_method
            #key_map_methods
            #register_capabilities_method
            #state_hash_method
            #persist_methods
        }

        export!(__KasanePlugin);
    })
}

// ---------------------------------------------------------------------------
// ADR-053 chunk 2: effects-block CPS lowering
// ---------------------------------------------------------------------------

/// ADR-053 chunk 3: capability map for the chunk-1 [`Effect`] taxonomy.
/// Mirrors `kasane_plugin_sdk::effects::Effect::required_capabilities`.
/// proc-macro crates cannot share runtime code, so the two sides must
/// stay in sync — adding a new `Effect` variant requires updating both
/// the SDK enum *and* this table.
fn effect_variant_capabilities(variant: &str) -> Option<&'static [&'static str]> {
    match variant {
        "Redraw" | "EvalCommand" => Some(&[]),
        "SetClipboard" | "PasteClipboard" => Some(&["clipboard"]),
        _ => None,
    }
}

/// Walk an `effects on` block body and collect the literal `Effect::<Variant>`
/// names appearing in `yield` sites. Yields whose right-hand side does not
/// statically name `Effect::<Variant>` are flagged so the caller can emit a
/// diagnostic — chunk 3 of ADR-053 demands literal variants so the
/// capability projection is sound.
struct YieldScan {
    /// Variant names in declaration order, deduplicated; paired with the
    /// span of the variant identifier so capability-resolution errors can
    /// point at the offending `yield` site.
    variants: Vec<(String, proc_macro2::Span)>,
    /// Spans of `yield` sites whose RHS could not be matched. Used to
    /// emit a compile error pointing at the offending site.
    unresolved: Vec<proc_macro2::Span>,
}

fn scan_yield_sites(body: &TokenStream) -> YieldScan {
    let mut scan = YieldScan {
        variants: Vec::new(),
        unresolved: Vec::new(),
    };
    scan_recursive(body.clone(), &mut scan);
    scan
}

fn scan_recursive(body: TokenStream, scan: &mut YieldScan) {
    let mut iter = body.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Ident(ref id) if id == "yield" => {
                let span = id.span();
                // Collect the RHS up to the next `;` at the same depth.
                let mut rhs: Vec<TokenTree> = Vec::new();
                while let Some(next) = iter.peek() {
                    if let TokenTree::Punct(p) = next {
                        if p.as_char() == ';' {
                            break;
                        }
                    }
                    rhs.push(iter.next().unwrap());
                }
                if let Some((variant, variant_span)) = extract_effect_variant(&rhs) {
                    if !scan.variants.iter().any(|(n, _)| n == &variant) {
                        scan.variants.push((variant, variant_span));
                    }
                } else {
                    scan.unresolved.push(span);
                }
            }
            TokenTree::Group(g) => {
                scan_recursive(g.stream(), scan);
            }
            _ => {}
        }
    }
}

/// Match `Effect :: <Variant>` (optionally followed by `(...)` or `{...}`)
/// at the head of `tokens`. Returns the variant name on a successful
/// match. Anything else — `MyEffect::Variant`, `foo()`, `path::to::Effect::X` —
/// is rejected so the projection only ever sees literal first-party
/// effect constructors.
fn extract_effect_variant(tokens: &[TokenTree]) -> Option<(String, proc_macro2::Span)> {
    // Skip a leading reference / qualified path that ends in `Effect`.
    // We accept the canonical `Effect::Variant` form and reject anything
    // more elaborate. Plugins that need indirection should match on an
    // enum and yield each branch literally.
    let mut iter = tokens.iter();
    let first = iter.next()?;
    let TokenTree::Ident(head) = first else {
        return None;
    };
    if head != "Effect" {
        return None;
    }
    let colon1 = iter.next()?;
    let colon2 = iter.next()?;
    match (colon1, colon2) {
        (TokenTree::Punct(p1), TokenTree::Punct(p2))
            if p1.as_char() == ':' && p2.as_char() == ':' => {}
        _ => return None,
    }
    let variant = iter.next()?;
    let TokenTree::Ident(variant_id) = variant else {
        return None;
    };
    Some((variant_id.to_string(), variant_id.span()))
}

/// Union the per-variant capability sets into a deduplicated, sorted
/// `&'static [&'static str]` literal for use in the generated EffectSet impl.
fn union_capabilities(variants: &[(String, proc_macro2::Span)]) -> syn::Result<Vec<String>> {
    let mut caps: Vec<String> = Vec::new();
    for (variant, span) in variants {
        let needs = effect_variant_capabilities(variant).ok_or_else(|| {
            syn::Error::new(
                *span,
                format!(
                    "unknown Effect variant `{variant}`; supported \
                     chunk-1 variants are Redraw, EvalCommand, \
                     SetClipboard, PasteClipboard"
                ),
            )
        })?;
        for c in needs {
            if !caps.iter().any(|x| x == c) {
                caps.push((*c).to_string());
            }
        }
    }
    caps.sort();
    Ok(caps)
}

/// Walk `body`, replacing each statement-level `yield <expr>;` (or `yield
/// <expr>` as the tail of a `let` initializer) with
/// `__yielder.emit(<expr>)?`.
///
/// The walker descends into every nested group (parentheses, brackets,
/// braces) so yields inside `if` / `match` / closures lower correctly. It
/// is intentionally simple: a yield expression spans tokens until the
/// next `;` *at the same nesting depth*. Yields embedded inside parens —
/// for example `let x = (yield foo) + 1` — would consume too much and
/// are not supported in chunk 2. The migration target (chunk 5) uses
/// only statement-level yields, so the restriction is documented but
/// not actively enforced.
fn cps_lower_yields(body: TokenStream) -> TokenStream {
    let mut out: Vec<TokenTree> = Vec::new();
    let mut iter = body.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Ident(ref id) if id == "yield" => {
                let mut expr_tokens: Vec<TokenTree> = Vec::new();
                while let Some(next) = iter.peek() {
                    if let TokenTree::Punct(p) = next {
                        if p.as_char() == ';' {
                            break;
                        }
                    }
                    expr_tokens.push(iter.next().unwrap());
                }
                let expr_ts: TokenStream = expr_tokens.into_iter().collect();
                let lowered_expr = cps_lower_yields(expr_ts);
                let replacement = quote! {
                    __yielder.emit(#lowered_expr)?
                };
                out.extend(replacement);
            }
            TokenTree::Group(g) => {
                let lowered_inner = cps_lower_yields(g.stream());
                let mut regrouped = Group::new(g.delimiter(), lowered_inner);
                regrouped.set_span(g.span());
                out.push(TokenTree::Group(regrouped));
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

/// Emit the `__KasaneYielder` bridge type. The bridge implements
/// [`kasane_plugin_sdk::effects::Yielder`] and forwards each
/// [`kasane_plugin_sdk::effects::Effect`] variant to the corresponding
/// tier-1 `KakouneSideCommand` so that lowered handlers slot into the
/// existing tier-1 wire format unchanged.
fn emit_yielder_bridge() -> TokenStream {
    quote! {
        #[doc(hidden)]
        struct __KasaneYielder {
            commands: ::std::vec::Vec<KakouneSideCommand>,
            redraw: u16,
        }

        impl ::kasane_plugin_sdk::effects::Yielder for __KasaneYielder {
            type Effect = ::kasane_plugin_sdk::effects::Effect;
            type Reply = ::kasane_plugin_sdk::effects::EffectReply;
            type Error = ::kasane_plugin_sdk::effects::EffectError;

            fn emit(
                &mut self,
                effect: Self::Effect,
            ) -> ::core::result::Result<Self::Reply, Self::Error> {
                use ::kasane_plugin_sdk::effects::{Effect, EffectReply, EffectError, CapabilityName};
                match effect {
                    Effect::Redraw(mask) => {
                        self.redraw |= mask;
                        Ok(EffectReply::Unit)
                    }
                    Effect::EvalCommand(s) => {
                        self.commands.push(KakouneSideCommand::EvalCommand(s));
                        Ok(EffectReply::Unit)
                    }
                    Effect::PasteClipboard => {
                        self.commands.push(KakouneSideCommand::PasteClipboard);
                        Ok(EffectReply::Unit)
                    }
                    // No tier-1 KakouneSideCommand variant carries clipboard-set.
                    // Surface as a runtime rejection until the WIT side gains
                    // a corresponding command (tracked in ADR-053 chunk 5).
                    Effect::SetClipboard(_) => Err(EffectError::MissingCapability(
                        CapabilityName("clipboard"),
                    )),
                    _ => Err(EffectError::Rejected(
                        "effect variant has no tier-1 bridge in ADR-053 chunk 2",
                    )),
                }
            }
        }

        impl __KasaneYielder {
            fn new() -> Self {
                Self {
                    commands: ::std::vec::Vec::new(),
                    redraw: 0,
                }
            }

            fn into_side_effects(self) -> KakouneSideEffects {
                KakouneSideEffects {
                    redraw: self.redraw,
                    commands: self.commands,
                    scroll_plans: ::std::vec::Vec::new(),
                }
            }
        }
    }
}

/// Lower an `EffectsBlock` to the Guest method that backs its trigger.
/// `has_state` controls whether the body is wrapped in the STATE guard
/// shared by all other handlers.
fn lower_effects_block(block: &EffectsBlock, has_state: bool) -> TokenStream {
    let lowered_body = cps_lower_yields(block.body.clone());

    // The user wrote, say, `effects on StateChanged(flags) { ... }`. The
    // generated handler signature uses `flags` directly, so the body sees
    // the same name it declared.
    let trigger_params: Vec<TokenStream> = match block.trigger {
        EffectsTrigger::StateChanged => {
            let p = &block.params[0];
            vec![quote! { #p: u16 }]
        }
        EffectsTrigger::Init | EffectsTrigger::SessionReady => Vec::new(),
    };

    // The runner closure returns `Result<(), EffectError>` so that lowered
    // `?` propagates cleanly. Errors are folded into the diagnostic stream
    // (ADR-033) once chunk 4 wires it; chunk 2 silently drops them, which
    // matches the legacy behavior of handlers that return whatever
    // commands they managed to accumulate before panicking.
    //
    // The `use Yielder` brings `emit` into scope so the lowered
    // `__yielder.emit(...)?` calls resolve without the plugin author
    // having to import the trait.
    let body_runner = quote! {
        #[allow(unused_imports)]
        use ::kasane_plugin_sdk::effects::Yielder as _;
        let mut __yielder = __KasaneYielder::new();
        let __result: ::core::result::Result<
            (),
            ::kasane_plugin_sdk::effects::EffectError,
        > = (|| {
            #lowered_body
            Ok(())
        })();
        let _ = __result;
        __yielder.into_side_effects()
    };

    let body_with_state = if has_state {
        quote! {
            STATE.with(|__s| {
                let __old_gen = __s.borrow().generation;
                let mut state = __KasaneStateMutGuard {
                    inner: __s.borrow_mut(),
                    old_generation: __old_gen,
                    mutated: false,
                };
                #body_runner
            })
        }
    } else {
        body_runner
    };

    match block.trigger {
        EffectsTrigger::StateChanged => quote! {
            fn on_state_changed_effects(#(#trigger_params),*) -> KakouneSideEffects {
                #body_with_state
            }
        },
        EffectsTrigger::Init => quote! {
            fn on_init_effects() -> BootstrapEffects {
                let __effects: KakouneSideEffects = { #body_with_state };
                __effects.into()
            }
        },
        EffectsTrigger::SessionReady => quote! {
            fn on_active_session_ready_effects() -> SessionReadyEffects {
                let __effects: KakouneSideEffects = { #body_with_state };
                __effects.into()
            }
        },
    }
}

// ---------------------------------------------------------------------------
// DSL parsing helpers
// ---------------------------------------------------------------------------

/// Parse tokens until a comma or end of input, consuming the comma if present.
fn parse_until_comma_or_end(
    input: syn::parse::ParseStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();
    while !input.is_empty() && !input.peek(syn::Token![,]) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        tokens.push(tt);
    }
    Ok(tokens.into_iter().collect())
}

/// Parse the expression part of `#[bind(expr, on: flag)]` — everything before `, on:`.
fn parse_bind_expr(input: syn::parse::ParseStream) -> syn::Result<proc_macro2::TokenStream> {
    let mut tokens = Vec::new();
    // Collect tokens until we see `, on` (comma followed by `on` ident)
    loop {
        if input.is_empty() {
            return Err(input.error("expected `, on: flags` in #[bind(expr, on: flags)]"));
        }
        // Peek ahead: if next is `,` and then `on`, stop
        if input.peek(syn::Token![,]) {
            let fork = input.fork();
            let _ = fork.parse::<syn::Token![,]>();
            if fork.peek(syn::Ident) {
                let ident: syn::Ident = fork.parse()?;
                if ident == "on" {
                    break;
                }
            }
        }
        let tt: proc_macro2::TokenTree = input.parse()?;
        tokens.push(tt);
    }
    Ok(tokens.into_iter().collect())
}

/// Parse slot entries from the `slots { ... }` block.
fn parse_slot_entries(input: syn::parse::ParseStream) -> syn::Result<Vec<SlotEntry>> {
    let mut entries = Vec::new();
    while !input.is_empty() {
        // 1. Slot name: IDENT or named("...")
        let name = {
            let ident: syn::Ident = input.parse()?;
            if ident == "named" {
                let args;
                syn::parenthesized!(args in input);
                let lit: syn::LitStr = args.parse()?;
                SlotName::Named(lit)
            } else {
                SlotName::WellKnown(ident)
            }
        };

        // 2. Optional (deps) — if next token is `(`, consume and ignore (deps removed)
        if input.peek(syn::token::Paren) {
            let args;
            syn::parenthesized!(args in input);
            let _: proc_macro2::TokenStream = args.parse()?;
        }

        // 3. `=>`
        input.parse::<syn::Token![=>]>()?;

        // 4. Closure `|ctx| { body }` or simple expression
        if input.peek(syn::Token![|]) {
            // Full closure form
            input.parse::<syn::Token![|]>()?;
            let ctx_param: syn::Ident = input.parse()?;
            input.parse::<syn::Token![|]>()?;
            let body;
            syn::braced!(body in input);
            let body_tokens: proc_macro2::TokenStream = body.parse()?;
            entries.push(SlotEntry {
                name,
                has_closure: true,
                ctx_param: Some(ctx_param),
                body: body_tokens,
            });
        } else {
            // Simple expression form — read until `,` or end
            let mut tokens = Vec::new();
            while !input.is_empty() && !input.peek(syn::Token![,]) {
                let tt: proc_macro2::TokenTree = input.parse()?;
                tokens.push(tt);
            }
            let body_tokens: proc_macro2::TokenStream = tokens.into_iter().collect();
            entries.push(SlotEntry {
                name,
                has_closure: false,
                ctx_param: None,
                body: body_tokens,
            });
        }

        // 5. Trailing comma
        if !input.is_empty() {
            let _ = input.parse::<syn::Token![,]>();
        }
    }
    Ok(entries)
}

/// Convert a SlotName to a match pattern for `SlotId`.
fn slot_name_to_pattern(name: &SlotName) -> proc_macro2::TokenStream {
    match name {
        SlotName::WellKnown(ident) => {
            let variant = match ident.to_string().as_str() {
                "BUFFER_LEFT" => quote! { BufferLeft },
                "BUFFER_RIGHT" => quote! { BufferRight },
                "ABOVE_BUFFER" => quote! { AboveBuffer },
                "BELOW_BUFFER" => quote! { BelowBuffer },
                "ABOVE_STATUS" => quote! { AboveStatus },
                "STATUS_LEFT" => quote! { StatusLeft },
                "STATUS_RIGHT" => quote! { StatusRight },
                "OVERLAY" => quote! { Overlay },
                other => {
                    let msg = format!("unknown well-known slot: `{other}`");
                    return quote! { compile_error!(#msg) };
                }
            };
            quote! { SlotId::WellKnown(WellKnownSlot::#variant) }
        }
        SlotName::Named(lit) => {
            quote! { SlotId::Named(ref __n) if __n == #lit }
        }
    }
}

// ---------------------------------------------------------------------------
// PluginDef parser (syn::parse::Parse impl)
// ---------------------------------------------------------------------------

impl syn::parse::Parse for PluginDef {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut def = PluginDef {
            manifest: None,
            id: syn::LitStr::new("", proc_macro2::Span::call_site()),
            state: None,
            on_init_effects: None,
            on_active_session_ready_effects: None,
            on_state_changed_effects: None,
            on_workspace_changed: None,
            update_effects: None,
            slots: None,
            annotate: None,
            display_directives: None,
            display: None,
            transform: None,
            transform_patch: None,
            transform_priority: None,
            overlay: None,
            handle_key: None,
            handle_key_middleware: None,
            handle_mouse: None,
            handle_drop: None,
            handle_default_scroll: None,
            paint_inline_box: None,
            capabilities: None,
            authorities: None,
            on_io_event_effects: None,
            view_deps: None,
            key_map: None,
            actions: None,
            settings: None,
            impl_block: None,
            effects_blocks: None,
        };

        let mut has_id = false;
        let mut has_manifest = false;
        let mut has_explicit_id = false;

        while !input.is_empty() {
            // `impl` is a Rust keyword, so it cannot be parsed as syn::Ident.
            // Check for it before the normal ident parse.
            if input.peek(syn::Token![impl]) {
                input.parse::<syn::Token![impl]>()?;
                let content;
                syn::braced!(content in input);
                let mut methods = Vec::new();
                while !content.is_empty() {
                    let method: ImplItemFn = content.parse()?;
                    methods.push(method);
                }
                def.impl_block = Some(methods);
                // Consume optional trailing comma between sections
                if !input.is_empty() {
                    let _ = input.parse::<syn::Token![,]>();
                }
                continue;
            }

            let ident: syn::Ident = input.parse()?;
            let section = ident.to_string();

            match section.as_str() {
                "manifest" => {
                    input.parse::<syn::Token![:]>()?;
                    let path_lit: syn::LitStr = input.parse()?;
                    let manifest_def = parse_manifest_at_compile_time(&path_lit)?;
                    def.id = syn::LitStr::new(&manifest_def.id, path_lit.span());
                    has_id = true;
                    has_manifest = true;
                    def.manifest = Some(manifest_def);
                }
                "id" => {
                    input.parse::<syn::Token![:]>()?;
                    def.id = input.parse()?;
                    has_id = true;
                    has_explicit_id = true;
                }
                "state" => {
                    let content;
                    syn::braced!(content in input);
                    let mut fields = Vec::new();
                    while !content.is_empty() {
                        // Parse optional attributes: #[bind(...)], #[persist]
                        let mut bind = None;
                        let mut persist = false;
                        while content.peek(syn::Token![#]) {
                            content.parse::<syn::Token![#]>()?;
                            let attr_content;
                            syn::bracketed!(attr_content in content);
                            let attr_name: syn::Ident = attr_content.parse()?;
                            if attr_name == "bind" {
                                let bind_args;
                                syn::parenthesized!(bind_args in attr_content);
                                let expr = parse_bind_expr(&bind_args)?;
                                bind_args.parse::<syn::Token![,]>()?;
                                let on_kw: syn::Ident = bind_args.parse()?;
                                if on_kw != "on" {
                                    return Err(syn::Error::new(
                                        on_kw.span(),
                                        "expected `on:` in #[bind(expr, on: flags)]",
                                    ));
                                }
                                bind_args.parse::<syn::Token![:]>()?;
                                let dirty_flag: proc_macro2::TokenStream = bind_args.parse()?;
                                bind = Some(BindDef { expr, dirty_flag });
                            } else if attr_name == "persist" {
                                persist = true;
                            } else {
                                return Err(syn::Error::new(
                                    attr_name.span(),
                                    "only #[bind(...)] and #[persist] are supported on state fields",
                                ));
                            }
                        }

                        let name: syn::Ident = content.parse()?;
                        content.parse::<syn::Token![:]>()?;
                        let ty: syn::Type = content.parse()?;
                        content.parse::<syn::Token![=]>()?;
                        let default: syn::Expr = content.parse()?;
                        if !content.is_empty() {
                            content.parse::<syn::Token![,]>()?;
                        }
                        fields.push(StateField {
                            name,
                            ty,
                            default,
                            bind,
                            persist,
                        });
                    }
                    def.state = Some(StateDef { fields });
                }
                "on_init_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let _ = params;
                    let body;
                    syn::braced!(body in input);
                    def.on_init_effects = Some(body.parse()?);
                }
                "on_active_session_ready_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let _ = params;
                    let body;
                    syn::braced!(body in input);
                    def.on_active_session_ready_effects = Some(body.parse()?);
                }
                "on_state_changed_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.on_state_changed_effects = Some(OnStateChanged {
                        param,
                        body: body.parse()?,
                    });
                }
                "effects" => {
                    // ADR-053 chunk 2: `effects on <Trigger>(<params>) { <body> }`.
                    // Lower yield-based effect blocks to existing handler form
                    // (StateChanged → on_state_changed_effects, etc.).
                    let on_kw: syn::Ident = input.parse()?;
                    if on_kw != "on" {
                        return Err(syn::Error::new(
                            on_kw.span(),
                            "expected `on` after `effects` (e.g. \
                             `effects on StateChanged(flags) { ... }`)",
                        ));
                    }
                    let trigger_ident: syn::Ident = input.parse()?;
                    let trigger = EffectsTrigger::from_ident(&trigger_ident)?;
                    let params_buf;
                    syn::parenthesized!(params_buf in input);
                    let mut params: Vec<syn::Ident> = Vec::new();
                    while !params_buf.is_empty() {
                        params.push(params_buf.parse()?);
                        if !params_buf.is_empty() {
                            params_buf.parse::<syn::Token![,]>()?;
                        }
                    }
                    if params.len() != trigger.expected_arity() {
                        return Err(syn::Error::new(
                            trigger_ident.span(),
                            format!(
                                "trigger `{}` expects {} parameter(s); got {}",
                                trigger.label(),
                                trigger.expected_arity(),
                                params.len()
                            ),
                        ));
                    }
                    let body;
                    syn::braced!(body in input);
                    let body_tokens: TokenStream = body.parse()?;
                    let blocks = def.effects_blocks.get_or_insert_with(Vec::new);
                    if blocks.iter().any(|b| b.trigger == trigger) {
                        return Err(syn::Error::new(
                            trigger_ident.span(),
                            format!(
                                "duplicate `effects on {}` block in define_plugin!",
                                trigger.label()
                            ),
                        ));
                    }
                    blocks.push(EffectsBlock {
                        trigger,
                        params,
                        body: body_tokens,
                    });
                }
                "on_state_changed_tier1_effects" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_state_changed_tier1_effects` was \
                         removed in WIT 5.0.0 (ADR-044 Phase B-5). The single \
                         `on_state_changed_effects` now returns \
                         `KakouneSideEffects` directly — rename the block.",
                    ));
                }
                "on_workspace_changed" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.on_workspace_changed = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "update_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.update_effects = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "slots" => {
                    let body;
                    syn::braced!(body in input);
                    let entries = parse_slot_entries(&body)?;
                    def.slots = Some(entries);
                }
                "annotate" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let line_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let ctx_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.annotate = Some(AnnotateDef {
                        line_param,
                        ctx_param,
                        body: body.parse()?,
                    });
                }
                "display_directives" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let _ = params;
                    let body;
                    syn::braced!(body in input);
                    def.display_directives = Some(body.parse()?);
                }
                "display" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let _ = params;
                    let body;
                    syn::braced!(body in input);
                    def.display = Some(body.parse()?);
                }
                "transform" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let target_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let element_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let ctx_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.transform = Some(TransformDef {
                        target_param,
                        element_param,
                        ctx_param,
                        body: body.parse()?,
                    });
                }
                "transform_patch" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let target_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let ctx_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.transform_patch = Some(TransformPatchDef {
                        target_param,
                        ctx_param,
                        body: body.parse()?,
                    });
                }
                "transform_priority" => {
                    input.parse::<syn::Token![:]>()?;
                    def.transform_priority = Some(parse_until_comma_or_end(input)?);
                }
                "overlay" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.overlay = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "handle_key" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_key = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "handle_key_middleware" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_key_middleware = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "handle_mouse" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let event_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let id_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_mouse = Some(HandleMouseDef {
                        event_param,
                        id_param,
                        body: body.parse()?,
                    });
                }
                "handle_drop" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let event_param: syn::Ident = params.parse()?;
                    params.parse::<syn::Token![,]>()?;
                    let id_param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_drop = Some(HandleDropDef {
                        event_param,
                        id_param,
                        body: body.parse()?,
                    });
                }
                "handle_default_scroll" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.handle_default_scroll = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "paint_inline_box" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.paint_inline_box = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "capabilities" => {
                    input.parse::<syn::Token![:]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    def.capabilities = Some(content.parse()?);
                }
                "authorities" => {
                    input.parse::<syn::Token![:]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    def.authorities = Some(content.parse()?);
                }
                "on_io_event_effects" => {
                    let params;
                    syn::parenthesized!(params in input);
                    let param: syn::Ident = params.parse()?;
                    let body;
                    syn::braced!(body in input);
                    def.on_io_event_effects = Some(ParamBodyDef {
                        param,
                        body: body.parse()?,
                    });
                }
                "view_deps" => {
                    input.parse::<syn::Token![:]>()?;
                    def.view_deps = Some(input.parse()?);
                }
                "on_init" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_init` was removed; use `on_init_effects()`",
                    ));
                }
                "on_active_session_ready" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_active_session_ready` was removed; use `on_active_session_ready_effects()`",
                    ));
                }
                "on_state_changed" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_state_changed` was removed; use `on_state_changed_effects(...)`",
                    ));
                }
                "on_state_changed_commands" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_state_changed_commands` was removed; return `Effects` from `on_state_changed_effects(...)`",
                    ));
                }
                "update" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `update` was removed; use `update_effects(...)`",
                    ));
                }
                "on_io_event" => {
                    return Err(syn::Error::new(
                        ident.span(),
                        "define_plugin! `on_io_event` was removed; use `on_io_event_effects(...)`",
                    ));
                }
                "key_map" => {
                    let body;
                    syn::braced!(body in input);
                    def.key_map = Some(parse_key_map_def(&body)?);
                }
                "actions" => {
                    let body;
                    syn::braced!(body in input);
                    def.actions = Some(parse_actions_def(&body)?);
                }
                "settings" => {
                    let body;
                    syn::braced!(body in input);
                    let mut fields = Vec::new();
                    while !body.is_empty() {
                        let name: syn::Ident = body.parse()?;
                        body.parse::<syn::Token![:]>()?;
                        let ty_ident: syn::Ident = body.parse()?;
                        let ty = SettingType::from_ident(&ty_ident)?;
                        body.parse::<syn::Token![=]>()?;
                        let default: syn::Expr = body.parse()?;
                        if !body.is_empty() {
                            body.parse::<syn::Token![,]>()?;
                        }
                        fields.push(SettingFieldDef { name, ty, default });
                    }
                    def.settings = Some(fields);
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("unknown define_plugin section: `{other}`"),
                    ));
                }
            }

            // Consume optional trailing comma between sections
            if !input.is_empty() {
                let _ = input.parse::<syn::Token![,]>();
            }
        }

        if !has_id {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "define_plugin! requires an `id: \"...\"` or `manifest: \"...\"` section",
            ));
        }

        // Conflict detection: manifest: is mutually exclusive with id:, capabilities:, authorities:
        if has_manifest {
            if has_explicit_id {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "define_plugin! `id:` conflicts with `manifest:` — the plugin ID is declared in the manifest TOML",
                ));
            }
            if def.capabilities.is_some() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "define_plugin! `capabilities:` conflicts with `manifest:` — capabilities are declared in the manifest TOML",
                ));
            }
            if def.authorities.is_some() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "define_plugin! `authorities:` conflicts with `manifest:` — authorities are declared in the manifest TOML",
                ));
            }
        }

        // Validate settings {} fields against manifest [settings.*] if both present
        if has_manifest {
            if let (Some(manifest), Some(settings_fields)) = (&def.manifest, &def.settings) {
                for field in settings_fields {
                    let key = field.name.to_string();
                    match manifest.settings_schema.get(&key) {
                        None => {
                            return Err(syn::Error::new(
                                field.name.span(),
                                format!(
                                    "setting `{key}` is declared in define_plugin! but not in manifest [settings.{key}]"
                                ),
                            ));
                        }
                        Some(manifest_type) => {
                            let expected = field.ty.manifest_type_str();
                            if manifest_type != expected {
                                return Err(syn::Error::new(
                                    field.name.span(),
                                    format!(
                                        "setting `{key}` type mismatch: define_plugin! declares `{}` (manifest type \"{expected}\") but manifest has type \"{manifest_type}\"",
                                        field.name
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }

        if def.impl_block.is_some() && def.state.is_none() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "define_plugin! `impl { ... }` requires a `state { ... }` section",
            ));
        }

        // ADR-053 chunk 2: an `effects on <Trigger>` block and the legacy
        // handler block for the same trigger are mutually exclusive — both
        // emit the same Guest method body, so allowing both would silently
        // drop one of them.
        if let Some(blocks) = def.effects_blocks.as_ref() {
            for block in blocks {
                let conflict = match block.trigger {
                    EffectsTrigger::StateChanged => {
                        def.on_state_changed_effects.is_some().then_some(
                            "`effects on StateChanged` conflicts with \
                             `on_state_changed_effects(...) { ... }`; use one or the other",
                        )
                    }
                    EffectsTrigger::Init => def.on_init_effects.is_some().then_some(
                        "`effects on Init` conflicts with \
                         `on_init_effects() { ... }`; use one or the other",
                    ),
                    EffectsTrigger::SessionReady => {
                        def.on_active_session_ready_effects.is_some().then_some(
                            "`effects on SessionReady` conflicts with \
                             `on_active_session_ready_effects() { ... }`; use one or the other",
                        )
                    }
                };
                if let Some(msg) = conflict {
                    return Err(syn::Error::new(proc_macro2::Span::call_site(), msg));
                }
            }
        }

        Ok(def)
    }
}
