# ADR-006: Async Runtime — Synchronous + Threads

**Status:** Decided

**Context:**
Kasane has 5 I/O streams: (1) Kakoune stdout reading, (2) crossterm input event reception, (3) Kakoune stdin writing, (4) terminal output, and (5) timers. The question was how to handle these concurrently.

**Evaluation of options:**

| Approach | Implementation Cost | crossterm Compatibility | Binary Size | Debuggability |
|----------|--------------------|-----------------------|-------------|---------------|
| Synchronous + threads | Low | Best | Smallest | High |
| tokio | Medium | Medium (EventStream spawns a separate thread internally) | +1-2MB | Medium |
| polling / mio direct | High | Low (dual management with crossterm) | Smallest | Medium |

**Decision:** Adopt synchronous + threads.

**Rationale:**
- crossterm's `read()` is a synchronous blocking API, more reliable than the async `EventStream`
- Kasane's I/O pattern is simply merging 3 streams, making most of tokio's features unnecessary
- Helix, Alacritty, and Zellij also use similar thread-based architectures for input processing
- `std::sync::mpsc` or `crossbeam-channel` for inter-thread message passing
- Timers realized via `crossbeam-channel::select!` timeout
