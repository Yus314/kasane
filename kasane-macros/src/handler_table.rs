//! `#[handler_table]` macro: declarative DSL for plugin handler dispatch tables.
//!
//! See `docs/handler-table-dsl.md` for the spec. Codegen sub-staging:
//!
//! - **γ-3.2.1**: parser for the full DSL grammar; codegen for erased
//!   type aliases, `HandlerTable` struct + `empty()`, and
//!   `EXPECTED_HANDLER_NAMES`.
//! - **γ-3.2.2a** *(this stage)*: `HandlerRegistry<S>` + base setter
//!   methods for the four dispatch shapes (no modifiers). Adds
//!   `Plugin`-flavoured downcast wrappers matching the existing
//!   `register_state_effect!` / `register_state_only!` / `register_view!`
//!   templates.
//! - **γ-3.2.2b**: modifier-driven setter variants
//!   (`tier1` / `tier2` / `transparent`).
//! - **γ-3.2.2c**: storage-shape modifiers
//!   (`per_slot` / `prioritized` / `void`).
//! - **γ-3.2.2d**: display-family modifiers
//!   (`unified` / `recovery` / `suppresses`).
//! - **γ-3.2.2e**: single-use modifiers
//!   (`targets` / `full_fallback` / `stateless`).
//! - **γ-3.2.3**: PluginBridge dispatch generation + parallel-impl gate.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, FnArg, Ident, ItemUse, Result, Token, Type, Visibility, braced};

// =============================================================================
// AST
// =============================================================================

pub(crate) struct HandlerTableSpec {
    pub vis: Visibility,
    pub mod_ident: Ident,
    pub items: Vec<SpecItem>,
}

pub(crate) enum SpecItem {
    Use(ItemUse),
    Handler(HandlerEntry),
    Config(ConfigEntry),
}

pub(crate) struct HandlerEntry {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub args: Punctuated<FnArg, Token![,]>,
    pub shape: Shape,
    pub modifiers: Vec<Modifier>,
}

pub(crate) enum Shape {
    Lifecycle { effect: Type },
    Observer,
    Dispatcher { command: Type },
    View { out: Type },
}

impl Shape {
    fn label(&self) -> &'static str {
        match self {
            Shape::Lifecycle { .. } => "Lifecycle",
            Shape::Observer => "Observer",
            Shape::Dispatcher { .. } => "Dispatcher",
            Shape::View { .. } => "View",
        }
    }
}

// Modifier payload fields (`key` / `value` / `names` / `ty`) hold parsed AST
// data that γ-3.2.2's modifier codegen consumes. They are intentionally dead
// in γ-3.2.1; the `allow` keeps the validation+round-trip path complete.
#[allow(dead_code)]
pub(crate) enum Modifier {
    Tier1(Ident),
    Tier2(Ident),
    Transparent(Ident),
    Void(Ident),
    PerSlot { kw: Ident, key: Type },
    Prioritized(Ident),
    Unified(Ident),
    Recovery(Ident),
    FullFallback(Ident),
    Stateless(Ident),
    Default { kw: Ident, value: Expr },
    Suppresses { kw: Ident, names: Vec<Ident> },
    Targets { kw: Ident, ty: Type },
}

impl Modifier {
    fn span_ident(&self) -> &Ident {
        match self {
            Modifier::Tier1(i)
            | Modifier::Tier2(i)
            | Modifier::Transparent(i)
            | Modifier::Void(i)
            | Modifier::Prioritized(i)
            | Modifier::Unified(i)
            | Modifier::Recovery(i)
            | Modifier::FullFallback(i)
            | Modifier::Stateless(i) => i,
            Modifier::PerSlot { kw, .. }
            | Modifier::Default { kw, .. }
            | Modifier::Suppresses { kw, .. }
            | Modifier::Targets { kw, .. } => kw,
        }
    }
}

pub(crate) struct ConfigEntry {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub ty: Type,
    pub default: Option<Expr>,
}

// =============================================================================
// Parsing
// =============================================================================

impl Parse for HandlerTableSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let _outer_attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        let _mod_token: Token![mod] = input.parse()?;
        let mod_ident: Ident = input.parse()?;

        let body;
        braced!(body in input);

        let mut items = Vec::new();
        while !body.is_empty() {
            items.push(body.parse()?);
        }
        Ok(HandlerTableSpec {
            vis,
            mod_ident,
            items,
        })
    }
}

impl Parse for SpecItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;

        if input.peek(Token![use]) {
            if !attrs.is_empty() {
                return Err(syn::Error::new_spanned(
                    &attrs[0],
                    "doc-comments / attributes on `use` items inside `#[handler_table]` are not supported",
                ));
            }
            let item: ItemUse = input.parse()?;
            return Ok(SpecItem::Use(item));
        }

        let lookahead: Ident = input.fork().parse().map_err(|_| {
            syn::Error::new(
                input.span(),
                "expected `handler …;`, `config …;`, or `use …;`",
            )
        })?;
        let kind = lookahead.to_string();
        match kind.as_str() {
            "handler" => parse_handler(input, attrs).map(SpecItem::Handler),
            "config" => parse_config(input, attrs).map(SpecItem::Config),
            other => Err(syn::Error::new(
                lookahead.span(),
                format!(
                    "expected `handler` or `config` keyword inside `#[handler_table]`, got `{other}`"
                ),
            )),
        }
    }
}

fn parse_handler(input: ParseStream, attrs: Vec<Attribute>) -> Result<HandlerEntry> {
    let _kw: Ident = input.parse()?; // "handler"
    let name: Ident = input.parse()?;

    let arg_content;
    syn::parenthesized!(arg_content in input);
    let args: Punctuated<FnArg, Token![,]> =
        arg_content.parse_terminated(FnArg::parse, Token![,])?;

    let _colon: Token![:] = input.parse()?;
    let shape = parse_shape(input)?;
    let modifiers = parse_optional_modifiers(input)?;
    let _semi: Token![;] = input.parse()?;

    if name == "state" || name == "app" {
        return Err(syn::Error::new(
            name.span(),
            format!(
                "`{name}` is reserved (state slot / AppView arg) and cannot be used as a handler name"
            ),
        ));
    }
    for arg in &args {
        if let FnArg::Typed(pt) = arg
            && let syn::Pat::Ident(id) = &*pt.pat
            && (id.ident == "state")
        {
            return Err(syn::Error::new(
                id.ident.span(),
                "`state` is reserved as the implicit first arg of every handler",
            ));
        }
    }

    Ok(HandlerEntry {
        attrs,
        name,
        args,
        shape,
        modifiers,
    })
}

fn parse_shape(input: ParseStream) -> Result<Shape> {
    let ident: Ident = input.parse()?;
    let label = ident.to_string();
    match label.as_str() {
        "Lifecycle" => {
            let _lt: Token![<] = input.parse()?;
            let effect: Type = input.parse()?;
            let _gt: Token![>] = input.parse()?;
            Ok(Shape::Lifecycle { effect })
        }
        "Observer" => Ok(Shape::Observer),
        "Dispatcher" => {
            let _lt: Token![<] = input.parse()?;
            let command: Type = input.parse()?;
            let _gt: Token![>] = input.parse()?;
            Ok(Shape::Dispatcher { command })
        }
        "View" => {
            let _lt: Token![<] = input.parse()?;
            let out: Type = input.parse()?;
            let _gt: Token![>] = input.parse()?;
            Ok(Shape::View { out })
        }
        other => Err(syn::Error::new(
            ident.span(),
            format!(
                "unknown dispatch shape `{other}`. Expected one of: Lifecycle<E>, Observer, Dispatcher<C>, View<Out>"
            ),
        )),
    }
}

