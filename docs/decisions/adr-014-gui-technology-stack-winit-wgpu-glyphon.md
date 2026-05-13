# ADR-014: GUI Technology Stack — winit + wgpu + glyphon

**Status:** Decided

**Context:**
After adopting the TUI + GUI hybrid approach in ADR-001, the specific technology stack and event loop design for the GUI backend were evaluated.

### 14-1: Rendering Stack — winit + wgpu + glyphon

**Decision:** Adopt winit for window management, wgpu for GPU rendering, and glyphon for text rendering.

| Library | Role |
|---------|------|
| winit | Window management, input events, IME |
| wgpu | GPU rendering API (Vulkan/Metal/DX12/GL abstraction) |
| glyphon | Text rendering (cosmic-text + swash + etagere atlas) |

**Selection rationale:** cosmic-term (the official terminal of COSMIC Desktop) uses the same stack in production, with proven track record for monospace grid rendering. glyphon integrates cosmic-text's font shaping (rustybuzz) + swash rasterization + etagere atlas packing into the wgpu pipeline.

**Rejected alternatives:**

| Candidate | Reason for rejection |
|-----------|---------------------|
| OpenGL (glutin + glow) | macOS has deprecated OpenGL. wgpu internally has an OpenGL ES backend |
| Native API (Metal/Vulkan direct) | Requires a separate renderer per platform. Doubles maintenance cost |
| CPU only (softbuffer + tiny-skia) | Insufficient as the main path for 60fps smooth scrolling. Considered as fallback but not implemented |
| egui | Immediate mode conflicts with TEA retained mode. Not specialized for monospace grids |
| Vello (Linebender) | No glyph cache (vector path rendering every frame), unstable API (breaking changes every 3-5 months), requires compute shaders |

### 14-2: Event Loop — run_tui/run_gui Branching

**Decision:** Adopt the approach of switching the entire event loop via the `--ui gui` CLI argument (run_tui/run_gui branching).

**Rationale:**
- winit's `run_app()` completely occupies the main thread, so it cannot coexist with TUI's existing `recv_timeout` loop
- GUI side places the winit event loop (`ApplicationHandler`) on the main thread, Kakoune Reader on a separate thread, and merges them via `EventLoopProxy`

**Rejected:** `pump_events` approach — does not work on macOS (Cocoa/AppKit constraints. winit documentation explicitly states "not supported on iOS, macOS, Web").

---
