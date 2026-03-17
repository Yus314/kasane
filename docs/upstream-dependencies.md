# Upstream Dependencies (Kakoune Protocol)

This document is a tracker for items that cannot be fully implemented without changes to the Kakoune upstream.
The authoritative constraint analysis is in [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md).

## 1. Document Scope

This document is a tracker for items that Kasane cannot fully implement without changes to the Kakoune upstream.

This document covers only:
- What is blocked
- Which upstream PRs / Issues to watch
- When items can be reintegrated into the roadmap

For detailed constraint analysis, see [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md).
For implementation order, see [roadmap.md](./roadmap.md).

## 2. Current Snapshot

Upstream status as of 2026-03-14:
- `PR #5455` was merged on 2026-03-11
- `PR #4707`, `PR #5304` are open
- `#5428`, `#4686`, `#4687`, `#5294` are open
- `#4138` is closed. P-060 / decoration extension use cases are not considered an upstream blocker; they are treated as Kasane-side rendering implementation and ecosystem-level issues
- `PR #4737` was absorbed into `PR #5455` and is no longer tracked

## 3. Fully Blocked

Items that cannot be fully implemented without upstream changes.

| ID | Item | Missing Information / Functionality | Limitation of Local Workarounds | Upstream | Reintegration Target |
|----|------|-------------------------------------|---------------------------------|----------|----------------------|
| D-004 | Completeness of right-side navigation UI | Information needed for scroll position, total line count, and handle ratio | Estimation from cursor position breaks down when the viewport does not follow | [PR #5304](https://github.com/mawww/kakoune/pull/5304), [#165](https://github.com/mawww/kakoune/issues/165) | `P-012`, right-side UI use cases |
| D-002 | Auxiliary display for off-screen cursors / selections | Number and positions of cursors outside the viewport | Only cursors visible within the view can be detected | [#2727](https://github.com/mawww/kakoune/issues/2727), [#5425](https://github.com/mawww/kakoune/issues/5425) | `D-002` reintegration |

## 4. Items with Quality-Limited Workarounds Only

Items where local implementation is possible, but are not treated as authoritative due to heuristic dependency or insufficient verification of upstream behavior.

| ID | Item | Current Status | Why Not Adopted | Upstream | Next Step |
|----|------|----------------|-----------------|----------|-----------|
| D-003 | Status line context inference | Inference via face names or strings is possible | Breaks with custom faces or message compositions | [#5428](https://github.com/mawww/kakoune/issues/5428) | Deferred until context type is available |
| D-001 | Startup info retention | Possibly avoidable with a local queue | Still isolating upstream startup behavior | [#5294](https://github.com/mawww/kakoune/issues/5294) | Reintegrate after confirming upstream behavior |
| P-010 / P-011 | Full auxiliary region contribution | `widget_columns` is available. Partial proof-of-concept completed | No semantic type for atoms; cannot strictly distinguish line numbers / virtual text / code | [PR #4707](https://github.com/mawww/kakoune/pull/4707), [#4687](https://github.com/mawww/kakoune/issues/4687) | Reintegrate after semantic type is added |
| P-001 | Overlay composition (full version) | Overlay itself is partially proven. `widget_columns` is also available | Semantic position within the buffer depends on atom ambiguity | [PR #4707](https://github.com/mawww/kakoune/pull/4707), [#4687](https://github.com/mawww/kakoune/issues/4687) | Reintegrate after semantic type is added |

## 5. Upstream Watchlist

Upstream items being tracked as of 2026-03-14:

| Upstream ID | Description | Affected Items | Status |
|-------------|-------------|----------------|--------|
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | Addition of face / semantic type equivalent to JSON UI | P-001, P-010, P-011, C-008 family | Open |
| [PR #5455](https://github.com/mawww/kakoune/pull/5455) | Addition of `widget_columns` to `draw` | P-001, P-010, P-011 | Merged (2026-03-11) |
| [PR #5304](https://github.com/mawww/kakoune/pull/5304) | Scroll position protocol | D-004, P-012 | Open |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | `draw_status` context | D-003 | Open |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | Incremental `draw` | Upstream version of NF-004 | Open |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Atom type ambiguity | P-001, P-010, P-011, C-008 family | Open |
| [#5294](https://github.com/mawww/kakoune/issues/5294) | Startup `info` display | D-001 | Open |

## 6. Reintegration Rules

An item is moved back to [roadmap.md](./roadmap.md) when the following conditions are met:

1. The required upstream PR / protocol change has been merged, or upstream behavior has been sufficiently confirmed
2. Kasane-side parser / state / render can incorporate the new information
3. Local heuristic workarounds can be removed or degraded
4. The status in [roadmap.md](./roadmap.md) is updated

## 7. Related Documents

- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md) — Constraint analysis
- [roadmap.md](./roadmap.md) — Kasane-side incomplete items
