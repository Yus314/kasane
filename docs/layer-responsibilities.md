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
- スクロール位置 (E-020) — `draw` メッセージにスクロール位置が含まれない
- Atom 種別 (PR #4707) — 行番号/仮想テキスト/コードの区別がない
- カーソル総数 (R-052) — `draw` メッセージにカーソルの総数が含まれない
- アンダーラインバリエーション (E-040) — Face の underline 属性が on/off のみ

### コア (kasane-core)

**定義:** プロトコルの忠実なレンダリング + フロントエンドネイティブ能力。

**判断基準:** 「唯一の正しい実装が存在するか？」 — Yes ならコア。

**プロトコル描画:**
- `draw` / `menu_show` / `info_show` / `draw_status` の忠実なレンダリング
- レイアウト計算 (Flex + Overlay + Grid)
- 差分描画 (CellGrid → diff → backend)

**フロントエンドネイティブ:**
- フォーカス検知 (R-051) — ウィンドウフォーカスの得失はフロントエンド固有
- D&D (E-030) — GUI ウィンドウイベントはフロントエンド固有
- クリップボード (R-080〜R-082) — システム API 直接アクセス
- 複数カーソル描画 (R-050) — プロトコル由来の face 解析

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
| **ネイティブ** | `kasane::run(\|registry\| { ... })` でコンパイル時結合 | パフォーマンスクリティカル or Decorator/Replacement 使用 |

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

## Plugin API の実証状況

バンドル WASM プラグインおよびサンプルで実証済みの extension point:

| Extension Point | 実証プラグイン | 状態 |
|-----------------|---------------|------|
| `Slot::BufferLeft` | color_preview (ガタースウォッチ), line-numbers (行番号) | 実証済み |
| `Slot::StatusRight` | sel-badge (選択数バッジ) | 実証済み |
| `contribute_line()` | cursor_line (行背景ハイライト), color_preview (ガタースウォッチ) | 実証済み |
| `contribute_overlay()` | color_preview (カラーピッカー) | 実証済み |
| `handle_mouse()` | color_preview (色値編集) | 実証済み |
| `Slot::Overlay` | 内部使用 (info/menu) | 実証済み (プラグインとしては未実証) |
| `Slot::BufferRight` | — | 未実証 (上流ブロッカーで先送り) |
| `Slot::BufferTop` / `BufferBottom` | — | 未実証 |
| `Decorator(Buffer)` | — | WASM v0.3.0 で公開済み、実プラグインなし |
| `Replacement` | — | WASM v0.3.0 で公開済み、実プラグインなし |
| `transform_menu_item()` | — | WASM v0.3.0 で公開済み、実プラグインなし |
| `cursor_style_override()` | — | メカニズム存在 (ネイティブ + WASM v0.4.0)、実プラグインなし |
| `contribute_named_slot()` | — | メカニズム存在 (ネイティブ + WASM v0.4.0)、実プラグインなし |
| `OverlayAnchor::Absolute` | 内部使用 (メニュー/検索バー) | ✓ インフラ実装済み (プラグインとしては未実証) |

---

## 具体例: 項目の分類

| 項目 | 分類 | 判断理由 |
|------|------|----------|
| R-027 | 上流確認中 → コア | 上流挙動の検証後、最小限のコア実装 (TEA update() キューイング)。唯一の正しい実装 |
| R-050 | コア | 複数カーソル描画はプロトコル由来の face 解析であり、唯一の正しい実装。ただし Primary/Secondary 区別は PR #4707 待ち |
| R-051 | コア (✓ 実装済み) | ウィンドウフォーカス検知はフロントエンドネイティブ能力。唯一の正しい実装 |
| R-052 | 上流依存 | `draw` メッセージにカーソル総数が含まれないため、ビューポート外カーソルの正確な検出が不可能 |
| E-005 | プラグイン | `OverlayAnchor::Absolute` の実証。WASM ゲストとして実装可能 |
| E-006 | プラグイン | `contribute_line()` の拡張 (選択範囲ハイライト)。WASM ゲストとして実装可能 |
| E-040 | 上流依存 (保留) | Face の underline 属性が on/off のみ。バリエーション情報をプロトコルが送信しない限り利用不可 |
| E-030 | コア | D&D は GUI バックエンド (winit) のネイティブ能力。唯一の正しい実装 |

---

## 既存「解決層」との対応

[requirements-traceability.md](./requirements-traceability.md) の解決層との関係:

| | 解決層 (HOW) | 三層モデル (WHERE) |
|---|---|---|
| **問い** | どの仕組みで解決するか？ | どのレイヤーが責任を持つか？ |
| **分類** | レンダラ / 設定 / 基盤 / プロトコル制約 | 上流 / コア / プラグイン |
| **例** | R-050 → レンダラ (ソフトウェアレンダリング) | R-050 → コア (唯一の正しい実装) |
| **例** | E-006 → 基盤 (Decorator) | E-006 → プラグイン |

両方の軸が機能分類に必要:
- **解決層**は実装の技術的メカニズムを決定する
- **三層モデル**はコードの配置場所と責務境界を決定する

## 関連文書

- [requirements-traceability.md](./requirements-traceability.md) — 解決層 (HOW) の追跡
- [semantics.md](./semantics.md) — 現行意味論
- [architecture.md](./architecture.md) — システム境界
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流依存の追跡
