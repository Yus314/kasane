# Kasane プラグイン開発者のための宣言的 UI ガイド

本ドキュメントは、Kasane プラグインを書き始めるための最短ガイドである。
API の詳細は [plugin-api.md](./plugin-api.md)、合成順序や正しさ条件は [semantics.md](./semantics.md) を参照。

## 1. はじめに

### 1.1 対象読者と開発パス

Kasane プラグインには 2 つの開発パスがある。

| | WASM (推奨) | ネイティブ |
|---|---|---|
| 安全性 | サンドボックス内で実行 | ホストプロセスと同一空間 |
| 配布 | `.wasm` ファイルを `plugins/` に配置 | カスタムバイナリとして配布 |
| API | WIT 経由 (`host-state` + `element-builder`) | `&AppState` 直接参照 |
| 依存 | `kasane-plugin-sdk` + `wit-bindgen` | `kasane` + `kasane-core` |

初めてのプラグインには WASM パスを推奨する。ネイティブパスは `&AppState` への完全アクセスが必要な場合や、まだ WASM parity がない escape hatch を使う場合に向いている。ネイティブでは `Plugin` trait の直接実装と proc macro 補助の両方を使える。

### 1.2 このガイドの読み方

1. まず `## 2. クイックスタート` の WASM 例をそのまま動かす
2. 次に [plugin-api.md](./plugin-api.md) で使いたい extension point を引く
3. `transform()` / `stable()` / cache の意味を変える場合だけ [semantics.md](./semantics.md) を読む

> 補足: Kasane は将来的に `display transformation` と `display unit` を第一級に扱う方向を取るが、現時点では専用 API は未完成である。現在の shared API は `contribute_to()`、`annotate_line_with_ctx()`、`contribute_overlay_with_ctx()`、`transform()` の組み合わせで段階的に検証する。`Surface` や `PaintHook` は native escape hatch であり、長期的には WASM parity を目指して再設計する。

### 1.3 設計思想

- プラグインは「何を表示したいか」を記述し、「どう描画するか」はフレームワークが決める
- 拡張は `contribute_to()`、`annotate_line_with_ctx()`、`transform()` など段階的な自由度を持つ
- 表示の大胆な再構成は将来方向として許容されるが、protocol truth の捏造は許されない
- Kasane は Kakoune 専用の UI 基盤であり、汎用 UI フレームワーク化は目標外である

### 1.4 プラグインで実現できること

各メカニズムで実現可能なプラグインの例を以下に示す。

| メカニズム | 実現可能な例 |
|---|---|
| `contribute_to()` | 行番号、選択カーソル数バッジ、Git diff マーカー、ブレッドクラム |
| `annotate_line_with_ctx()` | カーソル行ハイライト、インデントガイド、変更行マーカー |
| `contribute_overlay_with_ctx()` | カラーピッカー、ツールチップ、診断ポップアップ |
| `transform()` | ステータスバーカスタマイズ、メニューレイアウト変更 |
| `handle_key()` + `handle_mouse()` | インタラクティブ UI（ピッカー、ダイアログ） |
| `Surface` (現状ネイティブ) | サイドバー、ファイルツリー、専用パネル |

