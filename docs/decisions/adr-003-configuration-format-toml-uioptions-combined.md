# ADR-003: Configuration Format — TOML + ui_options Combined

**Status:** Superseded — migrated to unified KDL (`kasane.kdl`) for both config and widgets. The ui_options dynamic channel remains.

**Context:**
Three formats plus a combination were evaluated for configuration: TOML, KDL, Kakoune commands only (ui_options only), and TOML + ui_options combined.

**Decision:** Adopt TOML + ui_options combined.

**Rationale:**
- **TOML (static config):** `~/.config/kasane/config.toml` — theme, font, GUI settings, default behavior. Type-safe deserialization via `serde`
- **ui_options (dynamic config):** Kakoune `set-option global ui_options kasane_*=*` — UI behavior that can be changed at runtime. Can be combined with Kakoune hooks and conditionals
- Achieves both type-safe static configuration and dynamic configuration integrated with Kakoune

**Update:** Configuration and widget definitions are now unified in a single `~/.config/kasane/kasane.kdl` file using KDL v2 syntax. The dual-file system (`config.toml` + `widgets.kdl`) has been retired. The ui_options dynamic channel is unchanged.
