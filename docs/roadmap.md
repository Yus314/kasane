# 実装ロードマップ

## Phase 0 — プロジェクト基盤

コードを書き始める前の開発環境・CI 基盤の整備。

**タスク:**
- `flake.nix` — Rust ツールチェイン (rustc, cargo, clippy, rustfmt) + システム依存ライブラリの Nix devShell 定義
- `.envrc` — `use flake` による direnv 連携
- `rust-toolchain.toml` — Rust ツールチェインバージョンの固定
- `Cargo.toml` — workspace 定義
- `.gitignore` — target/, result 等の除外設定

## Phase 1 — MVP (TUI コア機能 + 宣言的 UI 基盤) ✓ 完了

Kakoune の標準ターミナル UI を置き換え可能な最小限の実装。同時に、ADR-009 の宣言的 UI 基盤 (Element + TEA + Plugin trait + Slot) を確立する。詳細は [plugin-development.md](./plugin-development.md) を参照。

**状態:** commit `32f1adc` で完了。

**対象要件:** R-001〜R-009, R-010〜R-013, R-020〜R-022, R-030〜R-033, R-040〜R-045, R-060, R-070〜R-071

**解決する Issue カテゴリ:**
- ちらつき/再描画の根絶 (5件)
- True Color の一貫した表示 (4件)
- Unicode/CJK/絵文字の正常表示 (7件)
- ターミナル互換性問題の全面解消 (7件)
- カーソルレンダリングの基本改善 (2件)

## Phase 2 — 強化フローティングウィンドウ + プラグイン基盤 ✓ 完了 (一部先送り)

Kasane のコア差別化要因となるフローティングウィンドウの高度な機能とプラグイン基盤インフラの構築。

詳細は [plugin-development.md](./plugin-development.md) を参照。

**対象要件:** R-014〜R-016, R-023〜R-028, R-050〜R-052, R-061〜R-064

**達成済み:**
- R-014: メニュー配置カスタマイズ (Auto/Above/Below)
- R-015: 検索補完ドロップダウン (垂直リスト表示)
- R-016: イベントバッチング (マクロ再生フラッシュ抑制)
- R-023: 複数ポップアップ同時表示 (InfoIdentity による同一性推定)
- R-024: スクロール可能ポップアップ (マウスホイール対応)
- R-025: 選択範囲衝突回避 (compute_pos の `&[Rect]` 汎化)
- R-026: カスタマイズ可能ボーダー (Single/Rounded/Double/Heavy/Ascii)
- R-028: 統一デザインシステム (StyleToken + Theme)
- R-061: ステータスバー位置 (上部/下部)
- R-063: マークアップレンダリング (`{face_spec}text{default}`)
- R-064: カーソル数バッジ (FINAL_FG+REVERSE ヒューリスティック)
- InteractiveId + マウスヒットテスト (Z-order 逆順走査)

**Phase 3 で達成済み (当初は先送り):**
- proc macro (`#[kasane::plugin]`) — State/Event/Slot/Decorator/Replace のコード生成が完全動作
- Decorator / Replacement メカニズム — PluginRegistry に統合、view.rs で全 Slot/Decorator/Replacement を使用
- `#[kasane::component]` — `deps()` アノテーション + AST ベースフィールドアクセス検証 (ADR-010 Stage 2 で本格実装)

**未実装 (Phase 4 以降):**
- R-027: 起動時 info キューイング (TEA update() キューイング)
- ~~R-051: フォーカス連動カーソル~~ → Phase 4a で達成済み
- R-052: 画面外カーソルインジケータ → [上流依存](./upstream-dependencies.md)に分離
- R-062: draw_status からのコンテキスト推定 → [上流依存](./upstream-dependencies.md)に分離

**解決する Issue カテゴリ:**
- 情報ポップアップの全制限 (6件)
- 補完メニューの全制限 (8件)
- カーソルレンダリング強化 (2件)
- ステータスバーのカスタマイズ (5件)

## Phase 3 — 拡張入力・クリップボード・プラグイン基盤完成 ✓ 完了

