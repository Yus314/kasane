# Architecture Decision Records (ADR)

This document is a historical record of the technical decisions made in Kasane, including subsequent updates and revocations.
For the current authoritative specification, refer to [semantics.md](./semantics.md) and each Current document.
The summary table in this chapter is a summary for current readers; each ADR body preserves the context at the time of decision. Where a subsequent ADR overrides an earlier one, the status field and notes in each section take precedence.

## Decision Summary (for current readers)

Legend: `Current` = still in effect, `Proposed` = future design. The Notes column indicates overrides by subsequent ADRs or implementation notes.

| Item | Status | Current Treatment | Notes |
|------|--------|-------------------|-------|
| Implementation language | Current | **Rust** | Performance and safety |
| Target platforms | Current | **Linux + macOS** | Kakoune's primary user base |
| Scope | Current | **Complete frontend replacement** | Replaces Kakoune's terminal UI, adds frontend-native capabilities |
| Rendering approach | Current | **TUI + GUI hybrid** | TUI for SSH/tmux, GUI for native window |
| TUI library | Current | **crossterm direct** | Full rendering control |
| GUI toolkit | Current | **winit + wgpu + Parley + swash** | cosmic-text + glyphon retired in [ADR-031](./decisions/adr-031-text-stack-migration-cosmic-text-parley-swash-with-protocol-style-redesign.md). Window/GPU layer unchanged. Original ADR-014 selection of glyphon is superseded for the text stack only. |
| Configuration format | Current | **Unified KDL + ui_options** | Single `kasane.kdl` for config + widgets. Supersedes ADR-003 (TOML + separate widgets.kdl) |
| Crate structure | Current | **Cargo workspace** | `kasane-core` / `kasane-tui` / `kasane-gui` / `kasane` / `kasane-macros` / `kasane-wasm` / `kasane-wasm-bench` |
| Kakoune version | Current | **Latest stable only** | Leverages new protocol features |
| kak-lsp integration | Current | **Pure JSON UI frontend** | No special handling for kak-lsp |
| Development environment | Current | **Nix flake + direnv** | Reproducible development environment |
| Async runtime | Current | **Synchronous + threads** | Compatible with backend / event loop |
| Kakoune process management | Current | **Child process spawn + session connection** | Supports `-c` / `-s` |
| Unicode width calculation | Current | **unicode-width + compatibility patches** | Corrects Kakoune mismatch cases |
| Error handling | Current | **anyhow + thiserror** | Structured in core, aggregated in bin |
| Logging | Current | **tracing + file output** | Filter control via `KASANE_LOG` |
| Testing strategy | Current | **Unit + snapshot + property-based tests** | Combined use of `insta` and `proptest` |
| CI/CD | Current | **GitHub Actions + Nix** | Build / test / lint on Linux/macOS |
| Rust edition | Current | **Edition 2024 / no MSRV** | Toolchain pinned via Nix |
| JSON parser | Current | **simd-json** | serde-compatible API |
| License | Current | **MIT OR Apache-2.0** | Standard Rust dual license |
| Declarative UI | Current | **Element tree + TEA** | Details in [ADR-009](./decisions/adr-009-declarative-ui-architecture-transition-to-plugin-infrastructure.md) |
| Plugin execution model | Current | **WASM Component Model as first choice, native proc-macro path coexists** | The native-only assumption of 9-2 was updated by [ADR-013](./decisions/adr-013-wasm-plugin-runtime-component-model-adoption.md) |
| Element memory | Current | **Owned** | No lifetimes |
| State management | Current | **TEA (The Elm Architecture)** | Unidirectional data flow |
| Plugin extension | Current | **Slot + Decorator + Replacement** | Three-tier extension mechanism |
| Layout | Current | **Flex + Overlay + Grid** | Basic layout + layering + tabular |
| Event propagation | Current | **Central dispatch + InteractiveId** | Keys centralized, mouse uses hit test |
| Compiler-driven optimization | Current | **Salsa incremental computation + SceneCache (GPU)** | ViewCache/PaintPatch superseded by Salsa (ADR-020) |
| CLI design | Current | **kak drop-in replacement** | Non-UI flags delegated via exec |
| Three-layer responsibilities | Current | **Upstream / Core / Plugin** | Criteria in [ADR-012](./decisions/adr-012-layer-responsibility-model.md) |
| WASM plugin runtime | Current | **Component Model (wasmtime)** | Detailed performance figures in [ADR-013](./decisions/adr-013-wasm-plugin-runtime-component-model-adoption.md) and [performance.md](./performance.md) |
| Pipeline equivalence testing | Current | **Trace-Equivalence axiom + proptest** | Current harness generates DirtyFlags at coarse granularity |
| SurfaceId-based invalidation | Proposed | **Per-surface dirty / cache design** | For multi-pane, not yet implemented |
| Plugin I/O infrastructure | Current | **Hybrid model (WASI direct + host-mediated)** | Design foundation for Phase P. Details in [ADR-019](./decisions/adr-019-plugin-io-infrastructure-hybrid-model.md) |
| Salsa incremental computation | Current | **Stage 1 (Salsa tracked) + Stage 2 (imperative plugins)** | Mandatory dependency (feature flag removed). Details in [ADR-020](./decisions/adr-020-salsa-incremental-computation-stage-12-split.md) |
| Plugin trait naming | Current | **`Plugin` (state-externalized, primary) + `PluginBackend` (mutable, internal)** | Renamed from `PurePlugin`/`Plugin`. Details in [ADR-022](./decisions/adr-022-plugin-trait-rename-pureplugin-plugin-plugin-pluginbackend.md) |
| Session management boundaries | Current | **Mechanism (core) / Policy (plugin) split** | Session lifecycle in core; session UI in plugins. Details in [ADR-023](./decisions/adr-023-session-management-boundaries-mechanism-policy-split.md) |
| Display transformation | Current | **DisplayMap + DisplayDirective** | Plugin-declared directives (Fold/InsertAfter/Hide) → core builds O(1) bidirectional mapping. Single-plugin constraint initially. Virtual text proof artifact in `examples/virtual-text-demo/`. Kakoune viewport control limits true folding |
| Performance policy | Current | **Three-layer perceptual framework** | Perceptual compass + engineering ratchets + optimization accountability. Details in [ADR-024](./decisions/adr-024-perception-oriented-performance-policy.md) |
| Plugin registration model | Current | **HandlerRegistry + Plugin trait (2 methods + 1 associated type)** | Plugins register handlers declaratively; capabilities auto-inferred. Details in [ADR-025](./decisions/adr-025-handlerregistry-plugin-architecture.md) |
| Declarative transforms | Current | **ElementPatch algebra** | Composable, normalizable, Salsa-memoizable. Custom escape hatch for imperative transforms. Details in [ADR-026](./decisions/adr-026-elementpatch-declarative-transforms.md) |
| Annotation decomposition | Current | **4 annotation extension points + render_ornaments** | Gutter, background, inline, virtual text (annotation), plus render_ornaments (physical decoration). Details in [ADR-027](./decisions/adr-027-lineannotation-decomposition.md) |
| WASM capability inference | Current | **`register-capabilities` WIT export** | WASM plugins declare capabilities as a bitmask; host skips non-participating dispatch. Details in [ADR-028](./decisions/adr-028-wasm-capability-inference.md) |
| Inter-plugin communication | Current | **Topic-based pub/sub + plugin-defined extension points** | Two-phase evaluation with cycle prevention; typed extension points with composition rules. Details in [ADR-029](./decisions/adr-029-topic-based-pubsub-and-plugin-defined-extension-points.md) |
| GPU rendering strategy | Proposed | **Vello evaluation framework (spike + trait abstraction)** | Re-evaluation of [ADR-014](./decisions/adr-014-gui-technology-stack-winit-wgpu-glyphon.md) §14-1 in light of 2026 Q1 changes (Glifo, Vello Hybrid). Details in [ADR-032](./decisions/adr-032-gpu-rendering-strategy-vello-evaluation-framework.md). |
| `kasane.kdl` auto-reload for plugins/settings | Current | **Opt-in `plugins.auto_reload`** | Live `resolve` + plugin reload triggered by kdl edits when enabled; default is the prior "resolve + restart" workflow. Details in [ADR-040](./decisions/adr-040-kasanekdl-auto-reload-for-plugins-and-settings.md). |