fn parse_optional_modifiers(input: ParseStream) -> Result<Vec<Modifier>> {
    if !input.peek(syn::token::Paren) {
        return Ok(Vec::new());
    }
    let paren_content;
    syn::parenthesized!(paren_content in input);
    let raw: Punctuated<Modifier, Token![,]> =
        paren_content.parse_terminated(Modifier::parse, Token![,])?;
    Ok(raw.into_iter().collect())
}

impl Parse for Modifier {
    fn parse(input: ParseStream) -> Result<Self> {
        let kw: Ident = input.parse()?;
        let label = kw.to_string();
        match label.as_str() {
            "tier1" => Ok(Modifier::Tier1(kw)),
            "tier2" => Ok(Modifier::Tier2(kw)),
            "transparent" => Ok(Modifier::Transparent(kw)),
            "void" => Ok(Modifier::Void(kw)),
            "prioritized" => Ok(Modifier::Prioritized(kw)),
            "unified" => Ok(Modifier::Unified(kw)),
            "recovery" => Ok(Modifier::Recovery(kw)),
            "full_fallback" => Ok(Modifier::FullFallback(kw)),
            "stateless" => Ok(Modifier::Stateless(kw)),
            "per_slot" => {
                let _eq: Token![=] = input.parse()?;
                let key: Type = input.parse()?;
                Ok(Modifier::PerSlot { kw, key })
            }
            "default" => {
                let _eq: Token![=] = input.parse()?;
                let value: Expr = input.parse()?;
                Ok(Modifier::Default { kw, value })
            }
            "suppresses" => {
                let _eq: Token![=] = input.parse()?;
                let bracketed;
                syn::bracketed!(bracketed in input);
                let names: Punctuated<Ident, Token![,]> =
                    bracketed.parse_terminated(Ident::parse, Token![,])?;
                Ok(Modifier::Suppresses {
                    kw,
                    names: names.into_iter().collect(),
                })
            }
            "targets" => {
                let _eq: Token![=] = input.parse()?;
                let ty: Type = input.parse()?;
                Ok(Modifier::Targets { kw, ty })
            }
            other => Err(syn::Error::new(
                kw.span(),
                format!(
                    "unknown modifier `{other}`. Expected one of: tier1, tier2, transparent, void, per_slot=K, prioritized, unified, recovery, full_fallback, stateless, default=…, suppresses=[…], targets=T"
                ),
            )),
        }
    }
}

fn parse_config(input: ParseStream, attrs: Vec<Attribute>) -> Result<ConfigEntry> {
    let _kw: Ident = input.parse()?; // "config"
    let name: Ident = input.parse()?;
    let _colon: Token![:] = input.parse()?;
    let ty: Type = input.parse()?;
    let default = if input.peek(Token![=]) {
        let _eq: Token![=] = input.parse()?;
        Some(input.parse::<Expr>()?)
    } else {
        None
    };
    let _semi: Token![;] = input.parse()?;
    Ok(ConfigEntry {
        attrs,
        name,
        ty,
        default,
    })
}

// =============================================================================
// Validation
// =============================================================================

fn validate_modifiers(entry: &HandlerEntry) -> Result<()> {
    for m in &entry.modifiers {
        let allowed = match (&entry.shape, m) {
            // Lifecycle: tier1, tier2, transparent
            (
                Shape::Lifecycle { .. },
                Modifier::Tier1(_) | Modifier::Tier2(_) | Modifier::Transparent(_),
            ) => true,
            // Observer: void (and per_slot for the subscribe topic-keyed Vec)
            (Shape::Observer, Modifier::Void(_) | Modifier::PerSlot { .. }) => true,
            // Dispatcher: transparent
            (Shape::Dispatcher { .. }, Modifier::Transparent(_)) => true,
            // View: per_slot, prioritized, unified, recovery, default, suppresses, targets, full_fallback, stateless
            (
                Shape::View { .. },
                Modifier::PerSlot { .. }
                | Modifier::Prioritized(_)
                | Modifier::Unified(_)
                | Modifier::Recovery(_)
                | Modifier::Default { .. }
                | Modifier::Suppresses { .. }
                | Modifier::Targets { .. }
                | Modifier::FullFallback(_)
                | Modifier::Stateless(_),
            ) => true,
            _ => false,
        };
        if !allowed {
            return Err(syn::Error::new(
                m.span_ident().span(),
                format!(
                    "modifier `{}` is not valid on shape `{}`",
                    m.span_ident(),
                    entry.shape.label()
                ),
            ));
        }
    }
    Ok(())
}

// =============================================================================
// Code generation
// =============================================================================

