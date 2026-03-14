# リポジトリ構成ガイド

本ドキュメントは、Kasane の workspace 構成と主要ディレクトリの責務を示す参照文書である。
システム境界や意味論を確認したい場合は [architecture.md](./architecture.md) と [semantics.md](./semantics.md) を参照。

## 1. ワークスペース概要

```text
kasane/
├── flake.nix
├── flake.lock
├── .envrc
├── rust-toolchain.toml
├── Cargo.toml
├── kasane-core/
├── kasane-tui/
├── kasane-macros/
├── kasane-gui/
├── kasane/
├── kasane-wasm/
├── kasane-plugin-sdk/
└── kasane-wasm-bench/
```

## 2. crate ごとの責務

| crate | 役割 |
|---|---|
| `kasane-core` | プロトコル、状態管理、レイアウト、抽象描画、プラグイン基盤 |
| `kasane-tui` | crossterm ベース TUI backend |
| `kasane-gui` | winit + wgpu + glyphon ベース GUI backend |
| `kasane-macros` | `#[kasane::plugin]`, `#[kasane::component]` などの proc macro |
| `kasane` | メインバイナリ、CLI、プロセス管理、backend 選択 |
| `kasane-wasm` | WASM プラグインランタイム、WIT ホストアダプタ |
| `kasane-plugin-sdk` | WASM guest 向け SDK |
| `kasane-wasm-bench` | WASM ベンチマークハーネス |

## 3. ソースツリーガイド

### 3.1 `kasane-core/src/`

```text
kasane-core/src/
├── lib.rs
├── element.rs
├── plugin.rs
├── input/
│   ├── mod.rs
│   └── builtin.rs
├── config.rs
├── io.rs
├── perf.rs
├── pane.rs
├── workspace.rs
├── plugin_prelude.rs
├── test_support.rs
├── surface/
│   ├── mod.rs
│   ├── buffer.rs
│   ├── menu.rs
│   ├── status.rs
│   └── info.rs
├── bin/
│   └── alloc_budget.rs
├── protocol/
│   ├── mod.rs
│   ├── color.rs
│   ├── message.rs
│   ├── parse.rs
│   └── tests.rs
├── test_utils.rs
├── state/
│   ├── mod.rs
│   ├── apply.rs
│   ├── update.rs
│   ├── info.rs
│   ├── menu.rs
│   └── tests.rs
├── layout/
│   ├── mod.rs
│   ├── flex.rs
│   ├── grid.rs
│   ├── position.rs
│   ├── info.rs
│   ├── hit_test.rs
│   ├── text.rs
│   └── word_wrap.rs
└── render/
    ├── mod.rs
    ├── grid.rs
    ├── paint.rs
    ├── patch.rs
    ├── cursor.rs
    ├── pipeline.rs
    ├── cache.rs
    ├── scene/
    │   ├── mod.rs
    │   └── cache.rs
    ├── theme.rs
    ├── markup.rs
    ├── test_helpers/
    │   ├── mod.rs
    │   └── info.rs
    ├── tests/
    │   ├── mod.rs
    │   ├── pipeline.rs
    │   ├── view_cache.rs
    │   ├── scene_cache.rs
    │   └── cursor.rs
    ├── menu.rs
    └── view/
        ├── mod.rs
        ├── info.rs
        ├── menu.rs
        └── tests.rs
```

主要責務:

| パス | 内容 |
|---|---|
| `element.rs` | 宣言的 UI の中核 `Element` 型 |
| `plugin.rs` | `Plugin` trait、registry、slot/decorator/replacement 合成 |
| `state/` | `AppState`、`apply()`、`update()`、dirty 生成 |
| `layout/` | measure/place、overlay 配置、hit test |
| `render/` | view 構築、paint、cache、pipeline、scene |
| `surface/` | surface 抽象と core surface 実装 |
| `workspace.rs` | surface 配置と分割構造 |
| `protocol/` | JSON-RPC パーサーと message 型 |
| `input/` | frontend 入力から Kakoune 入力への変換 |

### 3.2 `kasane-tui/src/`

```text
kasane-tui/src/
├── lib.rs
├── backend.rs
└── input.rs
```

| パス | 内容 |
|---|---|
| `backend.rs` | `RenderBackend` の TUI 実装 |
| `input.rs` | crossterm event 変換 |

