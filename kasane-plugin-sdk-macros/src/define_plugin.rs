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
    transform: Option<TransformDef>,
    transform_priority: Option<proc_macro2::TokenStream>,
    overlay: Option<ParamBodyDef>,
    handle_key: Option<ParamBodyDef>,
    handle_key_middleware: Option<ParamBodyDef>,
    handle_mouse: Option<HandleMouseDef>,
    handle_default_scroll: Option<ParamBodyDef>,
    capabilities: Option<proc_macro2::TokenStream>,
    authorities: Option<proc_macro2::TokenStream>,
    on_io_event_effects: Option<ParamBodyDef>,
    view_deps: Option<proc_macro2::TokenStream>,
    key_map: Option<KeyMapDef>,
    actions: Option<Vec<ActionDef>>,
    impl_block: Option<Vec<ImplItemFn>>,
}

struct StateDef {
    fields: Vec<StateField>,
}

struct StateField {
    name: syn::Ident,
    ty: syn::Type,
    default: syn::Expr,
    bind: Option<BindDef>,
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

// ---------------------------------------------------------------------------
// define_plugin! implementation
// ---------------------------------------------------------------------------

pub(crate) fn define_plugin_impl(
    input: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let def: PluginDef = syn::parse2(input)?;

    // 1. generate!()
    let wit_content = include_str!("../wit/plugin.wit");
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

    let on_init_method = if let Some(ref body) = def.on_init_effects {
        let wrapped = wrap_state(body);
        quote! { fn on_init_effects() -> BootstrapEffects { let __effects: Effects = { #wrapped }; __effects.into() } }
    } else {
        quote! {}
    };

    let on_active_session_ready_method = if let Some(ref body) = def.on_active_session_ready_effects
    {
        let wrapped = wrap_state(body);
        quote! {
            fn on_active_session_ready_effects() -> SessionReadyEffects { let __effects: Effects = { #wrapped }; __effects.into() }
        }
    } else {
        quote! {}
    };

    // Determine the dirty-flags parameter name early so auto-bindings can reference it.
    let has_osc = def.on_state_changed_effects.is_some();
    let osc_param_name = def
        .on_state_changed_effects
        .as_ref()
        .map(|osc| osc.param.clone())
        .unwrap_or_else(|| syn::Ident::new("__flags", proc_macro2::Span::call_site()));

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

    let has_bindings = !auto_bindings.is_empty();

    let on_state_changed_method = if has_osc || has_bindings {
        let param_name = &osc_param_name;

        let sync_body = def
            .on_state_changed_effects
            .as_ref()
            .map(|osc| osc.body.clone())
            .unwrap_or_else(|| quote! { Effects::default() });

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
            quote! {
                { #sync_body }
            }
        };
        quote! {
            fn on_state_changed_effects(#param_name: u16) -> RuntimeEffects {
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
            fn update_effects(#payload_param: Vec<u8>) -> RuntimeEffects {
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
            fn on_io_event_effects(#event_param: IoEvent) -> RuntimeEffects {
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

    // Combine everything
    Ok(quote! {
        #wit_bindings
        #sdk_helpers

        #[allow(unused_imports)]
        use kasane_plugin_sdk::{dirty, modifiers, keys, attributes};

        #state_tokens

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
            #transform_method
            #transform_priority_method
            #overlay_method
            #handle_key_method
            #handle_key_middleware_method
            #handle_mouse_method
            #handle_default_scroll_method
            #capabilities_method
            #authorities_method
            #on_io_event_method
            #view_deps_method
            #key_map_methods
            #state_hash_method
        }

        export!(__KasanePlugin);
    })
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
            transform: None,
            transform_priority: None,
            overlay: None,
            handle_key: None,
            handle_key_middleware: None,
            handle_mouse: None,
            handle_default_scroll: None,
            capabilities: None,
            authorities: None,
            on_io_event_effects: None,
            view_deps: None,
            key_map: None,
            actions: None,
            impl_block: None,
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
                        // Parse optional #[bind(expr, on: flag)] attribute
                        let bind = if content.peek(syn::Token![#]) {
                            content.parse::<syn::Token![#]>()?;
                            let attr_content;
                            syn::bracketed!(attr_content in content);
                            let attr_name: syn::Ident = attr_content.parse()?;
                            if attr_name != "bind" {
                                return Err(syn::Error::new(
                                    attr_name.span(),
                                    "only #[bind(...)] is supported on state fields",
                                ));
                            }
                            let bind_args;
                            syn::parenthesized!(bind_args in attr_content);
                            // Parse: expr, on: flag_expr
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
                            Some(BindDef { expr, dirty_flag })
                        } else {
                            None
                        };

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

        if def.impl_block.is_some() && def.state.is_none() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "define_plugin! `impl { ... }` requires a `state { ... }` section",
            ));
        }

        Ok(def)
    }
}