pub fn expand_handler_table(input: TokenStream) -> Result<TokenStream> {
    let spec: HandlerTableSpec = syn::parse2(input)?;

    let mut uses = Vec::new();
    let mut handlers = Vec::new();
    let mut configs = Vec::new();
    for item in spec.items {
        match item {
            SpecItem::Use(u) => uses.push(u),
            SpecItem::Handler(h) => {
                validate_modifiers(&h)?;
                check_modifier_codegen_supported(&h)?;
                // Validate arg patterns up front so a bad pattern errors
                // cleanly without cascading into downstream "type not in
                // scope" diagnostics from the rest of the generated module.
                arg_idents_and_types(&h)?;
                handlers.push(h);
            }
            SpecItem::Config(c) => configs.push(c),
        }
    }

    let mut alias_decls = TokenStream::new();
    let mut table_fields = TokenStream::new();
    let mut table_inits = TokenStream::new();
    let mut table_methods = TokenStream::new();
    let mut name_table = TokenStream::new();
    let mut registry_setters = TokenStream::new();
    let mut transparency_fields = TokenStream::new();

    let transparent_lifecycle: Vec<&HandlerEntry> = handlers
        .iter()
        .filter(|h| {
            matches!(h.shape, Shape::Lifecycle { .. }) && find_modifier(h, "transparent").is_some()
        })
        .collect();
    let transparent_dispatcher: Vec<&HandlerEntry> = handlers
        .iter()
        .filter(|h| {
            matches!(h.shape, Shape::Dispatcher { .. }) && find_modifier(h, "transparent").is_some()
        })
        .collect();

    let any_recovery = handlers.iter().any(is_recovery);

    for h in &handlers {
        alias_decls.extend(erased_alias_decl(h));
        alias_decls.extend(entry_struct_decl(h));
        alias_decls.extend(suppresses_const(h));
        alias_decls.extend(default_fn_decl(h));
        table_fields.extend(handler_table_field(h));
        table_inits.extend(handler_table_init(h));
        table_methods.extend(unified_predicate_method(h));
        registry_setters.extend(registry_setters_for_entry(h));

        let name_lit = h.name.to_string();
        name_table.extend(quote! { #name_lit, });
    }

    for c in &configs {
        table_fields.extend(config_field(c));
        table_inits.extend(config_init(c));
    }

    // TransparencyFlags struct generated from the union of
    // `transparent`-marked Lifecycle and Dispatcher entries. Each entry
    // contributes one `pub <name>: bool` field; the registry setter
    // flips the flag at registration when the closure's effect/command
    // type satisfies `Transparency::IS_TRANSPARENT`.
    for h in transparent_lifecycle
        .iter()
        .chain(transparent_dispatcher.iter())
    {
        let field = format_ident!("{}", h.name);
        transparency_fields.extend(quote! {
            pub(crate) #field: bool,
        });
    }
    // Predicate body: every transparent-marked entry must be either
    // unregistered or have been registered with a transparent-typed
    // effect/command. Registered + non-transparent ⇒ overall not
    // transparent. Lifecycle and Dispatcher entries are partitioned by
    // shape (the manual `TransparencyFlags::is_all_lifecycle_transparent` /
    // `is_all_input_transparent` split).
    let predicate_for = |entries: &[&HandlerEntry]| -> TokenStream {
        let parts: Vec<_> = entries
            .iter()
            .map(|h| {
                let field = format_ident!("{}", h.name);
                let table_field = format_ident!("{}_handler", h.name);
                quote! { (table.#table_field.is_none() || self.#field) }
            })
            .collect();
        if parts.is_empty() {
            quote! { true }
        } else {
            quote! { #(#parts)&&* }
        }
    };
    let lifecycle_predicate = predicate_for(&transparent_lifecycle);
    let input_predicate = predicate_for(&transparent_dispatcher);

    // The HandlerTable always carries a `transparency` field even when no
    // transparent entries exist — it makes downstream γ-3.3 wiring uniform
    // and the empty struct collapses to a zero-byte type.
    table_fields.extend(quote! {
        pub(crate) transparency: TransparencyFlags,
    });
    table_inits.extend(quote! {
        transparency: TransparencyFlags::default(),
    });

    let vis = &spec.vis;
    let mod_ident = &spec.mod_ident;

    // γ-3.2.2d-3: emit a local `DisplayRecoveryStatus` enum mirroring the
    // `pub(crate)` one in `kasane_core::plugin::handler_table`. Generated
    // only when at least one entry uses the `recovery` modifier — keeps
    // the spec-module surface zero-cost for non-display tables. The user
    // brings `RecoveryWitness` into scope via the spec's `use` block.
    let recovery_enum_decl = if any_recovery {
        quote! {
            /// Per-recovery-entry status flag (γ-3.2.2d-3).
            ///
            /// Mirrors `kasane_core::plugin::handler_table::DisplayRecoveryStatus`
            /// — generated locally because the manual enum is `pub(crate)`
            /// inside `kasane-core` and not reachable from external spec
            /// modules. γ-3.2.3's parallel-impl gate asserts structural
            /// equivalence with the manual enum.
            #[allow(dead_code)]
            pub(crate) enum DisplayRecoveryStatus {
                NotRegistered,
                NonDestructive,
                Witnessed(RecoveryWitness),
                Unwitnessed,
            }

            impl ::core::default::Default for DisplayRecoveryStatus {
                fn default() -> Self {
                    DisplayRecoveryStatus::NotRegistered
                }
            }
        }
    } else {
        TokenStream::new()
    };

    Ok(quote! {
        #vis mod #mod_ident {
            #(#uses)*
            #recovery_enum_decl
            #alias_decls

            /// Auto-generated handler table.
            #[allow(dead_code)]
            pub(crate) struct HandlerTable {
                #table_fields
            }

            #[allow(dead_code)]
            impl HandlerTable {
                pub(crate) fn empty() -> Self {
                    Self {
                        #table_inits
                    }
                }

                #table_methods
            }

            /// Auto-generated transparency tracking. One bool per
            /// `transparent`-marked handler entry; the registry setters
            /// flip the flag on registration when the closure's effect
            /// type satisfies `Transparency`.
            #[derive(::core::default::Default)]
            #[allow(dead_code)]
            pub(crate) struct TransparencyFlags {
                #transparency_fields
            }

            #[allow(dead_code)]
            impl TransparencyFlags {
                pub(crate) fn is_all_lifecycle_transparent(&self, table: &HandlerTable) -> bool {
                    let _ = table;
                    #lifecycle_predicate
                }

                pub(crate) fn is_all_input_transparent(&self, table: &HandlerTable) -> bool {
                    let _ = table;
                    #input_predicate
                }

                pub(crate) fn is_fully_transparent(&self, table: &HandlerTable) -> bool {
                    self.is_all_lifecycle_transparent(table)
                        && self.is_all_input_transparent(table)
                }
            }

            /// Canonical handler name list — replaces the hand-written
            /// `EXPECTED_HANDLER_NAMES` const in `plugin_bridge.rs`.
            #[allow(dead_code)]
            pub(crate) const EXPECTED_HANDLER_NAMES: &[&str] = &[
                #name_table
            ];

            /// Auto-generated typed registration builder.
            ///
            /// Mirrors `kasane_core::plugin::HandlerRegistry`. The `S` generic
            /// is the plugin's concrete state type; setter closures receive
            /// `&S` and return shape-appropriate tuples that the wrapper
            /// boxes / lifts into the type-erased table.
            // γ-3.3c-5a: `pub` (not `pub(crate)`) because the manual
            // `HandlerRegistry` wrapper exposes a `Deref<Target = Self>`
            // impl as part of its `pub` interface, and Rust forbids a
            // public `Deref` from leaking a crate-private target. The
            // setter methods retain their crate-internal visibility.
            #[allow(dead_code)]
            pub struct HandlerRegistry<S: PluginState + ::core::clone::Clone + 'static> {
                /// `pub(crate)` so the manual `HandlerRegistry`
                /// wrapper can route field-level setter writes through
                /// `self.table.<field>` after auto-derefing from the
                /// wrapper to the inner generated registry.
                pub(crate) table: HandlerTable,
                _phantom: ::core::marker::PhantomData<S>,
            }

            #[allow(dead_code)]
            impl<S: PluginState + ::core::clone::Clone + 'static> HandlerRegistry<S> {
                pub(crate) fn new() -> Self {
                    Self {
                        table: HandlerTable::empty(),
                        _phantom: ::core::marker::PhantomData,
                    }
                }

                pub(crate) fn into_table(self) -> HandlerTable {
                    self.table
                }

                #registry_setters
            }
        }
    })
}

fn check_modifier_codegen_supported(entry: &HandlerEntry) -> Result<()> {
    for m in &entry.modifiers {
        let supported = matches!(
            (&entry.shape, m),
            // γ-3.2.2b: tier1 / tier2 / transparent on Lifecycle.
            (
                Shape::Lifecycle { .. },
                Modifier::Tier1(_) | Modifier::Tier2(_) | Modifier::Transparent(_),
            )
            // γ-3.2.2c: per_slot / prioritized on View.
            // γ-3.2.2d-1: per_slot extended to Observer (subscribe pattern).
            | (
                Shape::View { .. } | Shape::Observer,
                Modifier::PerSlot { .. },
            )
            | (Shape::View { .. }, Modifier::Prioritized(_))
            // γ-3.2.2d-1: void on Observer (shutdown pattern).
            | (Shape::Observer, Modifier::Void(_))
            // γ-3.2.2d-2: unified + suppresses on View (annotate_line / unified_display).
            | (
                Shape::View { .. },
                Modifier::Unified(_) | Modifier::Suppresses { .. },
            )
            // γ-3.2.2d-3: recovery on View (display family).
            | (Shape::View { .. }, Modifier::Recovery(_))
            // γ-3.2.2d-4: transparent on Dispatcher (Vec<Command> → Vec<C> rewrite).
            | (Shape::Dispatcher { .. }, Modifier::Transparent(_))
            // γ-3.2.2e-1: default=expr / stateless on View.
            | (
                Shape::View { .. },
                Modifier::Default { .. } | Modifier::Stateless(_),
            )
            // γ-3.2.2e-2: targets=T on View (transform metadata).
            | (Shape::View { .. }, Modifier::Targets { .. })
        );
        if !supported {
            // γ-3.2.2e-2: full_fallback is a transform-bespoke modifier.
            // The on_transform_full setter has a unique TransformSubject
            // signature that does not fit the macro's generic shape model;
            // the spec doc §9 lists transform as a carve-out — the macro
            // generates the entry struct and the standard handler/priority
            // setter, but the full_handler companion stays hand-written.
            if matches!(m, Modifier::FullFallback(_)) {
                return Err(syn::Error::new(
                    m.span_ident().span(),
                    "modifier `full_fallback` is a documented carve-out (spec §9): the on_<name>_full setter \
                     uses TransformSubject which does not generalize. The macro generates the entry-struct \
                     storage so a hand-written setter can populate the `full_handler: Option<…>` companion field. \
                     Remove this modifier from the spec entry — register the full handler via a manual impl block.",
                ));
            }
            return Err(syn::Error::new(
                m.span_ident().span(),
                format!(
                    "modifier `{}` parsed but its codegen is not yet wired.",
                    m.span_ident()
                ),
            ));
        }
    }
    Ok(())
}

fn is_void(entry: &HandlerEntry) -> bool {
    find_modifier(entry, "void").is_some()
}

fn is_unified(entry: &HandlerEntry) -> bool {
    find_modifier(entry, "unified").is_some()
}

fn suppresses_list(entry: &HandlerEntry) -> Option<&[Ident]> {
    entry.modifiers.iter().find_map(|m| match m {
        Modifier::Suppresses { names, .. } => Some(names.as_slice()),
        _ => None,
    })
}

fn is_recovery(entry: &HandlerEntry) -> bool {
    find_modifier(entry, "recovery").is_some()
}

fn is_stateless(entry: &HandlerEntry) -> bool {
    find_modifier(entry, "stateless").is_some()
}

fn default_expr(entry: &HandlerEntry) -> Option<&Expr> {
    entry.modifiers.iter().find_map(|m| match m {
        Modifier::Default { value, .. } => Some(value),
        _ => None,
    })
}

fn targets_type(entry: &HandlerEntry) -> Option<&Type> {
    entry.modifiers.iter().find_map(|m| match m {
        Modifier::Targets { ty, .. } => Some(ty),
        _ => None,
    })
}

/// γ-3.2.2e-2: an entry "needs metadata storage" when it carries
/// `prioritized` or `targets=T` *without* `per_slot`. The storage shape
/// is `Option<<Name>Entry>` instead of `Option<Erased>`, with the entry
/// struct holding the metadata fields alongside the handler.
fn has_metadata_storage(entry: &HandlerEntry) -> bool {
    per_slot_key(entry).is_none() && (is_prioritized(entry) || targets_type(entry).is_some())
}

/// γ-3.2.2d-4: extract `T` from a `Vec<T>` type. Returns `None` if `ty`
/// is not a `Vec<…>` shape. Used by Dispatcher transparent codegen to
/// rewrite the user closure's element type into a generic `C` parameter.
fn extract_vec_inner(ty: &Type) -> Option<&Type> {
    let path = match ty {
        Type::Path(p) => &p.path,
        _ => return None,
    };
    let last = path.segments.last()?;
    if last.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    })
}

