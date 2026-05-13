# ADR-001: Rendering Approach — TUI + GUI Hybrid

**Status:** Decided

**Context:**
Four options were evaluated as the rendering approach for Kasane: TUI (in-terminal), GUI (native window), GPU-embedded terminal, and TUI + GUI hybrid.

**Evaluation of options:**

| Approach | Resolvable Issues | MVP Timeline | SSH/tmux |
|----------|-------------------|-------------|----------|
| TUI (Kitty-based) | ~71/80 | ~2 months | Supported |
| GUI | ~80/80 | ~4-5 months | Not supported |
| GPU-embedded terminal | ~80/80 | ~5-6 months | Not supported |
| TUI + GUI hybrid | TUI: ~71 / GUI: ~80 | TUI: ~2 months | TUI: Supported |

**Decision:** Adopt the TUI + GUI hybrid approach.

**Rationale:**
- Maintaining SSH/tmux workflows is necessary → TUI backend is required
- GUI benefits (subpixel rendering, D&D, font size adjustment, etc.) are also desired → GUI backend is needed
- Abstract core logic via the `RenderBackend` trait, making TUI and GUI interchangeable
- Release MVP quickly with TUI, add GUI backend in Phase 4
