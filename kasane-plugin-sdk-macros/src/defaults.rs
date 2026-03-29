use quote::quote;
use syn::{ImplItem, ItemImpl, parse_macro_input};

/// Implementation of the `kasane_wasm_plugin` attribute macro.
///
/// Fills in default implementations for all unimplemented `Guest` trait methods.
pub(crate) fn kasane_wasm_plugin_impl(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let mut impl_block = parse_macro_input!(item as ItemImpl);

    // Collect names of methods already implemented by the user.
    let existing: std::collections::HashSet<String> = impl_block
        .items
        .iter()
        .filter_map(|item| {
            if let ImplItem::Fn(method) = item {
                Some(method.sig.ident.to_string())
            } else {
                None
            }
        })
        .collect();

    // Validate that all user-written methods are known Guest methods.
    let known = known_guest_methods();
    let mut errors = Vec::new();
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            let name = method.sig.ident.to_string();
            if !known.contains(name.as_str()) {
                let suggestions = suggest_similar(&name, &known);
                let msg = if suggestions.is_empty() {
                    format!("unknown Guest method `{name}`")
                } else {
                    format!("unknown Guest method `{name}`. Did you mean {suggestions}?")
                };
                errors.push(syn::Error::new(method.sig.ident.span(), msg));
            }
        }
    }
    if !errors.is_empty() {
        let combined = errors
            .into_iter()
            .reduce(|mut a, b| {
                a.combine(b);
                a
            })
            .unwrap();
        return combined.into_compile_error().into();
    }

    // Generate defaults for every Guest method not already present.
    let defaults = generate_defaults(&existing);

    impl_block
        .items
        .extend(defaults.into_iter().map(ImplItem::Fn));

    proc_macro::TokenStream::from(quote! { #impl_block })
}

/// The complete set of valid Guest trait method names.
fn known_guest_methods() -> std::collections::HashSet<&'static str> {
    [
        "get_id",
        "on_init_effects",
        "on_active_session_ready_effects",
        "on_shutdown",
        "on_state_changed_effects",
        "on_workspace_changed",
        "surfaces",
        "render_surface",
        "handle_surface_event",
        "handle_surface_state_changed",
        "contribute",
        "contribute_named",
        "contribute_to",
        "contribute_line",
        "contribute_overlay",
        "contribute_overlay_v2",
        "annotate_line",
        "display_directives",
        "replace",
        "decorate",
        "decorator_priority",
        "transform",
        "transform_patch",
        "transform_priority",
        "transform_menu_item",
        "handle_mouse",
        "handle_key",
        "handle_key_middleware",
        "handle_default_scroll",
        "observe_key",
        "observe_mouse",
        "state_hash",
        "cursor_style_override",
        "decorate_cells",
        "update_effects",
        "requested_capabilities",
        "requested_authorities",
        "on_io_event_effects",
        "view_deps",
        "register_capabilities",
        "declare_key_map",
        "is_group_active",
        "invoke_action",
        "navigation_policy",
        "on_navigation_action",
    ]
    .into_iter()
    .collect()
}

