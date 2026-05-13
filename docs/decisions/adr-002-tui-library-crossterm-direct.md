# ADR-002: TUI Library — crossterm Direct

**Status:** Decided

**Context:**
Three options were evaluated as the TUI backend library: ratatui + crossterm, crossterm direct, and termwiz.

**Evaluation of options:**

| Library | Dev Speed | Performance | GUI Abstraction Compatibility |
|---------|-----------|-------------|-------------------------------|
| ratatui + crossterm | Fastest | Medium (framework constraints) | Medium |
| crossterm direct | Slow | Best (full control) | High |
| termwiz | Moderate | High | Medium |

**Decision:** Adopt crossterm direct.

**Rationale:**
- Enables custom optimization of the cell grid diff rendering algorithm
- Facilitates abstraction with the GUI backend — cell grid diff computation can be placed in core
- Avoids ratatui's widget rebuild overhead
- Aligns with the performance-focused design philosophy

**Trade-offs:**
- Border drawing, popup clipping, and layout computation all need custom implementation
- Cost of reimplementing ~2,000–3,000 lines of code that ratatui provides