fn per_slot_key(entry: &HandlerEntry) -> Option<&Type> {
    entry.modifiers.iter().find_map(|m| match m {
        Modifier::PerSlot { key, .. } => Some(key),
        _ => None,
    })
}

fn is_prioritized(entry: &HandlerEntry) -> bool {
    find_modifier(entry, "prioritized").is_some()
}

fn entry_struct_ident(entry: &HandlerEntry) -> Ident {
    format_ident!("{}Entry", to_pascal_case(&entry.name.to_string()))
}

fn find_modifier<'a>(entry: &'a HandlerEntry, name: &str) -> Option<&'a Modifier> {
    entry.modifiers.iter().find(|m| m.span_ident() == name)
}

fn arg_types(entry: &HandlerEntry) -> Vec<&Type> {
    entry
        .args
        .iter()
        .filter_map(|a| match a {
            FnArg::Typed(pt) => Some(&*pt.ty),
            _ => None,
        })
        .collect()
}

/// Extract `(name, type)` pairs from the spec entry's arg list. Errors on
/// non-`Ident: Type` patterns (the macro intentionally rejects destructuring
/// patterns to keep generated wrapper closures readable).
fn arg_idents_and_types(entry: &HandlerEntry) -> Result<Vec<(Ident, &Type)>> {
    entry
        .args
        .iter()
        .map(|a| match a {
            FnArg::Typed(pt) => match &*pt.pat {
                syn::Pat::Ident(id) => Ok((id.ident.clone(), &*pt.ty)),
                other => Err(syn::Error::new_spanned(
                    other,
                    "handler arg patterns must be simple `name: Type` — destructuring patterns are not supported",
                )),
            },
            FnArg::Receiver(r) => Err(syn::Error::new_spanned(
                r,
                "handler args cannot include `self` — `&S` (state) is implicit",
            )),
        })
        .collect()
}

/// Emit all setter variants for a handler entry.
///
/// γ-3.2.2a wired the four base shapes; γ-3.2.2b adds Lifecycle
/// `_tier1` / `_tier2` setter variants and a Transparency-aware base
/// setter (with `TransparencyFlags` flag-setting) when the
/// `transparent` modifier is present.
///
/// The strategy is **additive**: each modifier produces an additional
/// setter rather than replacing the base. A spec entry tagged
/// `(tier1, transparent)` therefore emits three setters:
/// `on_<name>` (Transparency-aware), `on_<name>_tier1`, `on_<name>_tier2`.
/// γ-3.3 prunes the redundant ones during the manual-code deletion pass.
fn registry_setters_for_entry(entry: &HandlerEntry) -> TokenStream {
    let mut out = TokenStream::new();
    if per_slot_key(entry).is_some() {
        // γ-3.2.2c: PerSlot setter pushes one entry per registration call;
        // the base scalar setter is replaced (a single Option<Erased> field
        // does not exist for these entries).
        out.extend(registry_setter_per_slot(entry));
        return out;
    }
    if has_metadata_storage(entry) {
        // γ-3.2.2e-2: single-entry-with-metadata setter.
        out.extend(registry_setter_metadata(entry));
        return out;
    }
    out.extend(registry_setter_base(entry));
    if matches!(entry.shape, Shape::Lifecycle { .. }) {
        if find_modifier(entry, "tier1").is_some() {
            out.extend(registry_setter_tier(
                entry,
                "tier1",
                quote! { KakouneSideEffects },
            ));
        }
        if find_modifier(entry, "tier2").is_some() {
            out.extend(registry_setter_tier(
                entry,
                "tier2",
                quote! { ProcessCapableEffects },
            ));
        }
    }
    // γ-3.2.2d-3: recovery entries get `_safe` and `_witnessed` companions.
    if is_recovery(entry) && matches!(entry.shape, Shape::View { .. }) {
        out.extend(registry_setter_safe(entry));
        out.extend(registry_setter_witnessed(entry));
    }
    out
}

