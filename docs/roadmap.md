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
- `#[kasane::component]` — Phase 1 バリデーション (パススルー)。段階 1〜3 の最適化は将来

**未実装 (Phase 4 以降):**
- R-027: 起動時 info キューイング (TEA update() キューイング)
- R-050: 複数カーソルソフトウェアレンダリング (完全実装)
- R-052: 画面外カーソルインジケータ (Slot/Decorator)
- R-062: draw_status からのコンテキスト推定 (ヒューリスティック)
- コンパイラ駆動最適化 (ADR-010 段階 1〜3: プロファイリング結果に基づき判断)

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
- `#[kasane::component]` — バリデーションパススルー (将来の最適化用)
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

プラグインシステムを実プラグインで実証する。

### Phase 4a — 先送り項目消化 + 拡張プラグイン実証

プラグインシステム (Slot/Decorator/Replacement) を実プラグインで検証し、API の妥当性を実証する。

**先送り項目:**
- R-027: 起動時 info キューイング
- R-050: 複数カーソルソフトウェアレンダリング (完全実装)
- R-052: 画面外カーソルインジケータ (Slot/Decorator で実装 — プラグインの実用例)
- R-062: draw_status からのコンテキスト推定

**最初の拡張プラグイン (プラグインシステム実証):**
- E-002: ガターアイコン → `Slot::BufferLeft` の実証
- E-020: スクロールバー → `Slot::BufferRight` の実証
- E-001: オーバーレイレイヤー → `Slot::Overlay` + `Decorator(Buffer)` の実証

**パフォーマンス改善 (計測結果に基づく):**
- DirtyFlags per-component skip — ステータスのみの変更時にバッファ再描画をスキップ
- Container fill fast-path — per-cell `put_char()` を `fill_row()` に置換し背景塗りを高速化
- ADR-010 Stage 1 準備 — 実プラグインを使い `#[kasane::component]` の入力メモ化を検証
- アロケーション削減 — `atoms.to_vec()` と `atom.contents.clone()` のホットスポット対策
- diff() 最適化検討 — 196 KB/frame の CellDiff アロケーション削減 (streaming diff 等)

### Phase 4b — 残りの拡張機能

**対象要件:** E-001〜E-006, E-010〜E-012, E-020〜E-023, E-030〜E-031, E-040〜E-041

**解決する Issue カテゴリ:**
- 仮想テキスト/オーバーレイ (8件)
- ウィンドウ管理 (3件)
- スクロールバー/ミニマップ (3件)
- コード折りたたみ (1件)
- 表示行ナビゲーション (4件)
- ドラッグ＆ドロップ/URL (2件)
- フォント/テキストスタイル (2件)