操作性向上のための入力処理強化。加えて、Phase 2 で先送りされていたプラグイン基盤 (proc macro, Decorator/Replacement) を完成。

**状態:** commit `3bd19b7` で完了。

**対象要件:** R-046〜R-047, R-080〜R-082, R-090〜R-093

**達成済み:**
- R-046: 選択中スクロール (DragState 追跡、座標計算)
- R-047: 右クリックドラッグ (選択範囲拡張)
- R-080: システムクリップボード連携 (arboard via RenderBackend trait)
- R-081: 高速ペースト (ブラケットペースト検出、キーへの変換)
- R-082: 特殊文字の正確な処理 (シェルエスケープ不要)
- R-090: スムーズスクロール (60fps アニメーションティック)
- R-091〜R-093: スクロール改善 (PageUp/PageDown インターセプト)
- proc macro (`#[kasane::plugin]`) — 完全動作
- Decorator / Replacement メカニズム — PluginRegistry に統合
- `#[kasane::component]` — `deps()` アノテーション + AST ベースフィールドアクセス検証 (ADR-010 Stage 2)
- ClipboardConfig, MouseConfig, ScrollConfig 設定拡張

**解決する Issue カテゴリ:**
- マウス操作改善 (4件)
- クリップボード統合 (4件)
- スクロール動作改善 (6件)

## Phase G — GUI バックエンド ✓ 完了