/// γ-3.2.2e-2: emit a setter for the single-entry-with-metadata storage
/// shape (`prioritized` and/or `targets=T` without `per_slot`). The
/// setter signature is `pub fn on_<name>(&mut self, [priority: i16,]
/// [targets: T,] handler: F)`. Used by the `transform` entry pattern.
fn registry_setter_metadata(entry: &HandlerEntry) -> TokenStream {
    let setter_name = format_ident!("on_{}", entry.name);
    let field_ident = format_ident!("{}_handler", entry.name);
    let struct_ident = entry_struct_ident(entry);
    let arg_pairs = match arg_idents_and_types(entry) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error(),
    };
    let arg_names: Vec<&Ident> = arg_pairs.iter().map(|(n, _)| n).collect();
    let arg_tys: Vec<&Type> = arg_pairs.iter().map(|(_, t)| *t).collect();
    let attrs = &entry.attrs;
    let downcast = quote! {
        let s = state
            .as_any()
            .downcast_ref::<S>()
            .expect("plugin state type mismatch");
    };
    let prioritized = is_prioritized(entry);
    let targets = targets_type(entry);
    let prio_param = if prioritized {
        quote! { priority: i16, }
    } else {
        TokenStream::new()
    };
    let prio_init = if prioritized {
        quote! { priority, }
    } else {
        TokenStream::new()
    };
    let targets_param = match targets {
        Some(t) => quote! { targets: #t, },
        None => TokenStream::new(),
    };
    let targets_init = if targets.is_some() {
        quote! { targets, }
    } else {
        TokenStream::new()
    };
    let Shape::View { out } = &entry.shape else {
        return syn::Error::new(
            entry.name.span(),
            "single-entry-with-metadata codegen is implemented only for View shape",
        )
        .to_compile_error();
    };
    quote! {
        #(#attrs)*
        pub fn #setter_name<F>(&mut self, #prio_param #targets_param handler: F)
        where
            F: ::core::ops::Fn(&S #(, #arg_tys)*) -> #out
                + ::core::marker::Send + ::core::marker::Sync + 'static,
        {
            let erased = ::std::boxed::Box::new(move |state: &dyn PluginState #(, #arg_names: #arg_tys)*| {
                #downcast
                handler(s #(, #arg_names)*)
            });
            self.table.#field_ident = ::core::option::Option::Some(#struct_ident {
                #prio_init
                #targets_init
                handler: erased,
            });
        }
    }
}

/// γ-3.2.2c: emit a `pub fn on_<name>(&mut self, key: K[, priority: i16],
/// handler: F)` that pushes one `<Name>Entry` into the table's
/// `<name>_handlers: Vec<<Name>Entry>` storage.
///
/// Currently restricted to View shape; Observer per_slot (`subscribe`'s
/// per-topic handler) lands in γ-3.2.2d.
fn registry_setter_per_slot(entry: &HandlerEntry) -> TokenStream {
    let setter_name = format_ident!("on_{}", entry.name);
    let field_ident = format_ident!("{}_handlers", entry.name);
    let struct_ident = entry_struct_ident(entry);
    let key_ty = match per_slot_key(entry) {
        Some(t) => t,
        None => unreachable!("registry_setter_per_slot called on non-per_slot entry"),
    };
    let arg_pairs = match arg_idents_and_types(entry) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error(),
    };
    let arg_names: Vec<&Ident> = arg_pairs.iter().map(|(n, _)| n).collect();
    let arg_tys: Vec<&Type> = arg_pairs.iter().map(|(_, t)| *t).collect();
    let attrs = &entry.attrs;
    let downcast = quote! {
        let s = state
            .as_any()
            .downcast_ref::<S>()
            .expect("plugin state type mismatch");
    };
    let prioritized = is_prioritized(entry);
    let prio_param = if prioritized {
        quote! { , priority: i16 }
    } else {
        TokenStream::new()
    };
    let prio_init = if prioritized {
        quote! { priority, }
    } else {
        TokenStream::new()
    };
    // γ-3.3c-3: per_slot+recovery (currently only `projection`) takes a
    // `recovery: DisplayRecoveryStatus` argument and stores it on the
    // per-entry struct. This is the lower-level building block; the
    // bespoke `define_projection` carve-out (spec §9.1) wraps this
    // setter and derives recovery from the descriptor's category.
    let recovery_param = if is_recovery(entry) {
        quote! { , recovery: DisplayRecoveryStatus }
    } else {
        TokenStream::new()
    };
    let recovery_init = if is_recovery(entry) {
        quote! { recovery, }
    } else {
        TokenStream::new()
    };

    match &entry.shape {
        Shape::View { out } => quote! {
            #(#attrs)*
            pub fn #setter_name<F>(&mut self, key: #key_ty #prio_param #recovery_param, handler: F)
            where
                F: ::core::ops::Fn(&S #(, #arg_tys)*) -> #out
                    + ::core::marker::Send + ::core::marker::Sync + 'static,
            {
                let erased = ::std::boxed::Box::new(move |state: &dyn PluginState #(, #arg_names: #arg_tys)*| {
                    #downcast
                    handler(s #(, #arg_names)*)
                });
                self.table.#field_ident.push(#struct_ident {
                    key,
                    #prio_init
                    handler: erased,
                    #recovery_init
                });
            }
        },
        // γ-3.2.2d-1: Observer per_slot (subscribe per-topic-handler pattern).
        // The wrapper boxes the handler's `S` return into the type-erased
        // `Box<dyn PluginState>` slot.
        Shape::Observer => quote! {
            #(#attrs)*
            pub fn #setter_name<F>(&mut self, key: #key_ty #prio_param, handler: F)
            where
                F: ::core::ops::Fn(&S #(, #arg_tys)*) -> S
                    + ::core::marker::Send + ::core::marker::Sync + 'static,
            {
                let erased = ::std::boxed::Box::new(move |state: &dyn PluginState #(, #arg_names: #arg_tys)*| {
                    #downcast
                    ::std::boxed::Box::new(handler(s #(, #arg_names)*))
                        as ::std::boxed::Box<dyn PluginState>
                });
                self.table.#field_ident.push(#struct_ident {
                    key,
                    #prio_init
                    handler: erased,
                });
            }
        },
        _ => syn::Error::new(
            entry.name.span(),
            "per_slot codegen is implemented for View and Observer; other shapes are not yet supported",
        )
        .to_compile_error(),
    }
}

