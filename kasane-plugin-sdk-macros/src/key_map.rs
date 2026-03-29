use quote::quote;

// ---------------------------------------------------------------------------
// Key map DSL types
// ---------------------------------------------------------------------------

pub(crate) struct KeyMapDef {
    pub(crate) groups: Vec<WhenGroupDef>,
    pub(crate) chords: Vec<ChordGroupDef>,
}

pub(crate) struct WhenGroupDef {
    pub(crate) predicate: proc_macro2::TokenStream,
    pub(crate) bindings: Vec<BindingEntry>,
}

pub(crate) struct ChordGroupDef {
    pub(crate) leader: KeyPatternExpr,
    pub(crate) bindings: Vec<BindingEntry>,
}

pub(crate) struct BindingEntry {
    pub(crate) pattern: KeyPatternExpr,
    pub(crate) action_id: syn::LitStr,
}

/// Parsed key pattern constructor, e.g. `ctrl('c')`, `key(Escape)`, `any_char()`, `any()`, `char('n')`.
pub(crate) enum KeyPatternExpr {
    Ctrl(syn::LitChar),
    Key(syn::Ident),
    Char(syn::LitChar),
    AnyChar,
    AnyCharPlain,
    Any,
}

pub(crate) struct ActionDef {
    pub(crate) id: syn::LitStr,
    pub(crate) event_param: syn::Ident,
    pub(crate) body: proc_macro2::TokenStream,
}

// ---------------------------------------------------------------------------
// Key map codegen helpers
// ---------------------------------------------------------------------------

