# レイヤー責務モデル

本ドキュメントは、新機能が上流 / コア / プラグインのどこに属するかを判断する基準を定義する。
実装メカニズムの分類は [requirements-traceability.md](./requirements-traceability.md) の解決層を参照。

## 概要

本ドキュメントは、Kasane に新機能を追加する際に「その機能がどの層に属するか」を判断するための基準を定める。

[requirements-traceability.md](./requirements-traceability.md) の「解決層」は**実装メカニズム** (HOW: レンダラ/設定/基盤/プロトコル制約) の分類であり、「どの仕組みで解決するか」を定義する。本モデルは**責務境界** (WHERE: 上流/コア/プラグイン) の分類であり、「どのレイヤーが責任を持つか」を定義する。両方の軸が機能分類に必要である。

**関連ドキュメント:**
- [architecture.md](./architecture.md) — 抽象化の境界
- [decisions.md](./decisions.md) — ADR-012: レイヤー責務モデル
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流依存項目の追跡

---

## 三層モデル

> **経緯:** 当初は「上流 / コア / 組み込みプラグイン / 外部プラグイン」の四層だったが、組み込みプラグイン (`kasane-core/src/plugins/`) を WASM バンドルプラグインに移行し、`kasane-core/src/plugins/` を削除した。組み込みプラグインが担っていた役割（API 実証・参照実装・デフォルト UX）はそれぞれ `examples/`・WASM ゲスト (`kasane-wasm/guests/`)・バンドル WASM で吸収されるため、プラグイン層を統合し三層モデルに改定。

### 上流 (Kakoune)

**定義:** プロトコルレベルの関心事。プロトコル変更が必要な機能。

**原則:** コアはプロトコルに存在しない情報のヒューリスティック回避策を原則構築しない。

**追跡:** [upstream-dependencies.md](./upstream-dependencies.md) に記録し、上流 PR/Issue を監視する。