winit + wgpu + glyphon による GPU バックエンド。技術選定は [ADR-014](./decisions.md#adr-014-gui-技術スタック--winit--wgpu--glyphon) を参照。

**Phase G1: MVP — ✓ 完了 (commit 43acdc0)**
- セル描画 (背景+テキスト+カーソル)、キー入力、リサイズ、HiDPI、設定、CLI (`--ui gui`)

**Phase G2: マウス・クリップボード・VSync — ✓ 完了**
- マウス入力 (ピクセル→グリッド座標変換)、クリップボード (arboard)、VSync スムーズスクロール

**Phase G3: ボーダー・シャドウ — ✓ 完了**
- GPU ボーダー描画 (角丸矩形シェーダー)、シーンベース描画パイプライン (DrawCommand + SceneRenderer)

## Phase 4 — 拡張機能実証

プラグインシステムを実プラグインで実証し、コアレンダリングの残り機能を完成させる。

> **スコープ方針:** 上流 (Kakoune) にブロックされている項目はロードマップから分離し、[upstream-dependencies.md](./upstream-dependencies.md) で追跡する。レイヤー責務の判断基準は [layer-responsibilities.md](./layer-responsibilities.md) を参照。

### レイヤー判断基準

機能の所属レイヤーは[三層レイヤー責務モデル](./layer-responsibilities.md) ([ADR-012](./decisions.md#adr-012-四層レイヤー責務モデル)) で判断する:

1. プロトコル変更が必要か？ → **上流** ([upstream-dependencies.md](./upstream-dependencies.md) に記録)
2. 唯一の正しい実装が存在するか？ → **コア** (kasane-core パイプライン内部)
3. それ以外 → **プラグイン** (バンドル WASM / FS 発見 WASM / ネイティブ)

現在の実証状況:

| Extension Point | 実証プラグイン | 状態 |
|-----------------|---------------|------|
| `Slot::BufferLeft` | color_preview (ガタースウォッチ), line-numbers (行番号) | ✓ |
| `Slot::StatusRight` | sel-badge (選択数バッジ) | ✓ |
| `contribute_line()` | cursor_line (行背景ハイライト), color_preview (ガタースウォッチ) | ✓ |
| `contribute_overlay()` | color_preview (カラーピッカー) | ✓ |
| `handle_mouse()` | color_preview (色値編集) | ✓ |
| `Slot::Overlay` | 内部使用 (info/menu) | ✓ (プラグインとしては未実証) |
| `Slot::BufferRight` | — | 未実証 (上流ブロッカーで先送り) |
| `Slot::BufferTop` / `BufferBottom` | — | 未実証 |
| `Decorator` | — | WASM v0.3.0 で公開済み、実プラグインなし |
| `Replacement` | — | WASM v0.3.0 で公開済み、実プラグインなし |
| `transform_menu_item()` | — | WASM v0.3.0 で公開済み、実プラグインなし |
| `cursor_style_override()` | — | メカニズム存在 (ネイティブ + WASM v0.4.0)、実プラグインなし |
| `contribute_named_slot()` | — | メカニズム存在 (ネイティブ + WASM v0.4.0)、実プラグインなし |
| `OverlayAnchor::Absolute` | 内部使用 (メニュー/検索バー) | ✓ インフラ実装済み (プラグインとしては未実証) |

### Phase 4a — 先送り項目消化 + 拡張プラグイン実証 (部分完了)

プラグインシステム (Slot/Decorator/Replacement) を実プラグインで検証し、API の妥当性を実証する。

**プラグインシステム実証 — ✓ 完了:**
- cursor_line (バンドル WASM): `contribute_line()` によるカーソル行背景ハイライト
- color_preview (バンドル WASM): `Slot::BufferLeft` (ガタースウォッチ) + `contribute_overlay()` (インタラクティブカラーピッカー) + `handle_mouse()` (色値編集)
- マルチプラグインガター合成: 複数プラグインの `contribute_line()` 結果を水平合成
- 内部プラグイン (`kasane-core/src/plugins/`) は WASM バンドルに移行し削除済み

**ADR-010 コンパイラ駆動最適化 — ✓ 完了:**
- Stage 1: DirtyFlags ベース view メモ化 — ViewCache, ComponentCache\<T\>, DirtyFlags u16 化, MENU→MENU_STRUCTURE+MENU_SELECTION 分割
- Stage 2: 検証済み依存追跡 — `#[kasane::component(deps(FLAG, ...))]` proc macro, AST ベースフィールドアクセス解析, FIELD_FLAG_MAP
- Stage 3: SceneCache — セクション別 DrawCommand キャッシュ, GUI カーソルアニメーション最適化
- Stage 4: コンパイル済み PaintPatch — StatusBarPatch (~80 cells), MenuSelectionPatch (~10 cells), CursorPatch (2 cells), LayoutCache, セクション別再描画

**先送り項目から達成済み:**
- R-051: フォーカス連動カーソル — `cursor_style()` のフォーカスチェック + crossterm `FocusLost` / winit `Focused(false)` イベント変換 + テスト

**先送り項目から達成済み (Phase 4b):**
- R-050: 複数カーソルソフトウェアレンダリング — Draw メッセージから FINAL_FG+REVERSE atom 座標を抽出、SetCursor と突合してセカンダリを判定、REVERSE 除去 + bg ブレンド (40% cursor / 60% bg) で差別化。TUI・GPU 両パイプライン対応

**先送り項目 (未実装):**
- R-027: 起動時 info キューイング
- R-052: 画面外カーソルインジケータ → [上流依存](./upstream-dependencies.md)に分離

### Phase 4b — コアレンダリング完成 + 未実証 Plugin API の検証

Phase 4a の先送りコア項目を消化しつつ、まだ実プラグインで検証されていない Plugin API extension point を WASM ゲストプラグインで実証する。

**コアレンダリング拡張 (プラグインではない — パイプライン内部の実装):**

| 項目 | 内容 | 実装方針 |
|------|------|----------|
| R-027 | 起動時 info キューイング | 上流挙動 ([#5294](https://github.com/mawww/kakoune/issues/5294)) 確認後、最小限のコア実装。TEA update() にキューを導入し、UI 準備完了前の `info_show` を保持 |
| ~~R-050~~ | ~~複数カーソル描画~~ | **達成済み** — Draw メッセージの FINAL_FG+REVERSE atom 座標を抽出、SetCursor と突合してセカンダリ判定、REVERSE 除去 + bg ブレンドで差別化。TUI・GPU 両パイプライン対応 |

> **Note:** E-040 (アンダーラインバリエーション) は[上流依存](./upstream-dependencies.md)に移動。Face の underline 属性が on/off のみのため、プロトコル変更が必要。

**プラグイン (未実証 API の検証):**

| 項目 | 実証する API | 内容 |
|------|------------|------|
| E-006 | `contribute_line()` 拡張 (選択範囲) | 改行を含む選択範囲をウィンドウ幅いっぱいまでハイライト。cursor_line ゲストの拡張として実装可能 |
| E-005 | `OverlayAnchor::Absolute` | コアインフラは実装済み (型定義、レイアウト、描画、ヒットテスト、メニュー/検索バーで内部使用)。残作業は WASM ゲストでの実証。ビューポート座標に対するオーバーレイ描画。easymotion ジャンプラベル等のユースケース。論点: バッファ→ビューポート座標変換 API の公開要否 |

> **Note:** R-052 (画面外カーソルインジケータ) は[上流依存](./upstream-dependencies.md)に移動。`draw` メッセージにカーソル総数が含まれないため。

**GUI バックエンド拡張:**

| 項目 | 内容 |
|------|------|
| E-030 | ファイルドラッグ＆ドロップ — winit の `WindowEvent::DroppedFile` を受信し `:edit {path}` を Kakoune に送信 |

**解決する Issue カテゴリ:**
- カーソルレンダリングの完成 ([#5377](https://github.com/mawww/kakoune/issues/5377), [#3652](https://github.com/mawww/kakoune/issues/3652))
- 起動時 info 消失の修正 ([#5294](https://github.com/mawww/kakoune/issues/5294))
- 選択範囲表示の改善 ([#1909](https://github.com/mawww/kakoune/issues/1909))
- ファイルドロップ ([#3928](https://github.com/mawww/kakoune/issues/3928))

## Phase W — WASM プラグインランタイム ✓ 基盤完了

WASM Component Model によるランタイムプラグインロード基盤。リビルド不要でのプラグインインストール・有効化を実現する。

> **ネイティブプラグイン:** コンパイル時結合方式も引き続きサポート。`kasane-core::plugin_prelude` + `#[kasane_plugin]` マクロで外部クレートからプラグインを定義し、`kasane::run(|registry| { ... })` でカスタムバイナリとして配布できる。詳細は [plugin-development.md](./plugin-development.md) および `examples/line-numbers/` を参照。ネイティブプラグインは PaintHook, Surface, Workspace hooks 等の WIT 未公開 API のためのエスケープハッチとして維持する。

**方式決定:** [ADR-013](./decisions.md#adr-013-wasm-プラグインランタイム--component-model-採用) により **WASM Component Model (wasmtime)** を採用。

Phase W0 のベンチマーク実証で以下を確認:
- 10 プラグイン フルフレーム: **18 μs** (フレーム予算 40 μs の 45%)
- プラグインあたりコスト: **~1.8 μs** (線形スケーリング)
- DirtyFlags キャッシュヒット時: **0.26 ns** (WASM 呼び出しを完全スキップ)
- Component Model の DX: WIT 型安全、canonical ABI 自動シリアライゼーション、言語非依存

**候補アーキテクチャの評価結果:**

| 方式 | 長所 | 短所 | 評価 |
|------|------|------|------|
| ダイナミックリンク (`cdylib` + FFI) | Rust プラグインの性能維持、ABI 安定化のみ必要 | ABI 互換性の維持が困難、バージョン管理が複雑 | ❌ 却下 |
| **WASM Component Model** | **サンドボックス安全性、言語非依存、WIT 型安全** | **~500 ns/call 固定オーバーヘッド** | **✅ 採用** |
| スクリプト言語 (Lua 等) | 設定ファイル的に書ける、学習コスト低 | 性能、型安全性の喪失 | ❌ 却下 |
| プロセス分離 (JSON-RPC) | 完全な言語非依存、クラッシュ耐性 | IPC オーバーヘッド、フレーム内での応答遅延 | ❌ 却下 |

### Phase W1 — 基盤 ✓ 完了

- WIT インターフェース v0.2.0 (Plugin trait 相当を WIT で定義)
- ホスト関数パターン (ゲスト→ホスト呼び出しで AppState 参照)
- WASM プラグインの読み込み・初期化・シャットダウンのライフサイクル管理
- cursor_line, color_preview をバンドル WASM プラグインとして移行 (`kasane-core/src/plugins/` 削除)
- `register_bundled_plugins()` による `include_bytes!` 埋め込みロード
- FS 自動発見 (`~/.local/share/kasane/plugins/*.wasm`)
- 登録順序: バンドル WASM → FS 発見 WASM (上書き可能) → ユーザーコールバック

### Phase W2 — ネイティブ Plugin trait との API パリティ ✓ 完了

**WIT v0.3.0:**
- Decorator / Replacement / transform_menu_item の WIT 公開
- host-state Tier 1-3: ステータスバー状態 (`get-status-prompt` 等)、メニュー/info 状態 (`has-menu`, `get-menu-item` 等)、一般状態 (`get-ui-option`, `get-default-face` 等)
- 高度 Element ビルダー: `create-container-styled`, `create-scrollable`, `create-stack`
- プラグイン間メッセージング (`update`)

**WIT v0.4.0:**
- `cursor-style-override`: プラグインからカーソルスタイルを上書き
- `contribute-named`: カスタム名前付きスロットへの Element 提供
- `create-container-custom-border`: 任意のボーダー文字によるコンテナ
- host-state Tier 4: マルチカーソル (`get-cursor-count`, `get-secondary-cursor` 等)
- host-state Tier 5: 設定 API (`get-config-string`)
- host-state Tier 6: Info 内容 (`get-info-title`, `get-info-content`, `get-info-anchor` 等)
- host-state Tier 7: Menu 詳細 (`get-menu-anchor`, `get-menu-style`, `get-menu-face` 等)

**バンドルプラグイン:**
- cursor_line: カーソル行背景ハイライト (`contribute_line()`)
- color_preview: カラーコード検出 + インタラクティブピッカー (`Slot::BufferLeft` + `contribute_overlay()` + `handle_mouse()`)
- sel-badge: 選択数バッジ (`Slot::StatusRight` 実証)

**残作業:**
- プラグインマニフェスト (名前、バージョン、依存、使用する extension point)
- プラグインの設定 API (`config.toml` との統合)
- コンパイル済みコンポーネントのキャッシュ (`Engine::precompile_component`) による起動高速化

## Phase 5 — Surface・Workspace 拡張性基盤

### 5a: 拡張性アーキテクチャ基盤 ✓ 完了

Plugin trait とレンダリングパイプラインを Surface/Workspace ベースのアーキテクチャに発展させるための基盤。コア UI コンポーネントとプラグインが対等に画面領域を所有し、レイアウトに参加できる設計。

**Surface Model:**
- `Surface` trait: `id()`, `size_hint()`, `view()`, `handle_event()`, `on_state_changed()`, `state_hash()`, `declared_slots()`
- `SurfaceId`: 定数定義 (BUFFER=0, STATUS=1, MENU=2, INFO_BASE=10, PLUGIN_BASE=100)
- `SurfaceRegistry`: Surface インスタンスと Workspace レイアウトツリーを管理。`compose_view()` / `compose_full_view()` で全 Surface を統合した Element ツリーを構築
- `ViewContext` / `EventContext`: Surface に渡されるコンテキスト (AppState, Rect, フォーカス状態, PluginRegistry)
- コアサーフェス実装: `KakouneBufferSurface`, `StatusBarSurface`, `MenuSurface` (エフェメラル), `InfoSurface` (エフェメラル)
- `sync_ephemeral_surfaces()`: AppState のメニュー/info 有無に応じてサーフェスを自動生成・破棄
- `all_declared_slots()` / `slot_owner()`: Surface が宣言したスロットの動的発見

**Workspace レイアウトツリー:**
- `WorkspaceNode`: Leaf / Split / Tabs / Float の 4 種ノード
- `Placement`: SplitFocused / SplitFrom / Tab / TabIn / Dock / Float (新サーフェスの配置指定)
- `Workspace`: ルートノード管理、フォーカストラッキング (履歴スタック)、`compute_rects()` / `surface_at()`
- `WorkspaceCommand`: AddSurface / RemoveSurface / Focus / FocusDirection / Resize / Swap / Float / Unfloat
- `WorkspaceQuery`: プラグインから workspace を読み取り専用で参照

**Plugin trait 拡張:**
- `PluginCapabilities` bitflags (14 種): 非参加プラグインの WASM 境界呼び出しをスキップ
- `SlotId` オープンスロットシステム: legacy `Slot` enum を deprecated にし、`SlotId::new("myplugin.sidebar")` でカスタムスロット定義可能。`contribute_slot()` / `slot_id_deps()` に移行
- `PaintHook` trait: paint 後の CellGrid 直接変更 (DirtyFlags ベース + Surface フィルタ)
- `cursor_style_override()`: プラグインからカーソルスタイルを上書き
- `overlay_deps()`: L3 オーバーレイキャッシュ最適化
- Pane hooks: `on_pane_created()`, `on_pane_closed()`, `on_focus_changed()`, `render_pane()`, `handle_pane_key()`, `pane_permissions()`
- Surface hooks: `surfaces()`, `workspace_request()`, `on_workspace_changed()`
- `Command::Pane`, `Command::Workspace`, `Command::RegisterThemeTokens`

### 5b: マルチペイン基盤拡張

5a のアーキテクチャ上でマルチペインプラグインが動作するために必要な最小限のコア拡張。マルチペインの具体的実装 (スプリット、フローティングパネル等) はプラグインに委ねる。

> **設計方針:** マルチペインに「唯一の正しい実装」は存在しない (tmux 風、IDE 風タブ、フローティング、タイリング WM 風等)。三層レイヤー責務モデルに従い、コアはプラットフォーム機能を提供し、具体的な UX はプラグインが決定する。
>
> **WASM 対応方針:** WASM プラグインはプロセス起動やスレッド管理ができないが、ホストが「マネージドセッション」をプリミティブとして提供すれば、WASM プラグインがオーケストレーション (いつ・どこに分割するか) を担える。ブラウザが `<iframe>` を提供し JavaScript がレイアウトを制御するのと同じ構造。

**ホスト提供セッション管理:**

ホストが Kakoune セッションのライフサイクル (プロセス起動、JSON-RPC 読み取り、状態保持、Surface 自動生成) を全て管理し、WASM/ネイティブ双方のプラグインが `command` 経由でセッションを操作する。

```
WASM プラグイン                      ホスト
─────────────                   ──────
handle_key(Ctrl+Shift+D)
  → Command::SpawnSession(cfg) ────→ kak -ui json をスポーン
                                     バックグラウンドスレッドで JSON-RPC 読み取り
                                     ペイン独自の AppState を保持
                                     KakouneBufferSurface を自動生成
                                     SurfaceRegistry + Workspace に登録
                                     compose_view() で統合描画
```

WIT の `command` variant に `spawn-session` / `close-session` を追加するだけで実現でき、WASM プラグイン側は ~20 行のオーケストレーションコードで分割ペインが動作する。セッション管理はホスト内部の問題であるため、WakeupHandle のようなスレッド間通信 API をプラグインに露出する必要がない。

**段階的拡張:**

| 段階 | 内容 | WASM |
|------|------|------|
| Stage 1 | `command` に `spawn-session` / `close-session` を追加。ホスト側に `SessionManager` (プロセス管理 + 独自 AppState + Surface 自動生成)。イベントループでセッション ID 付き Event を統合 | ✅ |
| Stage 2 | WIT `session-manager` インターフェース (`list-sessions`, `send-keys`, `get-session-surface`) を追加。セッション一覧 UI やリモートコマンド実行が可能に | ✅ |
| Stage 3 | 汎用プロセスサーフェス (`spawn-terminal` + VT100 エミュレーション)。fzf 等のフローティングターミナルパネルを WASM から操作可能に | ✅ |

**コア拡張タスク:**

| 項目 | 内容 |
|------|------|
| SessionManager | ホスト側のマネージドセッション管理。`HashMap<SessionId, ManagedSession>` で複数の `kak -ui json` プロセス + 独自 AppState + Surface を管理。イベントループで各セッションの Event を main channel に統合 |
| WIT command 拡張 | `spawn-session(session-config)` / `close-session(session-id)` を追加。ホストがプロセス起動・Surface 生成・Workspace 配置を実行 |
| WorkspaceCommand 完成 | FocusDirection / Resize / Swap / Float / Unfloat の dispatch 実装 (AddSurface / RemoveSurface / Focus は実装済み) |
| 分割境界描画 | Split ノードの 1-cell divider をドラッグ可能な UI として描画。マウスイベントによる Resize コマンド発行 |

### 5c: 外部プラグイン候補

以下の機能は、外部プラグインシステム上での実装が適切。Phase 4 + Phase W で実証済みの API、および 5a/5b のコア基盤で実装可能であり、組み込みにする理由がない。

| 項目 | 使用する API | WASM | 内容 |
|------|------------|------|------|
| E-003 | `contribute_line()` / `Decorator(Buffer)` | ✅ | インデントガイド — サブピクセル薄線でインデントレベルを表示 |
| E-004 | `contribute_overlay()` + `handle_mouse()` | ✅ | クリッカブルリンク — info ボックスやバッファ内の URL のクリック対応 |
| E-010 | `spawn-session` + `Workspace` | ✅ | ビルトインスプリット — 水平/垂直分割、任意レイアウト。ホストが Kakoune セッションを管理、プラグインは配置を指定 |
| E-011 | `spawn-terminal` + `Workspace` | ✅¹ | フローティングパネル — fzf/ファイルピッカー等のフローティングターミナル。ホストが PTY + VT100 エミュレーションを提供 |
| E-012 | `PaintHook` + `on_workspace_changed()` | ✅ | フォーカス視覚フィードバック — フォーカス/非フォーカスペインの視覚的区別 (減色、ボーダー色変更) |
| E-022 | `Decorator(Buffer)` + `Interactive` | ✅ | コード折りたたみ — 表示レベルでの行折りたたみ (折りたたみ範囲の情報ソースが課題: LSP? treesitter?) |
| E-023 | レンダラ層 | ✅ | 表示行ナビゲーション — ソフトラップ表示行単位のカーソル移動 (gj/gk)。Kakoune の行モデルとの複雑な相互作用あり |
| E-031 | レンダラ層 | ✅ | URL 検出 — バッファ内 URL の正規表現検出。E-004 の前提 |
| E-041 | レンダラ層 (GUI) | ✅ | 領域別フォントサイズ — インレイヒント小、見出し大 (glyphon フォントサイズ制御) |

¹ Stage 3 (汎用プロセスサーフェス) の実装が前提。VT100 エミュレーションはホスト側の大きな実装であり、需要に応じて判断する。

## パフォーマンストラック (Phase 非依存)

機能開発と並行して随時対応するパフォーマンス改善項目。現在 ~40 μs/frame (80×24) で十分高速だが、大規模バッファや高解像度ディスプレイでのスケーラビリティ確保のために継続的に改善する。

| 項目 | 内容 | 影響度 |
|------|------|--------|
| Container fill fast-path | per-cell `put_char()` を `fill_row()` に置換し背景塗りを高速化 | 中 — paint() のホットパス |
| アロケーション削減 | `atoms.to_vec()` と `atom.contents.clone()` のホットスポット対策 | 中 — apply() の GC 圧力 |
| diff() 最適化 | 196 KB/frame の CellDiff アロケーション削減 (streaming diff / in-place diff) | 高 — フレーム毎のアロケーション最大箇所 |
| PluginSlotCache L2+L4 | プラグイン slot 出力の深層キャッシュ (L1 state_hash + L3 slot_id_deps + overlay_deps は実装済み) | 低 — プラグイン数が増えてから |