## ADR Index

Per-ADR files live under [`docs/decisions/`](./decisions/). Click an entry to open its full record.

| ID | Title | File |
|---|---|---|
| ADR-001 | Rendering Approach — TUI + GUI Hybrid | [`adr-001-rendering-approach-tui-gui-hybrid.md`](./decisions/adr-001-rendering-approach-tui-gui-hybrid.md) |
| ADR-002 | TUI Library — crossterm Direct | [`adr-002-tui-library-crossterm-direct.md`](./decisions/adr-002-tui-library-crossterm-direct.md) |
| ADR-003 | Configuration Format — TOML + ui_options Combined | [`adr-003-configuration-format-toml-uioptions-combined.md`](./decisions/adr-003-configuration-format-toml-uioptions-combined.md) |
| ADR-004 | kak-lsp Integration — Pure JSON UI Frontend | [`adr-004-kak-lsp-integration-pure-json-ui-frontend.md`](./decisions/adr-004-kak-lsp-integration-pure-json-ui-frontend.md) |
| ADR-004A | Standard Frontend Compatibility as the Primary Constraint | [`adr-004a-standard-frontend-compatibility-as-the-primary-constraint.md`](./decisions/adr-004a-standard-frontend-compatibility-as-the-primary-constraint.md) |
| ADR-005 | Development Environment Management — Nix flake + direnv | [`adr-005-development-environment-management-nix-flake-direnv.md`](./decisions/adr-005-development-environment-management-nix-flake-direnv.md) |
| ADR-006 | Async Runtime — Synchronous + Threads | [`adr-006-async-runtime-synchronous-threads.md`](./decisions/adr-006-async-runtime-synchronous-threads.md) |
| ADR-007 | Kakoune Process Management — Child Process Spawn + Session Connection | [`adr-007-kakoune-process-management-child-process-spawn-session-connection.md`](./decisions/adr-007-kakoune-process-management-child-process-spawn-session-connection.md) |
| ADR-008 | JSON Parser — simd-json | [`adr-008-json-parser-simd-json.md`](./decisions/adr-008-json-parser-simd-json.md) |
| ADR-009 | Declarative UI Architecture — Transition to Plugin Infrastructure | [`adr-009-declarative-ui-architecture-transition-to-plugin-infrastructure.md`](./decisions/adr-009-declarative-ui-architecture-transition-to-plugin-infrastructure.md) |
| ADR-010 | Compiler-Driven Optimization — Svelte-like Two-Layer Rendering | [`adr-010-compiler-driven-optimization-svelte-like-two-layer-rendering.md`](./decisions/adr-010-compiler-driven-optimization-svelte-like-two-layer-rendering.md) |
| ADR-011 | CLI Design — kak Drop-in Replacement | [`adr-011-cli-design-kak-drop-in-replacement.md`](./decisions/adr-011-cli-design-kak-drop-in-replacement.md) |
| ADR-012 | Layer Responsibility Model | [`adr-012-layer-responsibility-model.md`](./decisions/adr-012-layer-responsibility-model.md) |
| ADR-013 | WASM Plugin Runtime — Component Model Adoption | [`adr-013-wasm-plugin-runtime-component-model-adoption.md`](./decisions/adr-013-wasm-plugin-runtime-component-model-adoption.md) |
| ADR-014 | GUI Technology Stack — winit + wgpu + glyphon | [`adr-014-gui-technology-stack-winit-wgpu-glyphon.md`](./decisions/adr-014-gui-technology-stack-winit-wgpu-glyphon.md) |
| ADR-015 | Rendering Pipeline Performance Improvements | [`adr-015-rendering-pipeline-performance-improvements.md`](./decisions/adr-015-rendering-pipeline-performance-improvements.md) |
| ADR-016 | Pipeline Equivalence Testing — Trace-Equivalence Axiom | [`adr-016-pipeline-equivalence-testing-trace-equivalence-axiom.md`](./decisions/adr-016-pipeline-equivalence-testing-trace-equivalence-axiom.md) |
| ADR-017 | SurfaceId-Based Invalidation (Design) | [`adr-017-surfaceid-based-invalidation-design.md`](./decisions/adr-017-surfaceid-based-invalidation-design.md) |
| ADR-018 | Display Policy Layer and Display Transformation / Display Unit Model | [`adr-018-display-policy-layer-and-display-transformation-display-unit-model.md`](./decisions/adr-018-display-policy-layer-and-display-transformation-display-unit-model.md) |
| ADR-019 | Plugin I/O Infrastructure — Hybrid Model | [`adr-019-plugin-io-infrastructure-hybrid-model.md`](./decisions/adr-019-plugin-io-infrastructure-hybrid-model.md) |
| ADR-020 | Salsa Incremental Computation — Stage 1/2 Split | [`adr-020-salsa-incremental-computation-stage-12-split.md`](./decisions/adr-020-salsa-incremental-computation-stage-12-split.md) |
| ADR-021 | PurePlugin State Externalization | [`adr-021-pureplugin-state-externalization.md`](./decisions/adr-021-pureplugin-state-externalization.md) |
| ADR-022 | Plugin Trait Rename — PurePlugin → Plugin, Plugin → PluginBackend | [`adr-022-plugin-trait-rename-pureplugin-plugin-plugin-pluginbackend.md`](./decisions/adr-022-plugin-trait-rename-pureplugin-plugin-plugin-pluginbackend.md) |
| ADR-023 | Session Management Boundaries — Mechanism / Policy Split | [`adr-023-session-management-boundaries-mechanism-policy-split.md`](./decisions/adr-023-session-management-boundaries-mechanism-policy-split.md) |
| ADR-024 | Perception-Oriented Performance Policy | [`adr-024-perception-oriented-performance-policy.md`](./decisions/adr-024-perception-oriented-performance-policy.md) |
| ADR-025 | HandlerRegistry Plugin Architecture | [`adr-025-handlerregistry-plugin-architecture.md`](./decisions/adr-025-handlerregistry-plugin-architecture.md) |
| ADR-026 | ElementPatch Declarative Transforms | [`adr-026-elementpatch-declarative-transforms.md`](./decisions/adr-026-elementpatch-declarative-transforms.md) |
| ADR-027 | LineAnnotation Decomposition | [`adr-027-lineannotation-decomposition.md`](./decisions/adr-027-lineannotation-decomposition.md) |
| ADR-028 | WASM Capability Inference | [`adr-028-wasm-capability-inference.md`](./decisions/adr-028-wasm-capability-inference.md) |
| ADR-029 | Topic-Based Pub/Sub and Plugin-Defined Extension Points | [`adr-029-topic-based-pubsub-and-plugin-defined-extension-points.md`](./decisions/adr-029-topic-based-pubsub-and-plugin-defined-extension-points.md) |
| ADR-030 | Observed/Policy Separation — Staged Projection Rollout | [`adr-030-observedpolicy-separation-staged-projection-rollout.md`](./decisions/adr-030-observedpolicy-separation-staged-projection-rollout.md) |
| ADR-031 | Text Stack Migration — cosmic-text → Parley + swash, with Protocol Style Redesign | [`adr-031-text-stack-migration-cosmic-text-parley-swash-with-protocol-style-redesign.md`](./decisions/adr-031-text-stack-migration-cosmic-text-parley-swash-with-protocol-style-redesign.md) |
| ADR-032 | GPU Rendering Strategy — Vello Evaluation Framework | [`adr-032-gpu-rendering-strategy-vello-evaluation-framework.md`](./decisions/adr-032-gpu-rendering-strategy-vello-evaluation-framework.md) |
| ADR-033 | Plugin Failure Semantics | [`adr-033-plugin-failure-semantics.md`](./decisions/adr-033-plugin-failure-semantics.md) |
| ADR-034 | Display Algebra — From Variant Enum to Composable Primitives | [`adr-034-display-algebra-from-variant-enum-to-composable-primitives.md`](./decisions/adr-034-display-algebra-from-variant-enum-to-composable-primitives.md) |
| ADR-035 | First-Class Selection and Time | [`adr-035-first-class-selection-and-time.md`](./decisions/adr-035-first-class-selection-and-time.md) |
| ADR-036 | Cross-File Inlining (reserved) | [`adr-036-cross-file-inlining-reserved.md`](./decisions/adr-036-cross-file-inlining-reserved.md) |
| ADR-037 | Fold-in-Algebra — Retiring the Hybrid Bridge | [`adr-037-fold-in-algebra-retiring-the-hybrid-bridge.md`](./decisions/adr-037-fold-in-algebra-retiring-the-hybrid-bridge.md) |
| ADR-038 | Plugin Authoring Path Consolidation | [`adr-038-plugin-authoring-path-consolidation.md`](./decisions/adr-038-plugin-authoring-path-consolidation.md) |
| ADR-039 | Plugin Path Consolidation (R2.x) | [`adr-039-plugin-path-consolidation-r2x.md`](./decisions/adr-039-plugin-path-consolidation-r2x.md) |
| ADR-040 | `kasane.kdl` Auto-Reload for `plugins` and `settings` | [`adr-040-kasanekdl-auto-reload-for-plugins-and-settings.md`](./decisions/adr-040-kasanekdl-auto-reload-for-plugins-and-settings.md) |
| ADR-041 | `eval-command` in `session-ready-command` | [`adr-041-eval-command-in-session-ready-command.md`](./decisions/adr-041-eval-command-in-session-ready-command.md) |
| ADR-042 | `command-error-event` via `info_show` Marker Attribution | [`adr-042-command-error-event-via-infoshow-marker-attribution.md`](./decisions/adr-042-command-error-event-via-infoshow-marker-attribution.md) |
| ADR-043 | Structured `KakCommand` enum for type-safe Kakoune command construction | [`adr-043-structured-kakcommand-enum-for-type-safe-kakoune-command-construction.md`](./decisions/adr-043-structured-kakcommand-enum-for-type-safe-kakoune-command-construction.md) |
| ADR-044 | Handler → Effect Tier Hierarchy | [`adr-044-handler-effect-tier-hierarchy.md`](./decisions/adr-044-handler-effect-tier-hierarchy.md) |
| ADR-045 | Retire the Extension-Point Dispatch Path | [`adr-045-retire-the-extension-point-dispatch-path.md`](./decisions/adr-045-retire-the-extension-point-dispatch-path.md) |
| ADR-046 | WIT ABI 6.0.0 — Batched Retirement | [`adr-046-wit-abi-600-batched-retirement.md`](./decisions/adr-046-wit-abi-600-batched-retirement.md) |
| ADR-047 | Salsa Render Path Strategy — Salsa Remains Canonical | [`adr-047-salsa-render-path-strategy-salsa-remains-canonical.md`](./decisions/adr-047-salsa-render-path-strategy-salsa-remains-canonical.md) |
| ADR-048 | Plugin Backend Trait Extinction (Phase β) | [`adr-048-plugin-backend-trait-extinction-phase.md`](./decisions/adr-048-plugin-backend-trait-extinction-phase.md) |
| ADR-049 | `PluginEntry` Shape and β-3.3 Staging | [`adr-049-pluginentry-shape-and-33-staging.md`](./decisions/adr-049-pluginentry-shape-and-33-staging.md) |