### 3.3 `kasane-gui/src/`

```text
kasane-gui/src/
├── lib.rs
├── app.rs
├── backend.rs
├── input.rs
├── animation.rs
├── colors.rs
├── gpu/
│   ├── mod.rs
│   ├── cell_renderer.rs
│   ├── scene_renderer.rs
│   ├── metrics.rs
│   ├── bg_pipeline.rs
│   ├── border_pipeline.rs
│   ├── bg.wgsl
│   └── rounded_rect.wgsl
└── cpu/
    └── mod.rs
```

| パス | 内容 |
|---|---|
| `app.rs` | winit の application loop |
| `backend.rs` | GUI backend 実装 |
| `animation.rs` | smooth scroll などのアニメーション |
| `gpu/` | GPU レンダラ本体 |

### 3.4 `kasane-macros/src/`

```text
kasane-macros/src/
├── lib.rs
├── plugin.rs
├── component.rs
└── analysis.rs
```

| パス | 内容 |
|---|---|
| `plugin.rs` | `#[kasane_plugin]` の生成コード |
| `component.rs` | `#[kasane_component]`、deps、allow、検証 |
| `analysis.rs` | AST 解析共通コード |

### 3.5 `kasane/src/`

```text
kasane/src/
├── lib.rs
├── main.rs
├── cli.rs
└── process.rs
```

| パス | 内容 |
|---|---|
| `lib.rs` | `kasane::run()` |
| `main.rs` | デフォルトバイナリ |
| `cli.rs` | CLI 引数 |
| `process.rs` | Kakoune 子プロセス管理 |

### 3.6 `kasane-wasm/`

```text
kasane-wasm/
├── src/
│   ├── lib.rs
│   ├── adapter.rs
│   ├── host.rs
│   ├── convert.rs
│   └── tests.rs
├── wit/
│   └── plugin.wit
├── bundled/
│   ├── cursor-line.wasm
│   ├── color-preview.wasm
│   └── sel-badge.wasm
└── guests/
    ├── cursor-line/
    ├── color-preview/
    ├── sel-badge/
    └── line-numbers/
```

| パス | 内容 |
|---|---|
| `src/adapter.rs` | `Plugin` trait の WASM adapter |
| `src/host.rs` | guest -> host 呼び出し |
| `wit/plugin.wit` | WIT API 定義 |
| `guests/` | 参照実装プラグイン |
| `bundled/` | 同梱 WASM バイナリ |

### 3.7 補助 crate

| パス | 内容 |
|---|---|
| `kasane-plugin-sdk/src/lib.rs` | WIT bindings、定数、guest helper macro |
| `kasane-wasm-bench/src/lib.rs` | WASM bench harness |
| `kasane-wasm-bench/guests/` | benchmark guest plugins |

## 4. 変更箇所の目安

| やりたい変更 | 主に触る場所 |
|---|---|
| `AppState` や dirty の変更 | `kasane-core/src/state/` |
| plugin 合成や registry の変更 | `kasane-core/src/plugin.rs` |
| `Element` の追加や変更 | `kasane-core/src/element.rs` |
| layout アルゴリズムの変更 | `kasane-core/src/layout/` |
| TUI 描画パイプラインの変更 | `kasane-core/src/render/` と `kasane-tui/src/backend.rs` |
| GUI scene/pipeline の変更 | `kasane-core/src/render/scene/` と `kasane-gui/src/gpu/` |
| proc macro の deps 検証 | `kasane-macros/src/component.rs` と `analysis.rs` |
| plugin WIT / host API の変更 | `kasane-wasm/wit/plugin.wit`, `kasane-wasm/src/host.rs`, `kasane-plugin-sdk/src/lib.rs` |
| CLI や起動経路の変更 | `kasane/src/cli.rs`, `kasane/src/process.rs`, `kasane/src/lib.rs` |
| bundle plugin の変更 | `kasane-wasm/guests/` |

## 5. 関連文書

- [architecture.md](./architecture.md): システム境界とランタイム構成
- [semantics.md](./semantics.md): 状態、描画、invalidation、正しさ条件
- [plugin-api.md](./plugin-api.md): plugin author 向け API リファレンス
