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

Kakoune の標準ターミナル UI を置き換え可能な最小限の実装。同時に、ADR-009 の宣言的 UI 基盤 (Element + TEA + Plugin trait + Slot) を確立する。詳細は [declarative-ui.md](./declarative-ui.md) の「段階的実装計画 Phase 1」を参照。

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

詳細は [declarative-ui.md](./declarative-ui.md) の「段階的実装計画 Phase 2」を参照。

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
- R-051: フォーカス連動カーソル
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

winit + wgpu + glyphon による GPU バックエンド。詳細は [gui-backend.md](./gui-backend.md) を参照。

**Phase G1: MVP — ✓ 完了 (commit 43acdc0)**
- セル描画 (背景+テキスト+カーソル)、キー入力、リサイズ、HiDPI、設定、CLI (`--ui gui`)

**Phase G2: マウス・クリップボード・VSync — ✓ 完了**
- マウス入力 (ピクセル→グリッド座標変換)、クリップボード (arboard)、VSync スムーズスクロール

**Phase G3: ボーダー・シャドウ — ✓ 完了**
- GPU ボーダー描画 (角丸矩形シェーダー)、シーンベース描画パイプライン (DrawCommand + SceneRenderer)

## Phase 4 — 拡張機能実証

プラグインシステムを実プラグインで実証し、コアレンダリングの残り機能を完成させる。

> **スコープ方針:** 上流 (Kakoune) にブロックされている項目はロードマップから分離し、[upstream-dependencies.md](./upstream-dependencies.md) で追跡する。また、組み込みプラグインは「未実証の Plugin API extension point を検証する」目的で作成し、既に実証済みの API 上で作れる機能は Phase 5 以降で外部プラグインとして実現する。レイヤー責務の判断基準は [layer-responsibilities.md](./layer-responsibilities.md) を参照。

### レイヤー判断基準