**例:**
- 右側ナビゲーション UI の完全性 (D-004) — `draw` メッセージにスクロール位置が含まれない
- Atom 種別 (PR #4707) — 補助領域 / オーバーレイ / 本文の区別がない
- 画面外カーソル / 選択範囲の補助表示 (D-002) — `draw` メッセージにカーソルの総数が含まれない

### コア (kasane-core)

**定義:** プロトコルの忠実なレンダリング + フロントエンドネイティブ能力。

**判断基準:** 「唯一の正しい実装が存在するか？」 — Yes ならコア。

**プロトコル描画:**
- `draw` / `menu_show` / `info_show` / `draw_status` の忠実なレンダリング
- レイアウト計算 (Flex + Overlay + Grid)
- 差分描画 (CellGrid → diff → backend)

**フロントエンドネイティブ:**
- フォーカス検知 (R-051) — ウィンドウフォーカスの得失はフロントエンド固有
- D&D (P-023 の実証ユースケース) — GUI ウィンドウイベントはフロントエンド固有
- クリップボード (R-080〜R-082) — システム API 直接アクセス
- 複数カーソル描画 (R-050) — プロトコル由来の face 解析
- テキスト装飾の忠実描画 (R-053) — プロトコルが送る下線種別、下線色、取り消し線の忠実描画

**コアが行わないこと:**
- ポリシーが分かれうる表示の決定 (カーソル行ハイライトの色、ガターの表示項目等) — プラグインの領域
- プロトコルに情報がない機能のヒューリスティック推測 — 上流の領域

### プラグイン

**定義:** ポリシーが分かれうる機能。ユーザーの好みや用途に応じてカスタマイズ可能な領域。

**判断基準:** 「唯一の正しい実装が存在するか？」 — No ならプラグイン。

**配布形態:**

| 形態 | 仕組み | 用途 |
|------|--------|------|
| **バンドル WASM** | `include_bytes!` でバイナリに埋め込み | デフォルト UX (cursor_line, color_preview) |
| **FS 発見 WASM** | `~/.local/share/kasane/plugins/*.wasm` | ユーザーが配置する WASM プラグイン |
| **ネイティブ** | `kasane::run(\|registry\| { ... })` でコンパイル時結合 | パフォーマンスクリティカル or Surface/PaintHook/Pane 使用 |

**登録順序:** バンドル WASM → FS 発見 WASM (同 ID で上書き可能) → ユーザーコールバック

**参照実装:** `examples/` (ネイティブ) と `kasane-wasm/guests/` (WASM) がプラグイン作者向けの実装例として機能する。

---

## 判断フローチャート

```
機能 F を追加したい
  │
  ▼
1. プロトコル変更が必要か？
  │  Yes → 上流 (upstream-dependencies.md に記録)
  │  No ↓
  ▼
2. 唯一の正しい実装が存在するか？
  │  Yes → コア (kasane-core)
  │  No ↓
  ▼
3. プラグイン
  │  デフォルトで必要？ → バンドル WASM (kasane-wasm/guests/ + bundled/)
  │  そうでなければ → 外部プラグイン (WASM or ネイティブ)
  │  API が不足？ → Plugin trait / WIT の拡張が先
```

---

## Shared Plugin API Validation

Phase 4 は **WASM から到達可能な共有 Plugin API** の妥当性検証を扱う。
proof artifact は配布用 sample に限定せず、WASM fixture、`examples/`、統合テスト内 plugin を等価に扱う。

| Shared Extension Point | Proof Artifact | 状態 |
|------------------------|----------------|------|
| `contribute_to(SlotId::BUFFER_LEFT)` | color_preview (ガタースウォッチ), line-numbers (行番号) | 実証済み |
| `contribute_to(SlotId::STATUS_RIGHT)` | sel-badge (選択数バッジ) | 実証済み |
| `annotate_line_with_ctx()` | cursor_line (行背景ハイライト), color_preview (ガタースウォッチ) | 実証済み |
| `contribute_overlay_with_ctx()` | color_preview (カラーピッカー) | 実証済み |
| `handle_mouse()` | color_preview (色値編集) | 実証済み |
| `handle_key()` | `kasane-core/tests/plugin_integration.rs` の test plugin | 実証済み |
| `transform_menu_item()` | `kasane-core/tests/plugin_integration.rs` の test plugin | 実証済み |
| `contribute_to(SlotId::OVERLAY)` | 内部使用 (info/menu) | 実装済み (外部 plugin proof は未) |
| `contribute_to(SlotId::BUFFER_RIGHT)` | — | 未実証 (上流ブロッカーで完全版は先送り) |
| `contribute_to(SlotId::ABOVE_BUFFER / BELOW_BUFFER)` | — | 未実証 |
| `transform(TransformTarget::Buffer)` | — | メカニズム存在、proof artifact なし |
| `cursor_style_override()` | — | メカニズム存在、proof artifact なし |
| `contribute_to(SlotId::Named(...))` | — | メカニズム存在、proof artifact なし |
| `OverlayAnchor::Absolute` | 内部使用 (メニュー/検索バー) | インフラ実装済み (共有 API proof は未) |

## Native Escape Hatches

native-only API は shared validation とは別に扱う。長期方針は WASM parity だが、同じ trait をそのまま WIT へ公開することは目標にしない。

| Native-only API | 現在位置づけ | parity 方針 |
|-----------------|--------------|-------------|
| `PaintHook` | 暫定 escape hatch | `CellGrid` 直操作ではなく高レベル render hook へ再設計が必要 |
| `Surface` / `SURFACE_PROVIDER` | native-only だが parity target | hosted surface model として WASM から扱える抽象へ再設計する |
| `Pane` / `Workspace` 高度 API | native-only だが parity target | object access ではなく command / observer モデルで parity を目指す |

---

## 具体例: 項目の分類

| 項目 | 分類 | 判断理由 |
|------|------|----------|
| D-001 | 上流確認中 → コア候補 | 上流挙動の検証後、最小限のコア実装 (TEA update() キューイング)。唯一の正しい実装になりうる |
| R-050 | コア | 複数カーソル描画はプロトコル由来の face 解析であり、唯一の正しい実装。ただし Primary/Secondary 区別は PR #4707 待ち |
| R-051 | コア (✓ 実装済み) | ウィンドウフォーカス検知はフロントエンドネイティブ能力。唯一の正しい実装 |
| D-002 | 上流依存 | `draw` メッセージにカーソル総数が含まれないため、ビューポート外カーソルの正確な検出が不可能 |
| R-053 | コア | プロトコルが送るテキスト装飾の忠実描画はフロントエンド描画系の責務であり、唯一の正しい実装 |
| P-002 の実証 | プラグイン | `OverlayAnchor::Absolute` の proof artifact。WASM ゲストや統合テスト内 plugin で実装可能 |
| 行 / 範囲 decoration の実証 | プラグイン | `annotate_line_with_ctx()` や `transform()` による proof artifact。WASM ゲストや統合テスト内 plugin で実装可能 |
| P-023 の実装 | コア | D&D は GUI バックエンド (winit) のネイティブ能力。唯一の正しい実装 |

---

## 既存「解決層」との対応

[requirements-traceability.md](./requirements-traceability.md) の解決層との関係:

| | 解決層 (HOW) | 三層モデル (WHERE) |
|---|---|---|
| **問い** | どの仕組みで解決するか？ | どのレイヤーが責任を持つか？ |
| **分類** | レンダラ / 設定 / 基盤 / プロトコル制約 | 上流 / コア / プラグイン |
| **例** | R-050 → レンダラ (ソフトウェアレンダリング) | R-050 → コア (唯一の正しい実装) |
| **例** | 行 / 範囲 decoration → 基盤 (Transform) | 行 / 範囲 decoration の実証 → プラグイン |

両方の軸が機能分類に必要:
- **解決層**は実装の技術的メカニズムを決定する
- **三層モデル**はコードの配置場所と責務境界を決定する

## 関連文書

- [requirements-traceability.md](./requirements-traceability.md) — 解決層 (HOW) の追跡
- [semantics.md](./semantics.md) — 現行意味論
- [architecture.md](./architecture.md) — システム境界
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流依存の追跡
