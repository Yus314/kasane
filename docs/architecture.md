# アーキテクチャ設計書

> **注:** 本ドキュメントは現在の命令的アーキテクチャを記述する。ADR-009 により宣言的 UI 基盤への移行が決定済み。新アーキテクチャの詳細は [declarative-ui.md](./declarative-ui.md) を参照。移行完了後に本ドキュメントを統合する。

## システム構成

```
┌──────────────────────────────────────────────────────────┐
│                   Kasane (フロントエンド)                   │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │                 kasane-core                         │  │
│  │  JSON-RPC パーサー / 状態管理 / レイアウトエンジン    │  │
│  │  入力マッピング / 設定管理 / RenderBackend trait     │  │
│  └──────────┬───────────────────────┬─────────────────┘  │
│             │                       │                    │
│  ┌──────────▼──────────┐ ┌─────────▼────────────────┐   │
│  │    kasane-tui        │ │     kasane-gui           │   │
│  │  (crossterm 直接)    │ │ (winit + wgpu +          │   │
│  │  セルグリッド管理     │ │  cosmic-text)            │   │
│  │  差分描画            │ │ GPU テキストレンダリング   │   │
│  └──────────────────────┘ └──────────────────────────┘   │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  ┌──────┐ ┌──────────────────────────────────────┐ │  │
│  │  │ガター │ │         メインバッファ表示            │ │  │
│  │  │行番号 │ │                                      │ │  │
│  │  │折畳   │ │   ┌─────────────────────┐           │ │  │
│  │  │アイコン│ │   │ フローティング       │           │ │  │
│  │  │      │ │   │ ウィンドウ (メニュー/ │  ┌─────┐ │ │  │
│  │  │      │ │   │ info / ポップアップ)  │  │スク │ │ │  │
│  │  │      │ │   └─────────────────────┘  │ロール│ │ │  │
│  │  │      │ │                             │バー  │ │ │  │
│  │  └──────┘ └──────────────────────────────┴─────┘ │ │  │
│  │  [ステータスバー / コマンドパレット / 通知エリア]     │ │  │
│  └────────────────────────────────────────────────────┘  │
│           ▲ 描画                │ キー/マウス入力          │
│           │ (stdout)            ▼ (stdin)                 │
│  ┌────────────────────────────────────────────────────┐  │
│  │             Kakoune (エディタエンジン)                │  │
│  │             kak -ui json                             │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

## crate 構成

```
kasane/
├── flake.nix                   # Nix devShell 定義 (Rust ツールチェイン + 依存)
├── flake.lock                  # Nix 依存のロックファイル
├── .envrc                      # direnv 連携 (use flake)
├── rust-toolchain.toml         # Rust ツールチェインバージョン固定
├── Cargo.toml                  # [workspace]
├── kasane-core/                # プロトコル・状態管理・レイアウト・抽象描画・プラグイン基盤
│   └── src/
│       ├── protocol.rs         # JSON-RPC パーサー、メッセージ型定義
│       ├── state.rs            # アプリケーション状態管理 (TEA: State + Msg + update)
│       ├── element.rs          # Element ツリー型定義 (宣言的 UI の中核)
│       ├── plugin.rs           # Plugin trait、PluginRegistry、Slot/Decorator/Replacement
│       ├── layout/             # レイアウトエンジン (Flex + Overlay + Grid)
│       │   ├── flex.rs         # Flexbox レイアウト計算
│       │   ├── overlay.rs      # Overlay/Stack 配置 (compute_pos 統合)
│       │   └── grid.rs         # Grid レイアウト計算
│       ├── input.rs            # 入力イベント → Kakoune キー変換
│       ├── config.rs           # TOML 設定パーサー
│       └── render.rs           # RenderBackend trait、paint()、CellGrid 差分描画
├── kasane-tui/                 # crossterm ベースの TUI バックエンド
│   └── src/
│       ├── backend.rs          # RenderBackend の TUI 実装
│       ├── cell_grid.rs        # セルグリッド管理、差分描画
│       └── input.rs            # crossterm イベント変換
├── kasane-gui/                 # Phase 4: winit + wgpu + cosmic-text
│   └── src/
│       ├── backend.rs          # RenderBackend の GUI 実装
│       ├── renderer.rs         # GPU テキストレンダリング
│       └── input.rs            # winit イベント変換
└── kasane/                     # メインバイナリ (CLI パース、バックエンド選択)
    └── src/main.rs
```

## 通信プロトコル

- **プロトコル:** JSON-RPC 2.0 (位置パラメータのみ)
- **Kakoune → Kasane (stdout):** 描画命令 (`draw`, `draw_status`, `menu_show`, `info_show` 等)
- **Kasane → Kakoune (stdin):** 入力イベント (`keys`, `resize`, `mouse_press` 等)
- **起動方法:** `kak -ui json` を子プロセスとして起動し、stdin/stdout をパイプで接続

プロトコルの詳細仕様は [json-ui-protocol.md](./json-ui-protocol.md) を参照。

## 設定

- **静的設定:** `~/.config/kasane/config.toml` — テーマ、フォント、GUI 設定、デフォルト動作
- **動的設定:** Kakoune `set-option global ui_options kasane_*=*` — ランタイムで変更可能な UI 挙動
- **起動:** `kasane` (TUI) / `kasane --ui gui` (GUI) / `kasane -c SESSION` (既存セッション接続)

## 抽象化の境界

コアが管理するのは「何を、どこに表示するか」。バックエンドが担当するのは「どう描画するか」。

### 宣言的 UI レイヤーの責務

ADR-009 により、kasane-core に宣言的 UI レイヤーが追加される。詳細は [declarative-ui.md](./declarative-ui.md) を参照。

| コンポーネント | 担当 | 説明 |
|--------------|------|------|
| Element ツリー構築 | kasane-core | view(&State) → Element。プラグインの Slot/Decorator/Replacement を合成 |
| レイアウト計算 | kasane-core | Flex + Overlay + Grid。Element ツリーからセル座標を計算 |
| paint() | kasane-core | Element + LayoutResult → CellGrid への描画 |
| プラグイン dispatch | kasane-core | TEA の update() 内でプラグインの状態更新・イベントルーティング |
| InteractiveId ヒットテスト | kasane-core | レイアウト結果を使ってマウスイベントの対象を特定 |

### バックエンドの責務

| コンポーネント | kasane-core | kasane-tui | kasane-gui |
|--------------|-------------|------------|------------|
| JSON-RPC パース | 担当 | — | — |
| 状態管理 (TEA) | 担当 | — | — |
| Element ツリー構築 | 担当 | — | — |
| レイアウト計算 | 担当 | — | — |
| CellGrid への paint | 担当 | — | — |
| CellGrid → 端末出力 | — | crossterm | wgpu + glyphon |
| ボーダー描画 | — | 罫線文字 | GPU 描画 (角丸/シャドウ) |
| キー入力取得 | — | crossterm | winit |
| マウス入力取得 | — | crossterm | winit |
| クリップボード | — | OSC 52 / arboard | arboard (ネイティブ) |
| IME | — | ターミナル経由 | winit + 自前 |
| D&D | — | 不可 | winit |