fn registry_setter_base(entry: &HandlerEntry) -> TokenStream {
    let setter_name = format_ident!("on_{}", entry.name);
    let field_ident = format_ident!("{}_handler", entry.name);

    let arg_pairs = match arg_idents_and_types(entry) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error(),
    };
    let arg_names: Vec<&Ident> = arg_pairs.iter().map(|(n, _)| n).collect();
    let arg_tys: Vec<&Type> = arg_pairs.iter().map(|(_, t)| *t).collect();
    let attrs = &entry.attrs;
    let transparent = matches!(entry.shape, Shape::Lifecycle { .. })
        && find_modifier(entry, "transparent").is_some();

    let downcast = quote! {
        let s = state
            .as_any()
            .downcast_ref::<S>()
            .expect("plugin state type mismatch");
    };

    match &entry.shape {
        Shape::Lifecycle { effect } if transparent => {
            // γ-3.2.2b: Transparency-aware base setter.
            //
            // Bound `E: Into<#effect> + Transparency` mirrors the manual
            // `on_command_error` / `on_subscription` pattern in
            // `handler_registry/lifecycle.rs`. The runtime checks
            // `E::IS_TRANSPARENT` and flips the corresponding
            // `TransparencyFlags` bit on registration.
            let trans_field = format_ident!("{}", entry.name);
            quote! {
                #(#attrs)*
                pub fn #setter_name<F, E>(&mut self, handler: F)
                where
                    E: ::core::convert::Into<#effect> + Transparency + 'static,
                    F: ::core::ops::Fn(&S #(, #arg_tys)*) -> (S, E)
                        + ::core::marker::Send + ::core::marker::Sync + 'static,
                {
                    self.table.#field_ident = ::core::option::Option::Some(
                        ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                            #downcast
                            let (new_state, effects) = handler(s #(, #arg_names)*);
                            (
                                ::std::boxed::Box::new(new_state) as ::std::boxed::Box<dyn PluginState>,
                                effects.into(),
                            )
                        })
                    );
                    if <E as Transparency>::IS_TRANSPARENT {
                        self.table.transparency.#trans_field = true;
                    }
                }
            }
        }
        Shape::Lifecycle { effect } => quote! {
            #(#attrs)*
            pub fn #setter_name<F>(&mut self, handler: F)
            where
                F: ::core::ops::Fn(&S #(, #arg_tys)*) -> (S, #effect)
                    + ::core::marker::Send + ::core::marker::Sync + 'static,
            {
                self.table.#field_ident = ::core::option::Option::Some(
                    ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                        #downcast
                        let (new_state, effects) = handler(s #(, #arg_names)*);
                        (
                            ::std::boxed::Box::new(new_state) as ::std::boxed::Box<dyn PluginState>,
                            effects,
                        )
                    })
                );
            }
        },
        Shape::Observer if is_void(entry) => quote! {
            #(#attrs)*
            pub fn #setter_name<F>(&mut self, handler: F)
            where
                F: ::core::ops::Fn(&S #(, #arg_tys)*)
                    + ::core::marker::Send + ::core::marker::Sync + 'static,
            {
                self.table.#field_ident = ::core::option::Option::Some(
                    ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                        #downcast
                        handler(s #(, #arg_names)*);
                    })
                );
            }
        },
        Shape::Observer => quote! {
            #(#attrs)*
            pub fn #setter_name<F>(&mut self, handler: F)
            where
                F: ::core::ops::Fn(&S #(, #arg_tys)*) -> S
                    + ::core::marker::Send + ::core::marker::Sync + 'static,
            {
                self.table.#field_ident = ::core::option::Option::Some(
                    ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                        #downcast
                        ::std::boxed::Box::new(handler(s #(, #arg_names)*))
                            as ::std::boxed::Box<dyn PluginState>
                    })
                );
            }
        },
        Shape::Dispatcher { command } if find_modifier(entry, "transparent").is_some() => {
            // γ-3.2.2d-4: Dispatcher transparent rewrites the closure's
            // command type from `Vec<T>` to `Vec<C>` where `C: Into<T> +
            // Transparency`. The wrapper does `cmds.into_iter().map(Into::into)
            // .collect()` to lift back to the table's `Vec<T>` slot.
            let trans_field = format_ident!("{}", entry.name);
            let inner = match extract_vec_inner(command) {
                Some(t) => t,
                None => {
                    return syn::Error::new_spanned(
                        command,
                        "Dispatcher(transparent) requires the command type to be `Vec<T>` (the inner T is what the generic C bound rewrites to)",
                    )
                    .to_compile_error();
                }
            };
            quote! {
                #(#attrs)*
                pub fn #setter_name<F, C>(&mut self, handler: F)
                where
                    C: ::core::convert::Into<#inner> + Transparency + 'static,
                    F: ::core::ops::Fn(&S #(, #arg_tys)*)
                        -> ::core::option::Option<(S, ::std::vec::Vec<C>)>
                        + ::core::marker::Send + ::core::marker::Sync + 'static,
                {
                    self.table.#field_ident = ::core::option::Option::Some(
                        ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                            #downcast
                            handler(s #(, #arg_names)*).map(|(new_state, cmds)| {
                                (
                                    ::std::boxed::Box::new(new_state) as ::std::boxed::Box<dyn PluginState>,
                                    cmds.into_iter().map(::core::convert::Into::into).collect(),
                                )
                            })
                        })
                    );
                    if <C as Transparency>::IS_TRANSPARENT {
                        self.table.transparency.#trans_field = true;
                    }
                }
            }
        }
        Shape::Dispatcher { command } => quote! {
            #(#attrs)*
            pub fn #setter_name<F>(&mut self, handler: F)
            where
                F: ::core::ops::Fn(&S #(, #arg_tys)*) -> ::core::option::Option<(S, #command)>
                    + ::core::marker::Send + ::core::marker::Sync + 'static,
            {
                self.table.#field_ident = ::core::option::Option::Some(
                    ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                        #downcast
                        handler(s #(, #arg_names)*).map(|(new_state, cmds)| {
                            (
                                ::std::boxed::Box::new(new_state) as ::std::boxed::Box<dyn PluginState>,
                                cmds,
                            )
                        })
                    })
                );
            }
        },
        Shape::View { out } if is_stateless(entry) => {
            // γ-3.2.2e-1: stateless View entries (lenses-style factory)
            // drop the `&S` arg from both the closure type and the
            // wrapper. No downcast happens.
            quote! {
                #(#attrs)*
                pub fn #setter_name<F>(&mut self, handler: F)
                where
                    F: ::core::ops::Fn(#(#arg_tys),*) -> #out
                        + ::core::marker::Send + ::core::marker::Sync + 'static,
                {
                    self.table.#field_ident = ::core::option::Option::Some(
                        ::std::boxed::Box::new(move |#(#arg_names: #arg_tys),*| {
                            handler(#(#arg_names),*)
                        })
                    );
                }
            }
        }
        Shape::View { out } => {
            // γ-3.2.2d-3: recovery entries flag their base setter as
            // `Unwitnessed` (the imperative path that may emit `Hide`
            // without recovery evidence).
            let recovery_assign = if is_recovery(entry) {
                let r_field = format_ident!("{}_recovery", entry.name);
                quote! {
                    self.table.#r_field = DisplayRecoveryStatus::Unwitnessed;
                }
            } else {
                TokenStream::new()
            };
            quote! {
                #(#attrs)*
                pub fn #setter_name<F>(&mut self, handler: F)
                where
                    F: ::core::ops::Fn(&S #(, #arg_tys)*) -> #out
                        + ::core::marker::Send + ::core::marker::Sync + 'static,
                {
                    self.table.#field_ident = ::core::option::Option::Some(
                        ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                            #downcast
                            handler(s #(, #arg_names)*)
                        })
                    );
                    #recovery_assign
                }
            }
        }
    }
}