/// Suggest similar method names using edit distance (Levenshtein).
fn suggest_similar(input: &str, known: &std::collections::HashSet<&str>) -> String {
    let mut candidates: Vec<(usize, &str)> = known
        .iter()
        .map(|k| (edit_distance(input, k), *k))
        .filter(|(d, _)| *d <= 4)
        .collect();
    candidates.sort_by_key(|(d, _)| *d);
    candidates
        .iter()
        .take(3)
        .map(|(_, k)| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Simple Levenshtein distance.
#[allow(clippy::needless_range_loop)]
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Generate default `ImplItemFn` nodes for all Guest methods not in `existing`.
pub(crate) fn generate_defaults(
    existing: &std::collections::HashSet<String>,
) -> Vec<syn::ImplItemFn> {
    let mut defaults = Vec::new();

    macro_rules! add_default {
        ($name:expr, $tokens:expr) => {
            if !existing.contains($name) {
                defaults.push(syn::parse2($tokens).unwrap_or_else(|e| {
                    panic!(
                        "kasane_wasm_plugin: failed to parse default for `{}`: {}",
                        $name, e
                    )
                }));
            }
        };
    }

    // --- Lifecycle ---

    add_default!(
        "on_init_effects",
        quote! { fn on_init_effects() -> BootstrapEffects { Effects::default().into() } }
    );

    add_default!(
        "on_active_session_ready_effects",
        quote! {
            fn on_active_session_ready_effects() -> SessionReadyEffects {
                Effects::default().into()
            }
        }
    );

    add_default!(
        "on_shutdown",
        quote! { fn on_shutdown() -> Vec<Command> { vec![] } }
    );

    add_default!(
        "on_state_changed_effects",
        quote! {
            fn on_state_changed_effects(_dirty_flags: u16) -> RuntimeEffects {
                Effects::default()
            }
        }
    );

    add_default!(
        "on_workspace_changed",
        quote! {
            fn on_workspace_changed(_snapshot: WorkspaceSnapshot) {}
        }
    );

    // --- Surfaces ---

    add_default!(
        "surfaces",
        quote! { fn surfaces() -> Vec<SurfaceDescriptor> { vec![] } }
    );

    add_default!(
        "render_surface",
        quote! {
            fn render_surface(
                _surface_key: String,
                _ctx: SurfaceViewContext,
            ) -> Option<ElementHandle> {
                None
            }
        }
    );

    add_default!(
        "handle_surface_event",
        quote! {
            fn handle_surface_event(
                _surface_key: String,
                _event: SurfaceEvent,
                _ctx: SurfaceEventContext,
            ) -> Vec<Command> {
                vec![]
            }
        }
    );

    add_default!(
        "handle_surface_state_changed",
        quote! {
            fn handle_surface_state_changed(
                _surface_key: String,
                _dirty_flags: u16,
            ) -> Vec<Command> {
                vec![]
            }
        }
    );

    // --- Slot contributions (legacy) ---

    add_default!(
        "contribute",
        quote! { fn contribute(_slot: u8) -> Option<ElementHandle> { None } }
    );

    add_default!(
        "contribute_named",
        quote! { fn contribute_named(_slot_name: String) -> Option<ElementHandle> { None } }
    );

    // --- Slot contributions (current) ---

    add_default!(
        "contribute_to",
        quote! {
            fn contribute_to(_region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
                None
            }
        }
    );

    // --- Line decoration (legacy) ---

    add_default!(
        "contribute_line",
        quote! { fn contribute_line(_line: u32) -> Option<LineDecoration> { None } }
    );

    // --- Overlay (legacy) ---

    add_default!(
        "contribute_overlay",
        quote! { fn contribute_overlay() -> Option<Overlay> { None } }
    );

    // --- Overlay (current) ---

    add_default!(
        "contribute_overlay_v2",
        quote! {
            fn contribute_overlay_v2(_ctx: OverlayContext) -> Option<OverlayContribution> {
                None
            }
        }
    );

    // --- Line annotation (current) ---

    add_default!(
        "annotate_line",
        quote! {
            fn annotate_line(_line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
                None
            }
        }
    );

    add_default!(
        "display_directives",
        quote! {
            fn display_directives() -> Vec<DisplayDirective> {
                vec![]
            }
        }
    );

    // --- Element transformation (legacy) ---

    add_default!(
        "replace",
        quote! { fn replace(_target: ReplaceTarget) -> Option<ElementHandle> { None } }
    );

    add_default!(
        "decorate",
        quote! {
            fn decorate(_target: DecorateTarget, element: ElementHandle) -> ElementHandle {
                element
            }
        }
    );

    add_default!(
        "decorator_priority",
        quote! { fn decorator_priority() -> u32 { 0 } }
    );

    // --- Element transformation (current) ---

    add_default!(
        "transform",
        quote! {
            fn transform(
                _target: TransformTarget,
                subject: TransformSubject,
                _ctx: TransformContext,
            ) -> TransformSubject {
                subject
            }
        }
    );

    add_default!(
        "transform_patch",
        quote! {
            fn transform_patch(
                _target: TransformTarget,
                _ctx: TransformContext,
            ) -> Vec<ElementPatchOp> {
                Vec::new()
            }
        }
    );

    add_default!(
        "transform_priority",
        quote! { fn transform_priority() -> i16 { 0 } }
    );

    add_default!(
        "transform_menu_item",
        quote! {
            fn transform_menu_item(
                _item: Vec<Atom>,
                _index: u32,
                _selected: bool,
            ) -> Option<Vec<Atom>> {
                None
            }
        }
    );

    // --- Input handling ---

    add_default!(
        "handle_mouse",
        quote! {
            fn handle_mouse(_event: MouseEvent, _id: InteractiveId) -> Option<Vec<Command>> {
                None
            }
        }
    );

    add_default!(
        "handle_key",
        quote! {
            fn handle_key(_event: KeyEvent) -> Option<Vec<Command>> {
                None
            }
        }
    );

    add_default!(
        "handle_key_middleware",
        quote! {
            fn handle_key_middleware(event: KeyEvent) -> KeyHandleResult {
                match Self::handle_key(event) {
                    Some(commands) => KeyHandleResult::Consumed(commands),
                    None => KeyHandleResult::Passthrough,
                }
            }
        }
    );

    add_default!(
        "handle_default_scroll",
        quote! {
            fn handle_default_scroll(
                _candidate: DefaultScrollCandidate
            ) -> Option<ScrollPolicyResult> {
                None
            }
        }
    );

    add_default!(
        "observe_key",
        quote! { fn observe_key(_event: KeyEvent) {} }
    );

    add_default!(
        "observe_mouse",
        quote! { fn observe_mouse(_event: MouseEvent) {} }
    );

    // --- Key map protocol (Phase 3+) ---

    add_default!(
        "declare_key_map",
        quote! {
            fn declare_key_map() -> Vec<KeyGroupDecl> {
                Vec::new()
            }
        }
    );

    add_default!(
        "is_group_active",
        quote! {
            fn is_group_active(_group_name: String) -> bool {
                true
            }
        }
    );

    add_default!(
        "invoke_action",
        quote! {
            fn invoke_action(_action_id: String, _event: KeyEvent) -> KeyResponse {
                KeyResponse::Pass
            }
        }
    );

    // --- Caching ---

    add_default!("state_hash", quote! { fn state_hash() -> u64 { 0 } });

    // --- Cursor ---

    add_default!(
        "cursor_style_override",
        quote! { fn cursor_style_override() -> Option<u8> { None } }
    );

    add_default!(
        "decorate_cells",
        quote! { fn decorate_cells() -> Vec<CellDecoration> { Vec::new() } }
    );

    // --- Inter-plugin messaging ---

    add_default!(
        "update_effects",
        quote! { fn update_effects(_payload: Vec<u8>) -> RuntimeEffects { Effects::default() } }
    );

    // --- WASI capabilities ---

    add_default!(
        "requested_capabilities",
        quote! { fn requested_capabilities() -> Vec<Capability> { vec![] } }
    );

    add_default!(
        "requested_authorities",
        quote! { fn requested_authorities() -> Vec<PluginAuthority> { vec![] } }
    );

    // --- I/O events ---

    add_default!(
        "on_io_event_effects",
        quote! { fn on_io_event_effects(_event: IoEvent) -> RuntimeEffects { Effects::default() } }
    );

    // --- View dependency declaration ---

    add_default!(
        "view_deps",
        quote! { fn view_deps() -> u16 { 0x17F } } // ALL
    );

    // --- Navigation (DU-4) ---

    add_default!(
        "navigation_policy",
        quote! { fn navigation_policy(_unit: DisplayUnitInfo) -> NavigationPolicyKind { NavigationPolicyKind::Normal } }
    );

    add_default!(
        "on_navigation_action",
        quote! { fn on_navigation_action(_unit: DisplayUnitInfo, _action_kind: u32) -> NavigationActionResult { NavigationActionResult { handled: false, keys: None } } }
    );

    // --- Handler capability declaration (v0.23.0) ---
    // Auto-infer PluginCapabilities bitmask from which methods are implemented.
    // Bit layout matches kasane-core PluginCapabilities bitflags.
    if !existing.contains("register_capabilities") {
        let mut caps: u32 = 0;
        // OVERLAY = 1 << 2
        if existing.contains("contribute_overlay") || existing.contains("contribute_overlay_v2") {
            caps |= 1 << 2;
        }
        // MENU_TRANSFORM = 1 << 5
        if existing.contains("transform_menu_item") {
            caps |= 1 << 5;
        }
        // CURSOR_STYLE = 1 << 6
        if existing.contains("cursor_style_override") {
            caps |= 1 << 6;
        }
        // INPUT_HANDLER = 1 << 7
        if existing.contains("handle_key")
            || existing.contains("handle_key_middleware")
            || existing.contains("handle_mouse")
        {
            caps |= 1 << 7;
        }
        // SURFACE_PROVIDER = 1 << 11
        if existing.contains("surfaces") {
            caps |= 1 << 11;
        }
        // WORKSPACE_OBSERVER = 1 << 12
        if existing.contains("on_workspace_changed") {
            caps |= 1 << 12;
        }
        // CONTRIBUTOR = 1 << 14
        if existing.contains("contribute")
            || existing.contains("contribute_to")
            || existing.contains("contribute_named")
        {
            caps |= 1 << 14;
        }
        // TRANSFORMER = 1 << 15
        if existing.contains("transform")
            || existing.contains("transform_patch")
            || existing.contains("replace")
            || existing.contains("decorate")
        {
            caps |= 1 << 15;
        }
        // ANNOTATOR = 1 << 16
        if existing.contains("annotate_line") || existing.contains("contribute_line") {
            caps |= 1 << 16;
        }
        // IO_HANDLER = 1 << 17
        if existing.contains("on_io_event_effects") {
            caps |= 1 << 17;
        }
        // DISPLAY_TRANSFORM = 1 << 18
        if existing.contains("display_directives") {
            caps |= 1 << 18;
        }
        // SCROLL_POLICY = 1 << 19
        if existing.contains("handle_default_scroll") {
            caps |= 1 << 19;
        }
        // CELL_DECORATION = 1 << 20
        if existing.contains("decorate_cells") {
            caps |= 1 << 20;
        }
        // NAVIGATION_POLICY = 1 << 21
        if existing.contains("navigation_policy") {
            caps |= 1 << 21;
        }
        // NAVIGATION_ACTION = 1 << 22
        if existing.contains("on_navigation_action") {
            caps |= 1 << 22;
        }

        let caps_literal = caps;
        defaults.push(
            syn::parse2(quote! {
                fn register_capabilities() -> u32 { #caps_literal }
            })
            .expect("register_capabilities default"),
        );
    }

    defaults
}