/// Convert a `KeyPatternExpr` into tokens that construct a WIT `KeyPattern`.
fn key_pattern_expr_to_tokens(pat: &KeyPatternExpr) -> proc_macro2::TokenStream {
    match pat {
        KeyPatternExpr::Ctrl(ch) => {
            let c = ch.value();
            // Ctrl+char: emit Exact(KeyEvent { key: Char(c as u32), modifiers: CTRL })
            let cp = c as u32;
            quote! {
                KeyPattern {
                    kind: KeyPatternKind::Exact(KeyEvent {
                        key: KeyCode::Char(#cp),
                        modifiers: kasane_plugin_sdk::modifiers::CTRL,
                    }),
                }
            }
        }
        KeyPatternExpr::Key(ident) => {
            // Well-known key name: Escape, Enter, Up, Down, etc.
            quote! {
                KeyPattern {
                    kind: KeyPatternKind::Exact(KeyEvent {
                        key: KeyCode::#ident,
                        modifiers: 0,
                    }),
                }
            }
        }
        KeyPatternExpr::Char(ch) => {
            let cp = ch.value() as u32;
            quote! {
                KeyPattern {
                    kind: KeyPatternKind::Exact(KeyEvent {
                        key: KeyCode::Char(#cp),
                        modifiers: 0,
                    }),
                }
            }
        }
        KeyPatternExpr::AnyChar => {
            quote! { KeyPattern { kind: KeyPatternKind::AnyChar } }
        }
        KeyPatternExpr::AnyCharPlain => {
            quote! { KeyPattern { kind: KeyPatternKind::AnyCharPlain } }
        }
        KeyPatternExpr::Any => {
            quote! { KeyPattern { kind: KeyPatternKind::AnyKey } }
        }
    }
}

/// Convert a `KeyPatternExpr` into tokens for a WIT `KeyEvent` (for chord leaders).
fn key_pattern_expr_to_key_event_tokens(pat: &KeyPatternExpr) -> proc_macro2::TokenStream {
    match pat {
        KeyPatternExpr::Ctrl(ch) => {
            let cp = ch.value() as u32;
            quote! {
                KeyEvent {
                    key: KeyCode::Char(#cp),
                    modifiers: kasane_plugin_sdk::modifiers::CTRL,
                }
            }
        }
        KeyPatternExpr::Key(ident) => {
            quote! {
                KeyEvent {
                    key: KeyCode::#ident,
                    modifiers: 0,
                }
            }
        }
        KeyPatternExpr::Char(ch) => {
            let cp = ch.value() as u32;
            quote! {
                KeyEvent {
                    key: KeyCode::Char(#cp),
                    modifiers: 0,
                }
            }
        }
        _ => {
            quote! { compile_error!("chord leader must be a specific key, not any_char/any") }
        }
    }
}

/// Generate the `declare_key_map()` method body.
pub(crate) fn generate_key_map_declare(km: &KeyMapDef) -> proc_macro2::TokenStream {
    let mut group_tokens = Vec::new();
    let mut group_index: usize = 0;

    // When-groups
    for wg in &km.groups {
        let group_name = format!("__group_{group_index}");
        group_index += 1;

        let binding_tokens: Vec<_> = wg
            .bindings
            .iter()
            .map(|b| {
                let pat = key_pattern_expr_to_tokens(&b.pattern);
                let action = &b.action_id;
                quote! {
                    KeyBindingDecl {
                        pattern: #pat,
                        action_id: #action.to_string(),
                    }
                }
            })
            .collect();

        group_tokens.push(quote! {
            KeyGroupDecl {
                name: #group_name.to_string(),
                bindings: vec![#( #binding_tokens ),*],
                chords: vec![],
            }
        });
    }

    // Chord groups
    for cg in &km.chords {
        let group_name = format!("__chord_{group_index}");
        group_index += 1;

        let leader = key_pattern_expr_to_key_event_tokens(&cg.leader);
        let chord_tokens: Vec<_> = cg
            .bindings
            .iter()
            .map(|b| {
                let follower = key_pattern_expr_to_tokens(&b.pattern);
                let action = &b.action_id;
                quote! {
                    ChordBindingDecl {
                        leader: #leader,
                        follower: #follower,
                        action_id: #action.to_string(),
                    }
                }
            })
            .collect();

        group_tokens.push(quote! {
            KeyGroupDecl {
                name: #group_name.to_string(),
                bindings: vec![],
                chords: vec![#( #chord_tokens ),*],
            }
        });
    }

    quote! {
        vec![#( #group_tokens ),*]
    }
}

/// Generate the `is_group_active(group_name)` method body.
pub(crate) fn generate_is_group_active(
    km: &KeyMapDef,
    has_state: bool,
) -> proc_macro2::TokenStream {
    let mut arms = Vec::new();
    let mut group_index: usize = 0;

    for wg in &km.groups {
        let group_name = format!("__group_{group_index}");
        group_index += 1;
        let pred = &wg.predicate;

        if has_state {
            arms.push(quote! {
                #group_name => STATE.with(|__s| {
                    let state = __s.borrow();
                    #pred
                }),
            });
        } else {
            arms.push(quote! {
                #group_name => { #pred },
            });
        }
    }

    // Chord groups are always active
    for _cg in &km.chords {
        let group_name = format!("__chord_{group_index}");
        group_index += 1;
        arms.push(quote! {
            #group_name => true,
        });
    }

    quote! {
        match group_name.as_str() {
            #( #arms )*
            _ => true,
        }
    }
}

/// Generate the `invoke_action(action_id, event)` method.
pub(crate) fn generate_invoke_action(
    actions: &Option<Vec<ActionDef>>,
    has_state: bool,
    wrap_state: &dyn Fn(&proc_macro2::TokenStream) -> proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let actions = match actions {
        Some(a) => a,
        None => {
            return quote! {
                fn invoke_action(_action_id: String, _event: KeyEvent) -> KeyResponse {
                    KeyResponse::Pass
                }
            };
        }
    };

    let arms: Vec<_> = actions
        .iter()
        .map(|a| {
            let id = &a.id;
            let event_param = &a.event_param;
            let body = &a.body;
            // Note: the arm body is the same regardless of has_state;
            // state wrapping is applied to the whole match expression below.
            quote! {
                #id => {
                    let #event_param = &event;
                    #body
                }
            }
        })
        .collect();

    let match_body = quote! {
        match action_id.as_str() {
            #( #arms )*
            _ => KeyResponse::Pass,
        }
    };

    let wrapped = if has_state {
        wrap_state(&match_body)
    } else {
        match_body
    };

    quote! {
        fn invoke_action(action_id: String, event: KeyEvent) -> KeyResponse {
            #wrapped
        }
    }
}

// ---------------------------------------------------------------------------
// Key map DSL parsers
// ---------------------------------------------------------------------------

/// Parse a key pattern constructor: `ctrl('c')`, `key(Escape)`, `char('n')`,
/// `any_char()`, `any_char_plain()`, `any()`.
pub(crate) fn parse_key_pattern_expr(
    input: syn::parse::ParseStream,
) -> syn::Result<KeyPatternExpr> {
    // Check for a bare char literal (used in chord bindings: `'v' => "split_v"`)
    if input.peek(syn::LitChar) {
        let ch: syn::LitChar = input.parse()?;
        return Ok(KeyPatternExpr::Char(ch));
    }

    let name: syn::Ident = input.parse()?;
    let name_str = name.to_string();

    let args;
    syn::parenthesized!(args in input);

    match name_str.as_str() {
        "ctrl" => {
            let ch: syn::LitChar = args.parse()?;
            Ok(KeyPatternExpr::Ctrl(ch))
        }
        "key" => {
            let ident: syn::Ident = args.parse()?;
            Ok(KeyPatternExpr::Key(ident))
        }
        "char" => {
            let ch: syn::LitChar = args.parse()?;
            Ok(KeyPatternExpr::Char(ch))
        }
        "any_char" => Ok(KeyPatternExpr::AnyChar),
        "any_char_plain" => Ok(KeyPatternExpr::AnyCharPlain),
        "any" => Ok(KeyPatternExpr::Any),
        other => Err(syn::Error::new(
            name.span(),
            format!(
                "unknown key pattern `{other}()`; expected ctrl, key, char, any_char, any_char_plain, or any"
            ),
        )),
    }
}

/// Parse a single binding: `PATTERN => "action_id"`.
pub(crate) fn parse_binding_entry(input: syn::parse::ParseStream) -> syn::Result<BindingEntry> {
    let pattern = parse_key_pattern_expr(input)?;
    input.parse::<syn::Token![=>]>()?;
    let action_id: syn::LitStr = input.parse()?;
    Ok(BindingEntry { pattern, action_id })
}

/// Parse the `key_map { ... }` section body.
pub(crate) fn parse_key_map_def(input: syn::parse::ParseStream) -> syn::Result<KeyMapDef> {
    let mut groups = Vec::new();
    let mut chords = Vec::new();

    while !input.is_empty() {
        let ident: syn::Ident = input.parse()?;
        match ident.to_string().as_str() {
            "when" => {
                // when(PREDICATE) { bindings }
                let pred_content;
                syn::parenthesized!(pred_content in input);
                let predicate: proc_macro2::TokenStream = pred_content.parse()?;

                let bindings_content;
                syn::braced!(bindings_content in input);
                let mut bindings = Vec::new();
                while !bindings_content.is_empty() {
                    bindings.push(parse_binding_entry(&bindings_content)?);
                    if !bindings_content.is_empty() {
                        bindings_content.parse::<syn::Token![,]>()?;
                    }
                }
                groups.push(WhenGroupDef {
                    predicate,
                    bindings,
                });
            }
            "chord" => {
                // chord(LEADER_PATTERN) { bindings }
                let leader_content;
                syn::parenthesized!(leader_content in input);
                let leader = parse_key_pattern_expr(&leader_content)?;

                let bindings_content;
                syn::braced!(bindings_content in input);
                let mut bindings = Vec::new();
                while !bindings_content.is_empty() {
                    bindings.push(parse_binding_entry(&bindings_content)?);
                    if !bindings_content.is_empty() {
                        bindings_content.parse::<syn::Token![,]>()?;
                    }
                }
                chords.push(ChordGroupDef { leader, bindings });
            }
            other => {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("expected `when` or `chord` in key_map, got `{other}`"),
                ));
            }
        }

        if !input.is_empty() {
            let _ = input.parse::<syn::Token![,]>();
        }
    }

    Ok(KeyMapDef { groups, chords })
}

/// Parse the `actions { ... }` section body.
pub(crate) fn parse_actions_def(input: syn::parse::ParseStream) -> syn::Result<Vec<ActionDef>> {
    let mut actions = Vec::new();

    while !input.is_empty() {
        let id: syn::LitStr = input.parse()?;
        input.parse::<syn::Token![=>]>()?;
        input.parse::<syn::Token![|]>()?;
        let event_param: syn::Ident = input.parse()?;
        input.parse::<syn::Token![|]>()?;

        let body;
        syn::braced!(body in input);
        let body_tokens: proc_macro2::TokenStream = body.parse()?;

        actions.push(ActionDef {
            id,
            event_param,
            body: body_tokens,
        });

        if !input.is_empty() {
            let _ = input.parse::<syn::Token![,]>();
        }
    }

    Ok(actions)
}