/// γ-3.2.2d-3: emit `pub fn on_<name>_safe(handler)` accepting closures
/// returning `Vec<SafeDisplayDirective>`. The wrapper lifts each safe
/// directive into a `DisplayDirective` (via the `From<SafeDisplayDirective>
/// for DisplayDirective` impl) and stores in the existing
/// `Vec<DisplayDirective>` slot. Recovery status flips to `NonDestructive`.
fn registry_setter_safe(entry: &HandlerEntry) -> TokenStream {
    let setter_name = format_ident!("on_{}_safe", entry.name);
    let field_ident = format_ident!("{}_handler", entry.name);
    let recovery_ident = format_ident!("{}_recovery", entry.name);
    let arg_pairs = match arg_idents_and_types(entry) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error(),
    };
    let arg_names: Vec<&Ident> = arg_pairs.iter().map(|(n, _)| n).collect();
    let arg_tys: Vec<&Type> = arg_pairs.iter().map(|(_, t)| *t).collect();
    let attrs = &entry.attrs;
    let downcast = quote! {
        let s = state
            .as_any()
            .downcast_ref::<S>()
            .expect("plugin state type mismatch");
    };
    quote! {
        #(#attrs)*
        pub fn #setter_name<F>(&mut self, handler: F)
        where
            F: ::core::ops::Fn(&S #(, #arg_tys)*) -> ::std::vec::Vec<SafeDisplayDirective>
                + ::core::marker::Send + ::core::marker::Sync + 'static,
        {
            self.table.#field_ident = ::core::option::Option::Some(
                ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                    #downcast
                    handler(s #(, #arg_names)*)
                        .into_iter()
                        .map(::core::convert::Into::into)
                        .collect()
                })
            );
            self.table.#recovery_ident = DisplayRecoveryStatus::NonDestructive;
        }
    }
}

/// γ-3.2.2d-3: emit `pub fn on_<name>_witnessed(witness, handler)`. The
/// caller supplies a `RecoveryWitness` describing how `Hide` directives
/// remain recoverable; the macro stores it inline on the
/// `<name>_recovery` field so the framework's compliance audit can later
/// inspect it.
fn registry_setter_witnessed(entry: &HandlerEntry) -> TokenStream {
    let setter_name = format_ident!("on_{}_witnessed", entry.name);
    let field_ident = format_ident!("{}_handler", entry.name);
    let recovery_ident = format_ident!("{}_recovery", entry.name);
    let arg_pairs = match arg_idents_and_types(entry) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error(),
    };
    let arg_names: Vec<&Ident> = arg_pairs.iter().map(|(n, _)| n).collect();
    let arg_tys: Vec<&Type> = arg_pairs.iter().map(|(_, t)| *t).collect();
    let attrs = &entry.attrs;
    let out = match &entry.shape {
        Shape::View { out } => out,
        _ => unreachable!("registry_setter_witnessed called on non-View entry"),
    };
    let downcast = quote! {
        let s = state
            .as_any()
            .downcast_ref::<S>()
            .expect("plugin state type mismatch");
    };
    quote! {
        #(#attrs)*
        pub fn #setter_name<F>(&mut self, witness: RecoveryWitness, handler: F)
        where
            F: ::core::ops::Fn(&S #(, #arg_tys)*) -> #out
                + ::core::marker::Send + ::core::marker::Sync + 'static,
        {
            self.table.#field_ident = ::core::option::Option::Some(
                ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                    #downcast
                    handler(s #(, #arg_names)*)
                })
            );
            self.table.#recovery_ident = DisplayRecoveryStatus::Witnessed(witness);
        }
    }
}

/// Emit a tier-narrowed Lifecycle setter (`on_<name>_tier1` /
/// `on_<name>_tier2`).
///
/// `tier_suffix` is the literal string appended to the setter name
/// (`"tier1"` or `"tier2"`); `tier_type` names the narrowing type
/// (`KakouneSideEffects` / `ProcessCapableEffects`). The wrapper performs
/// a double-`Into` lift through the tier type to `Effects` so the table
/// boundary stores untyped effects regardless of the user's tier choice.
fn registry_setter_tier(
    entry: &HandlerEntry,
    tier_suffix: &str,
    tier_type: TokenStream,
) -> TokenStream {
    let setter_name = format_ident!("on_{}_{}", entry.name, tier_suffix);
    let field_ident = format_ident!("{}_handler", entry.name);
    let arg_pairs = match arg_idents_and_types(entry) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error(),
    };
    let arg_names: Vec<&Ident> = arg_pairs.iter().map(|(n, _)| n).collect();
    let arg_tys: Vec<&Type> = arg_pairs.iter().map(|(_, t)| *t).collect();
    let attrs = &entry.attrs;
    let downcast = quote! {
        let s = state
            .as_any()
            .downcast_ref::<S>()
            .expect("plugin state type mismatch");
    };
    quote! {
        #(#attrs)*
        pub fn #setter_name<F, E>(&mut self, handler: F)
        where
            E: ::core::convert::Into<#tier_type> + 'static,
            F: ::core::ops::Fn(&S #(, #arg_tys)*) -> (S, E)
                + ::core::marker::Send + ::core::marker::Sync + 'static,
        {
            self.table.#field_ident = ::core::option::Option::Some(
                ::std::boxed::Box::new(move |state #(, #arg_names)*| {
                    #downcast
                    let (new_state, side) = handler(s #(, #arg_names)*);
                    let side: #tier_type = side.into();
                    let effects: Effects = side.into();
                    (
                        ::std::boxed::Box::new(new_state) as ::std::boxed::Box<dyn PluginState>,
                        effects,
                    )
                })
            );
        }
    }
}