ファイルシステムアクセスは WASI ケイパビリティ宣言 (`Capability::Filesystem`) で利用可能。外部プロセス実行（ファジーファインダー等）は Phase P-2 で対応予定。詳細は [plugin-api.md §0](./plugin-api.md#0-プラグイン-api-のスコープ) を参照。

## 2. クイックスタート

### 2.1 WASM プラグイン (推奨)

以下は選択カーソル数をステータスバー右側に表示する `sel-badge` プラグインの全文である。

```rust
// kasane-wasm/guests/sel-badge/src/lib.rs
kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");

use std::cell::Cell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;
use kasane_plugin_sdk::{dirty, slot};

thread_local! {
    static CURSOR_COUNT: Cell<u32> = const { Cell::new(0) };
}

struct SelBadgePlugin;

impl Guest for SelBadgePlugin {
    fn get_id() -> String {
        "sel_badge".to_string()
    }

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::BUFFER != 0 {
            CURSOR_COUNT.set(host_state::get_cursor_count());
        }
        vec![]
    }

    fn contribute_to(region: u8, _ctx: ContributeContext) -> Option<Contribution> {
        kasane_plugin_sdk::route_slots!(region, {
            slot::STATUS_RIGHT => {
                let count = CURSOR_COUNT.get();
                if count > 1 {
                    let text = format!(" {} sel ", count);
                    let face = Face {
                        fg: Color::DefaultColor,
                        bg: Color::DefaultColor,
                        underline: Color::DefaultColor,
                        attributes: 0,
                    };
                    let el = element_builder::create_text(&text, face);
                    Some(Contribution {
                        element: el,
                        priority: 0,
                        size_hint: ContribSizeHint::Auto,
                    })
                } else {
                    None
                }
            },
        })
    }

    fn contribute_deps(region: u8) -> u16 {
        kasane_plugin_sdk::route_slot_deps!(region, {
            slot::STATUS_RIGHT => dirty::BUFFER,
        })
    }

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
    }

    // Old WIT API stubs (required by WIT interface, not called by host)
    kasane_plugin_sdk::default_contribute!();
    kasane_plugin_sdk::default_line!();
    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_named_slot!();

    // New API defaults
    kasane_plugin_sdk::default_init!();
    kasane_plugin_sdk::default_shutdown!();
    kasane_plugin_sdk::default_input!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_update!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_annotate!();
    kasane_plugin_sdk::default_overlay_v2!();
    kasane_plugin_sdk::default_transform_deps!();
    kasane_plugin_sdk::default_annotate_deps!();
    kasane_plugin_sdk::default_capabilities!();
}

export!(SelBadgePlugin);
```

**プロジェクトセットアップ:**

```toml
# Cargo.toml
[package]
name = "sel-badge"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
kasane-plugin-sdk = { path = "../../kasane-plugin-sdk" }
wit-bindgen = "0.41"
```

**ビルド・配置:**

```bash
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/sel_badge.wasm ~/.local/share/kasane/plugins/
```

### 2.2 ネイティブプラグイン

```rust
// examples/line-numbers/src/main.rs
use kasane::kasane_core::plugin_prelude::*;

struct LineNumbersPlugin;

impl Plugin for LineNumbersPlugin {
    fn id(&self) -> PluginId {
        PluginId("line_numbers".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppState,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region != &SlotId::BUFFER_LEFT {
            return None;
        }

        let total = state.lines.len();
        let width = total.to_string().len().max(2);

        let children: Vec<_> = (0..total)
            .map(|i| {
                let num = format!("{:>w$} ", i + 1, w = width);
                FlexChild::fixed(Element::text(
                    num,
                    Face {
                        fg: Color::Named(NamedColor::Cyan),
                        ..Face::default()
                    },
                ))
            })
            .collect();

        Some(Contribution {
            element: Element::column(children),
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }

    fn contribute_deps(&self, _region: &SlotId) -> DirtyFlags {
        DirtyFlags::BUFFER_CONTENT
    }
}

fn main() {
    kasane::run(|registry| {
        registry.register(Box::new(LineNumbersPlugin));
    });
}
```

```toml
# Cargo.toml
[dependencies]
kasane = { path = "../kasane" }
kasane-core = { path = "../kasane-core" }
```

`Plugin` trait を直接実装し、`kasane::run()` でプラグインを登録してカスタムバイナリとして配布する。`PluginCapabilities` で使用する機能を明示する。`#[kasane_plugin]` macro は使える hook では便利だが、現時点では hook parity が完全ではないため、一部機能では直接実装が必要になる。

## 3. 次に読む文書

| 目的 | 読む文書 |
|---|---|
| `contribute_to`、`transform`、`annotate_line_with_ctx`、`contribute_overlay_with_ctx` の違いを知りたい | [plugin-api.md](./plugin-api.md) |
| `display transformation` / `display unit` の将来方向を知りたい | [plugin-api.md](./plugin-api.md), [semantics.md](./semantics.md) |
| `Element` の作り方を調べたい | [plugin-api.md](./plugin-api.md) |
| `host-state`、入力、`Command` を確認したい | [plugin-api.md](./plugin-api.md) |
| `state_hash()`、`contribute_deps()`、`PaintHook` を使いたい | [plugin-api.md](./plugin-api.md) |
| `Surface`、`Workspace`、カスタム slot を使いたい | [plugin-api.md](./plugin-api.md) |
| 合成順序、`stable()`、観測等価性を確認したい | [semantics.md](./semantics.md) |
| 性能の支配コストや計測結果を知りたい | [performance.md](./performance.md) |

## 4. 登録と配布

### 4.1 登録順序

Kasane は次の順序でプラグインを登録する。

1. バンドル WASM
2. FS 発見 WASM (`~/.local/share/kasane/plugins/*.wasm`)
3. `kasane::run(|registry| { ... })` で登録されるネイティブプラグイン

同じ ID の FS 発見 WASM はバンドルプラグインを上書きできる。

### 4.2 配布方法

- WASM: `.wasm` ファイルを `~/.local/share/kasane/plugins/` に配置
- ネイティブ: `kasane::run()` を使うカスタムバイナリとして配布

### 4.3 config.toml での制御

```toml
[plugins]
disabled = ["color_preview"]

# プラグインごとの WASI ケイパビリティ拒否
[plugins.deny_capabilities]
untrusted_plugin = ["filesystem", "environment"]
```

### 4.4 WASI ケイパビリティ

WASM プラグインは `requested_capabilities()` で必要な WASI ケイパビリティを宣言できる。
ホストは宣言に基づき、プラグインごとに WASI コンテキストを構成する。

利用可能なケイパビリティ:

| ケイパビリティ | 効果 | デフォルト |
|---|---|---|
| `Capability::Filesystem` | `data/` (プラグイン専用データディレクトリ, read/write) と `.` (CWD, read-only) を preopen | 無効 |
| `Capability::Environment` | ホストの環境変数を継承 | 無効 |
| `Capability::MonotonicClock` | 単調時計へのアクセス (デフォルトで有効だが、宣言により監査可能) | 有効 |

```rust
// ファイルシステムアクセスが必要なプラグインの例
fn requested_capabilities() -> Vec<Capability> {
    vec![Capability::Filesystem]
}
```

ケイパビリティは宣言即承認される。ユーザーは `config.toml` の `deny_capabilities` で拒否できる。

制約: WASI ケイパビリティは `on_init()` 以降で利用可能。コンポーネント初期化 (`_initialize`) 中は利用できない。

### 4.5 テスト

`PluginRegistry` を直接使ってユニットテストが書ける。

```rust
#[test]
fn my_plugin_contributes_gutter() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(MyPlugin));

    let state = AppState::default();
    let _ = registry.init_all(&state);

    let contributions = registry.collect_contributions(&SlotId::BUFFER_LEFT, &state);
    assert_eq!(contributions.len(), 1);
}
```

## 5. 参照実装一覧

| プラグイン | パス | 行数 | 主な機能 |
|---|---|---|---|
| cursor-line (WASM) | `kasane-wasm/guests/cursor-line/` | 73行 | `annotate_line_with_ctx()`, `state_hash()` |
| sel-badge (WASM) | `kasane-wasm/guests/sel-badge/` | 111行 | `contribute_to()` (`STATUS_RIGHT`) |
| line-numbers (WASM) | `kasane-wasm/guests/line-numbers/` | 92行 | `contribute_to()` (`BUFFER_LEFT`) |
| color-preview (WASM) | `kasane-wasm/guests/color-preview/` | 641行 | `annotate_line_with_ctx()`, `contribute_overlay_with_ctx()`, `handle_mouse()` |
| line-numbers (ネイティブ) | `examples/line-numbers/` | 57行 | `Plugin` trait 直接実装, `contribute_to()`, `kasane::run()` |

## 6. 付録: WASM vs ネイティブ比較表

| 観点 | WASM | ネイティブ |
|---|---|---|
| 安全性 | サンドボックス分離、ホストクラッシュ防止 | ホストと同一プロセス |
| パフォーマンス | WASM 境界越えコストあり | 直接関数呼び出し |
| API アクセス | `host-state` + `element-builder` | `&AppState` 直接参照 |
| 配布 | `.wasm` ファイル配置 | カスタムバイナリ |
| 開発体験 | SDK マクロ + `wit-bindgen` | `#[kasane::plugin]` マクロ |
| `Surface` / `PaintHook` | 未対応 | 対応 |
| プラグイン間通信 | `Vec<u8>` | `Box<dyn Any>` |

## 7. 関連文書

- [plugin-api.md](./plugin-api.md) — API の詳細
- [semantics.md](./semantics.md) — 合成順序と正しさ条件
- [repo-layout.md](./repo-layout.md) — コードの場所
- [index.md](./index.md) — docs 全体の入口
