# ADR-004: kak-lsp Integration — Pure JSON UI Frontend

**Status:** Decided

**Context:**
kak-lsp makes heavy use of info/menu and thus benefits most from Kasane's floating windows. The question was whether to provide special handling specific to kak-lsp.

**Decision:** As a pure JSON UI frontend, no kak-lsp-specific handling is provided.

**Rationale:**
- Protocol compliance alone naturally provides the main benefits (scrollable popups, placement customization, borders)
- Depending on kak-lsp implementation details risks breakage on version upgrades
- Maintains fairness with other plugins (parinfer.kak, kak-tree-sitter, etc.)
- Future integration via `ui_options` can be considered if needed