fn erased_alias_decl(entry: &HandlerEntry) -> TokenStream {
    let alias_ident = format_ident!("Erased{}Handler", to_pascal_case(&entry.name.to_string()));
    let arg_tys = arg_types(entry);
    let attrs = &entry.attrs;
    match &entry.shape {
        Shape::Lifecycle { effect } => quote! {
            #(#attrs)*
            pub(crate) type #alias_ident = ::std::boxed::Box<
                dyn ::core::ops::Fn(
                    &dyn PluginState
                    #(, #arg_tys)*
                ) -> (::std::boxed::Box<dyn PluginState>, #effect)
                + ::core::marker::Send + ::core::marker::Sync,
            >;
        },
        Shape::Observer if is_void(entry) => quote! {
            #(#attrs)*
            pub(crate) type #alias_ident = ::std::boxed::Box<
                dyn ::core::ops::Fn(&dyn PluginState #(, #arg_tys)*)
                    + ::core::marker::Send + ::core::marker::Sync,
            >;
        },
        Shape::Observer => quote! {
            #(#attrs)*
            pub(crate) type #alias_ident = ::std::boxed::Box<
                dyn ::core::ops::Fn(
                    &dyn PluginState
                    #(, #arg_tys)*
                ) -> ::std::boxed::Box<dyn PluginState>
                + ::core::marker::Send + ::core::marker::Sync,
            >;
        },
        Shape::Dispatcher { command } => quote! {
            #(#attrs)*
            pub(crate) type #alias_ident = ::std::boxed::Box<
                dyn ::core::ops::Fn(
                    &dyn PluginState
                    #(, #arg_tys)*
                ) -> ::core::option::Option<(::std::boxed::Box<dyn PluginState>, #command)>
                + ::core::marker::Send + ::core::marker::Sync,
            >;
        },
        Shape::View { out } if is_stateless(entry) => quote! {
            #(#attrs)*
            pub(crate) type #alias_ident = ::std::boxed::Box<
                dyn ::core::ops::Fn(#(#arg_tys),*) -> #out
                    + ::core::marker::Send + ::core::marker::Sync,
            >;
        },
        Shape::View { out } => quote! {
            #(#attrs)*
            pub(crate) type #alias_ident = ::std::boxed::Box<
                dyn ::core::ops::Fn(
                    &dyn PluginState
                    #(, #arg_tys)*
                ) -> #out
                + ::core::marker::Send + ::core::marker::Sync,
            >;
        },
    }
}

/// γ-3.2.2e-1: emit `pub(crate) fn default_<name>() -> <Out>` returning
/// the View dispatch fallback value declared via `default = expr`. Returns
/// empty for entries without a `default=…` modifier.
fn default_fn_decl(entry: &HandlerEntry) -> TokenStream {
    let Some(expr) = default_expr(entry) else {
        return TokenStream::new();
    };
    let Shape::View { out } = &entry.shape else {
        return TokenStream::new();
    };
    let fn_ident = format_ident!("default_{}", entry.name);
    quote! {
        #[allow(dead_code)]
        pub(crate) fn #fn_ident() -> #out {
            #expr
        }
    }
}

fn handler_table_field(entry: &HandlerEntry) -> TokenStream {
    let mut out = if per_slot_key(entry).is_some() {
        // γ-3.2.2c: PerSlot storage uses Vec<Entry> with the plural
        // `_handlers` suffix matching the manual `contribute_handlers` /
        // `gutter_handlers` convention.
        let field_ident = format_ident!("{}_handlers", entry.name);
        let entry_struct = entry_struct_ident(entry);
        quote! {
            pub(crate) #field_ident: ::std::vec::Vec<#entry_struct>,
        }
    } else if has_metadata_storage(entry) {
        // γ-3.2.2e-2: single-entry storage carrying metadata
        // (priority / targets) alongside the handler. Used by `transform`.
        let field_ident = format_ident!("{}_handler", entry.name);
        let entry_struct = entry_struct_ident(entry);
        quote! {
            pub(crate) #field_ident: ::core::option::Option<#entry_struct>,
        }
    } else {
        let field_ident = format_ident!("{}_handler", entry.name);
        let alias_ident = format_ident!("Erased{}Handler", to_pascal_case(&entry.name.to_string()));
        quote! {
            pub(crate) #field_ident: ::core::option::Option<#alias_ident>,
        }
    };
    if is_recovery(entry) && per_slot_key(entry).is_none() {
        // γ-3.2.2d-3: per-recovery-entry status field for singleton
        // entries. Default `NotRegistered`; setter variants flip to
        // NonDestructive / Witnessed / Unwitnessed at registration time.
        //
        // γ-3.3c-3: `per_slot + recovery` entries (currently only
        // `projection`) carry recovery on the per-entry struct instead
        // of at table level, since each per-slot registration can have
        // a different recovery hint (e.g. `define_projection` derives
        // it from the descriptor's `Structural` / `Additive` category).
        let recovery_ident = format_ident!("{}_recovery", entry.name);
        out.extend(quote! {
            pub(crate) #recovery_ident: DisplayRecoveryStatus,
        });
    }
    out
}

fn handler_table_init(entry: &HandlerEntry) -> TokenStream {
    let mut out = if per_slot_key(entry).is_some() {
        let field_ident = format_ident!("{}_handlers", entry.name);
        quote! {
            #field_ident: ::std::vec::Vec::new(),
        }
    } else {
        let field_ident = format_ident!("{}_handler", entry.name);
        quote! {
            #field_ident: ::core::option::Option::None,
        }
    };
    if is_recovery(entry) && per_slot_key(entry).is_none() {
        // γ-3.3c-3: only singleton recovery entries get a table-level
        // init; per_slot+recovery carries recovery on each per-entry
        // struct (initialized at registration time).
        let recovery_ident = format_ident!("{}_recovery", entry.name);
        out.extend(quote! {
            #recovery_ident: DisplayRecoveryStatus::NotRegistered,
        });
    }
    out
}

/// γ-3.2.2d-2: emit a `pub(crate) fn has_<name>(&self) -> bool` predicate
/// that the bridge consults to decide whether to dispatch via the unified
/// monolithic handler instead of the decomposed handlers it suppresses.
/// Returns empty for non-unified entries.
fn unified_predicate_method(entry: &HandlerEntry) -> TokenStream {
    if !is_unified(entry) {
        return TokenStream::new();
    }
    let method_ident = format_ident!("has_{}", entry.name);
    let field_ident = format_ident!("{}_handler", entry.name);
    let attrs = &entry.attrs;
    quote! {
        #(#attrs)*
        pub(crate) fn #method_ident(&self) -> bool {
            self.#field_ident.is_some()
        }
    }
}

/// γ-3.2.2d-2: emit a `pub(crate) const SUPPRESSED_BY_<NAME>: &[&str]`
/// listing the decomposed handler names a unified entry supersedes when
/// registered. The bridge reads this to skip the listed decomposed
/// handlers in its dispatch loop. Returns empty for entries without a
/// `suppresses=[…]` modifier.
fn suppresses_const(entry: &HandlerEntry) -> TokenStream {
    let Some(names) = suppresses_list(entry) else {
        return TokenStream::new();
    };
    let const_ident = format_ident!("SUPPRESSED_BY_{}", entry.name.to_string().to_uppercase());
    let lits: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    quote! {
        #[allow(dead_code)]
        pub(crate) const #const_ident: &[&str] = &[
            #(#lits),*
        ];
    }
}

/// Emit the `<Name>Entry` struct for entries with metadata-bearing storage.
///
/// γ-3.2.2c emitted this only for `per_slot=K` entries (key + handler,
/// optionally + priority). γ-3.2.2e-2 extends to standalone `prioritized` /
/// `targets=T` View entries, where the storage becomes `Option<<Name>Entry>`
/// with metadata fields but no key (single-entry-with-metadata pattern,
/// used by `transform`).
///
/// Field order is stable: `key` → `priority` → `targets` → `handler` (+
/// optional `full_handler` companion when `full_fallback` is wired in
/// future work).
fn entry_struct_decl(entry: &HandlerEntry) -> TokenStream {
    let key = per_slot_key(entry);
    let metadata = has_metadata_storage(entry);
    if key.is_none() && !metadata {
        return TokenStream::new();
    }
    let struct_ident = entry_struct_ident(entry);
    let alias_ident = format_ident!("Erased{}Handler", to_pascal_case(&entry.name.to_string()));
    let key_field = match key {
        Some(k) => quote! { pub(crate) key: #k, },
        None => TokenStream::new(),
    };
    let priority_field = if is_prioritized(entry) {
        quote! { pub(crate) priority: i16, }
    } else {
        TokenStream::new()
    };
    let targets_field = match targets_type(entry) {
        Some(t) => quote! { pub(crate) targets: #t, },
        None => TokenStream::new(),
    };
    // γ-3.3c-3: per_slot+recovery entries carry recovery per-entry.
    // Singleton recovery still lives at table level (handled by
    // `handler_table_field`).
    let recovery_field = if key.is_some() && is_recovery(entry) {
        quote! { pub(crate) recovery: DisplayRecoveryStatus, }
    } else {
        TokenStream::new()
    };
    quote! {
        #[allow(dead_code)]
        pub(crate) struct #struct_ident {
            #key_field
            #priority_field
            #targets_field
            pub(crate) handler: #alias_ident,
            #recovery_field
        }
    }
}

fn config_field(entry: &ConfigEntry) -> TokenStream {
    let name = &entry.name;
    let ty = &entry.ty;
    let attrs = &entry.attrs;
    quote! {
        #(#attrs)*
        pub(crate) #name: #ty,
    }
}

fn config_init(entry: &ConfigEntry) -> TokenStream {
    let name = &entry.name;
    match &entry.default {
        Some(expr) => quote! { #name: #expr, },
        None => quote! { #name: ::core::default::Default::default(), },
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}