機能の所属レイヤーは[四層レイヤー責務モデル](./layer-responsibilities.md) ([ADR-012](./decisions.md#adr-012-四層レイヤー責務モデル)) で判断する:

1. プロトコル変更が必要か？ → **上流** ([upstream-dependencies.md](./upstream-dependencies.md) に記録)
2. 唯一の正しい実装が存在するか？ → **コア** (kasane-core パイプライン内部)
3. 未実証 API の検証を兼ねるか？ → **組み込みプラグイン** (kasane-core/src/plugins/)
4. 実証済み API で実装可能か？ → **外部プラグイン** (Phase 5)

> **Note:** 組み込みプラグインは公開 Plugin trait API のみを使用する (API パリティの原則)。内部状態が必要な機能はプラグインではなくコアに属するか、API 拡張が先。Phase 5 (外部プラグインシステム) が未実装の間、「ほぼ全ユーザーが期待する機能」を組み込みプラグインで提供することは現実的な妥協として許容する。

現在の実証状況:

| Extension Point | 実証済みプラグイン | 状態 |
|-----------------|-------------------|------|
| `Slot::BufferLeft` | ColorPreviewPlugin (ガタースウォッチ) | ✓ |
| `contribute_line()` | CursorLinePlugin (行背景ハイライト) | ✓ |
| `contribute_overlay()` | ColorPreviewPlugin (カラーピッカー) | ✓ |
| `handle_mouse()` | ColorPreviewPlugin (色値編集) | ✓ |
| `Slot::Overlay` | 内部使用 (info/menu) | ✓ (プラグインとしては未実証) |
| `Slot::BufferRight` | — | 未実証 (上流ブロッカーで先送り) |
| `Slot::BufferTop` / `BufferBottom` | — | 未実証 |
| `Decorator(Buffer)` | — | メカニズム存在、実プラグインなし |
| `Replacement` | — | メカニズム存在、実プラグインなし |
| `OverlayAnchor::Absolute` | 内部使用 (メニュー/検索バー) | ✓ インフラ実装済み (プラグインとしては未実証) |

### Phase 4a — 先送り項目消化 + 拡張プラグイン実証 (部分完了)

プラグインシステム (Slot/Decorator/Replacement) を実プラグインで検証し、API の妥当性を実証する。

**プラグインシステム実証 — ✓ 完了:**
- CursorLinePlugin: `contribute_line()` によるカーソル行背景ハイライト
- ColorPreviewPlugin: `Slot::BufferLeft` (ガタースウォッチ) + `contribute_overlay()` (インタラクティブカラーピッカー) + `handle_mouse()` (色値編集)
- マルチプラグインガター合成: 複数プラグインの `contribute_line()` 結果を水平合成

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

Phase 4a の先送りコア項目を消化しつつ、まだ実プラグインで検証されていない Plugin API extension point を組み込みプラグインで実証する。

**コアレンダリング拡張 (プラグインではない — パイプライン内部の実装):**

| 項目 | 内容 | 実装方針 |
|------|------|----------|
| R-027 | 起動時 info キューイング | 上流挙動 ([#5294](https://github.com/mawww/kakoune/issues/5294)) 確認後、最小限のコア実装。TEA update() にキューを導入し、UI 準備完了前の `info_show` を保持 |
| ~~R-050~~ | ~~複数カーソル描画~~ | **達成済み** — Draw メッセージの FINAL_FG+REVERSE atom 座標を抽出、SetCursor と突合してセカンダリ判定、REVERSE 除去 + bg ブレンドで差別化。TUI・GPU 両パイプライン対応 |

> **Note:** E-040 (アンダーラインバリエーション) は[上流依存](./upstream-dependencies.md)に移動。Face の underline 属性が on/off のみのため、プロトコル変更が必要。

**組み込みプラグイン (未実証 API の検証):**

| 項目 | 実証する API | 内容 |
|------|------------|------|
| E-006 | `contribute_line()` 拡張 (選択範囲) | 改行を含む選択範囲をウィンドウ幅いっぱいまでハイライト。CursorLinePlugin の拡張として実装可能 |
| E-005 | `OverlayAnchor::Absolute` | コアインフラは実装済み (型定義、レイアウト、描画、ヒットテスト、メニュー/検索バーで内部使用)。残作業は組み込みプラグインでの実証。ビューポート座標に対するオーバーレイ描画。easymotion ジャンプラベル等のユースケース。論点: バッファ→ビューポート座標変換 API の公開要否 |

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

## Phase 5 — マルチペイン + 外部プラグインシステム

### 5a: マルチペイン・ウィンドウ管理

tmux/WM に依存しない分割表示の実現。現在のアーキテクチャは単一 Kakoune セッション前提であり、根本的な変更が必要。

**前提の分析:**
- **TUI**: tmux が使えるため優先度は低い。ただし統合的な体験の提供として価値はある
- **GUI**: tmux 相当の分割手段がないため、**GUI ユーザーにとっては必須機能**
- **アーキテクチャ変更**: 複数の `kak -ui json` プロセスの同時管理、Flex レイアウトの多重化、フォーカス管理、セッション間通信

| 項目 | 内容 |
|------|------|
| E-010 | ビルトインスプリット — 水平/垂直分割、ドラッグ可能な境界、任意レイアウト。複数 Kakoune プロセスの同時管理 |
| E-011 | フローティングパネル — fzf/ファイルピッカー等のためのフローティングターミナルパネル。PTY 管理 + 仮想ターミナルエミュレーション |
| E-012 | フォーカス視覚フィードバック — フォーカス/非フォーカスペインの視覚的区別 (減色、ボーダー色変更)。E-010 前提 |

### 5b: 外部プラグインシステム (WASM)

> **現状:** コンパイル時結合方式の外部プラグインは実現済み。`kasane-core::plugin_prelude` + `#[kasane_plugin]` マクロで外部クレートからプラグインを定義し、`kasane::run(|registry| { ... })` でカスタムバイナリとして配布できる。詳細は [external-plugins.md](./external-plugins.md) および `examples/line-numbers/` を参照。
>
> 以下はランタイムプラグインロード（リビルド不要のインストール・有効化）の計画。

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

**Phase W1 で実現すること:**
- WIT インターフェース設計 (kasane の Plugin trait 相当を WIT で定義)
- ホスト関数パターンの確立 (ゲスト→ホスト呼び出しで AppState 参照)
- 外部プラグインの読み込み・初期化・シャットダウンのライフサイクル管理
- プラグインマニフェスト (名前、バージョン、依存、使用する extension point)
- プラグインの設定 API (`config.toml` との統合)
- Phase 4 で組み込みとして実装したプラグインの一部を WASM で外部化し、仕組みの検証
- コンパイル済みコンポーネントのキャッシュ (`Engine::precompile_component`) による起動高速化

> **Note:** ネイティブプラグイン (現行のコンパイル時結合方式) はパフォーマンスクリティカルな組み込みプラグイン向けの最適化エスケープハッチとして維持する。

### 5c: 外部プラグイン候補

以下の機能は、外部プラグインシステム上での実装が適切。Phase 4 で実証済みの API で実装可能であり、組み込みにする理由がない。

| 項目 | 使用する API (実証済み) | 内容 |
|------|----------------------|------|
| E-003 | `contribute_line()` / `Decorator(Buffer)` | インデントガイド — サブピクセル薄線でインデントレベルを表示 |
| E-004 | `contribute_overlay()` + `handle_mouse()` | クリッカブルリンク — info ボックスやバッファ内の URL のクリック対応 |
| E-022 | `Decorator(Buffer)` + `Interactive` | コード折りたたみ — 表示レベルでの行折りたたみ (折りたたみ範囲の情報ソースが課題: LSP? treesitter?) |
| E-023 | レンダラ層 | 表示行ナビゲーション — ソフトラップ表示行単位のカーソル移動 (gj/gk)。Kakoune の行モデルとの複雑な相互作用あり |
| E-031 | レンダラ層 | URL 検出 — バッファ内 URL の正規表現検出。E-004 の前提 |
| E-041 | レンダラ層 (GUI) | 領域別フォントサイズ — インレイヒント小、見出し大 (glyphon フォントサイズ制御) |

## パフォーマンストラック (Phase 非依存)

機能開発と並行して随時対応するパフォーマンス改善項目。現在 ~40 μs/frame (80×24) で十分高速だが、大規模バッファや高解像度ディスプレイでのスケーラビリティ確保のために継続的に改善する。

| 項目 | 内容 | 影響度 |
|------|------|--------|
| Container fill fast-path | per-cell `put_char()` を `fill_row()` に置換し背景塗りを高速化 | 中 — paint() のホットパス |
| アロケーション削減 | `atoms.to_vec()` と `atom.contents.clone()` のホットスポット対策 | 中 — apply() の GC 圧力 |
| diff() 最適化 | 196 KB/frame の CellDiff アロケーション削減 (streaming diff / in-place diff) | 高 — フレーム毎のアロケーション最大箇所 |
| PluginSlotCache L2+L4 | プラグイン slot 出力の深層キャッシュ (L1 state_hash + L3 slot_deps は実装済み) | 低 — プラグイン数が増えてから |
