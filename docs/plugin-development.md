# Kasane プラグイン開発者のための宣言的 UI ガイド

本ドキュメントは Kasane プラグインの開発に必要な情報を網羅する。
設計思想や最適化の根拠は [decisions.md](./decisions.md)、パフォーマンス測定は [performance.md](./performance.md)、レイアウト/描画の実装詳細は [architecture.md](./architecture.md) を参照。

## 1. はじめに

### 対象読者と開発パス

Kasane プラグインには2つの開発パスがある:

| | WASM (推奨) | ネイティブ |
|---|---|---|
| 安全性 | サンドボックス内で実行 | ホストプロセスと同一空間 |
| 配布 | `.wasm` ファイルを `plugins/` に配置 | カスタムバイナリとして配布 |
| API | WIT 経由 (host-state + element-builder) | `&AppState` 直接参照 |
| 依存 | `kasane-plugin-sdk` + `wit-bindgen` | `kasane` + `kasane-core` |

**初めてのプラグインには WASM パスを推奨する。** ネイティブパスは `&AppState` への完全なアクセスが必要な場合や、ホストプロセスに統合する必要がある場合に使用する。

### 設計思想

- **宣言的**: プラグインは「何を表示したいか」を記述し、「どう描画するか」はフレームワークが決定する
- **拡張性**: Slot / Decorator / Replacement の三段階でプラグインが UI を拡張できる
- **設定可能性**: テーマ・レイアウト・キーバインドをユーザーが設定で変更できる
- **Kakoune 専用**: Kakoune の JSON UI プロトコルに特化し、不要な抽象化を避ける

## 2. 概念モデル

### 全体の流れ

```
Kakoune (kak -ui json)
  → JSON-RPC parse
  → AppState.apply()           # プロトコル → 状態
  → プラグイン通知               # on_state_changed(dirty)
  → view(&state, &registry)    # 状態 → Element ツリー (プラグイン寄与を合成)
  → place(&element, rect)      # Element → レイアウト
  → paint(&element, &layout)   # レイアウト → CellGrid
  → grid.diff()                # 差分検出
  → backend.draw()             # 端末 (TUI) or GPU (GUI) 出力
```

### Surface: 画面領域の抽象

Surface はスクリーン上の矩形領域を所有し、自身の Element ツリーの構築とイベント処理を担う。コア画面要素 (バッファ、ステータスバー、メニュー、Info) は全て Surface として実装されている。

| SurfaceId | Surface | 説明 |
|---|---|---|
| `BUFFER` (0) | KakouneBufferSurface | メインのバッファ表示 (常駐) |
| `STATUS` (1) | StatusBarSurface | ステータスバー (常駐) |
| `MENU` (2) | MenuSurface | メニュー (一時的、menu_show/hide で出現/消滅) |
| `INFO_BASE`+ (10+) | InfoSurface | Info ポップアップ (一時的) |
| `PLUGIN_BASE`+ (100+) | プラグイン定義 | プラグインが `SURFACE_PROVIDER` で登録 |

### SlotId: Surface が宣言する拡張点

各 Surface は SlotDeclaration で拡張点を宣言する。プラグインはこれらの SlotId に Element を寄与する。

| SlotId | 位置 | 宣言元 Surface |
|---|---|---|
| `kasane.buffer.left` | バッファの左 (ガター) | KakouneBufferSurface |
| `kasane.buffer.right` | バッファの右 | KakouneBufferSurface |
| `kasane.buffer.above` | バッファの上 | KakouneBufferSurface |
| `kasane.buffer.below` | バッファの下 | KakouneBufferSurface |
| `kasane.buffer.overlay` | バッファ上のオーバーレイ | KakouneBufferSurface |
| `kasane.status.above` | ステータスバーの上 | StatusBarSurface |
| `kasane.status.left` | ステータスバー左側 | StatusBarSurface |
| `kasane.status.right` | ステータスバー右側 | StatusBarSurface |

カスタム SlotId も `SlotId::new("myplugin.sidebar")` で定義できる (§9.3 参照)。

### Element: 宣言的 UI 型

プラグインが UI を記述するために使う不変のデータ構造。`Element` 型は `Text`、`StyledLine`、`Flex`、`Stack`、`Grid`、`Container`、`Scrollable`、`Interactive`、`Empty`、`BufferRef` のバリアントを持つ。

### フレームワーク vs プラグインの責務分離

| 責務 | フレームワーク | プラグイン |
|------|-------------|----------|
| Element ツリーの合成 | Slot 収集、Decorator/Replacement 適用 | contribute(), decorate(), replace() を実装 |
| レイアウト計算 | measure() + place() を実行 | 関与しない (フレームワークに委任) |
| CellGrid への描画 | paint() を実行 | 関与しない (Element で宣言するのみ) |
| イベントルーティング | キーディスパッチ、InteractiveId ヒットテスト | handle_key(), handle_mouse() を実装 |
| 差分描画・端末出力 | grid.diff() + backend.draw() | 関与しない |

## 3. クイックスタート

### 3.1 WASM プラグイン (推奨)

以下は選択カーソル数をステータスバー右側に表示する `sel-badge` プラグインの全文:

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

    fn contribute(s: u8) -> Option<ElementHandle> {
        kasane_plugin_sdk::route_slots!(s, {
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
                    Some(element_builder::create_text(&text, face))
                } else {
                    None
                }
            },
        })
    }

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
    }

    fn slot_deps(s: u8) -> u16 {
        kasane_plugin_sdk::route_slot_deps!(s, {
            slot::STATUS_RIGHT => dirty::BUFFER,
        })
    }

    kasane_plugin_sdk::default_init!();
    kasane_plugin_sdk::default_shutdown!();
    kasane_plugin_sdk::default_line!();
    kasane_plugin_sdk::default_input!();
    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_update!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_named_slot!();
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

### 3.2 ネイティブプラグイン

```rust
// examples/line-numbers/src/main.rs
use kasane::kasane_core::plugin_prelude::*;

#[kasane_plugin]
mod line_numbers {
    use kasane::kasane_core::plugin_prelude::*;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[slot(Slot::BufferLeft)]
    pub fn gutter(_state: &State, core: &AppState) -> Option<Element> {
        let total = core.lines.len();
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

        Some(Element::column(children))
    }
}

fn main() {
    kasane::run(|registry| {
        registry.register(Box::new(LineNumbersPlugin::new()));
    });
}
```

**セットアップ:**

```toml
# Cargo.toml
[dependencies]
kasane = { path = "../kasane" }       # or git/registry
kasane-core = { path = "../kasane-core" }
```

`#[kasane_plugin]` マクロはモジュール名を PascalCase + `Plugin` に変換する (`line_numbers` → `LineNumbersPlugin`)。`kasane::run()` でプラグインを登録し、カスタムバイナリとして配布する。

## 4. 拡張モデル

### 4.1 判断フロー

```
やりたいこと                              → 使うメカニズム
───────────────────────────────────────────────────────
定義済みの場所に UI を追加したい           → Slot
  例: 行番号、スクロールバー、タブバー

バッファの各行を装飾したい                 → LineDecoration
  例: カーソル行ハイライト、git 差分マーク

フローティング UI を表示したい             → Overlay
  例: カラーピッカー、ツールチップ

既存 UI の見た目を変更したい               → Decorator
  例: ボーダー追加、背景色変更、テーマ適用

既存 UI を根本的に別の UI にしたい         → Replacement
  例: fzf 風メニュー、カスタムステータスバー

Element ツリーを経由せず直接描画したい     → PaintHook
  例: カスタムハイライト、ビジュアルインジケータ
```

**原則: 自由度が低いメカニズムを優先する。** Slot で済むなら Decorator は使わない。Decorator で済むなら Replacement は使わない。

#### 合成ルール

Slot、Decorator、Replacement は以下の順序で適用される:

```
1. Replacement の確認
   → ターゲットに Replacement が登録されている？
     Yes → Replacement の Element を使用（デフォルト Element は構築しない）
     No  → デフォルト Element を構築

2. Decorator の適用
   → Replacement/デフォルトの Element に対して、priority 順に Decorator を適用

3. Slot の収集
   → 各 Slot に登録された Element を収集し、レイアウトに配置
```

**重要: Replacement が存在するターゲットに対しても Decorator は適用される。** Replacement はコンテンツを差し替え、Decorator はスタイリング（ボーダー、シャドウ等）を担当する。この分離により、テーマプラグイン (Decorator) とカスタムメニュープラグイン (Replacement) が自然に共存できる。

### 4.2 Slot (挿入点)

SlotId 一覧は §2「SlotId: Surface が宣言する拡張点」を参照。

**WASM:**

```rust
fn contribute(s: u8) -> Option<ElementHandle> {
    kasane_plugin_sdk::route_slots!(s, {
        slot::BUFFER_LEFT => {
            Some(element_builder::create_text("★", face))
        },
        slot::STATUS_RIGHT => {
            Some(element_builder::create_text("info", face))
        },
    })
}
```

`slot::BUFFER_LEFT` (=0)〜`slot::OVERLAY` (=7) の定数は `kasane_plugin_sdk::slot` モジュールで定義されている。

**ネイティブ:**

```rust
#[slot(Slot::BufferLeft)]
pub fn gutter(_state: &State, core: &AppState) -> Option<Element> {
    Some(Element::text("★", Face::default()))
}
```

> **カスタム SlotId:** WASM では `contribute_named(slot_name)` を実装し、ネイティブでは `contribute_named_slot(name, state)` をオーバーライドする。カスタム SlotId の定義方法は §9.3 を参照。

### 4.3 LineDecoration (行装飾)

バッファの各行に対してガターアイコンや行背景を提供する。

**WASM (cursor-line):**

```rust
fn contribute_line(line: u32) -> Option<LineDecoration> {
    let active = ACTIVE_LINE.get();
    if line as i32 == active {
        Some(LineDecoration {
            left_gutter: None,
            right_gutter: None,
            background: Some(Face {
                fg: Color::DefaultColor,
                bg: Color::Rgb(RgbColor { r: 40, g: 40, b: 50 }),
                underline: Color::DefaultColor,
                attributes: 0,
            }),
        })
    } else {
        None
    }
}
```

**ネイティブ:**

```rust
pub fn contribute_line(state: &State, line: usize, _core: &AppState) -> Option<LineDecoration> {
    if line == state.active_line {
        Some(LineDecoration {
            left_gutter: Some(Element::text("→", Face::default())),
            right_gutter: None,
            background: Some(Face { bg: Color::Rgb { r: 40, g: 40, b: 50 }, ..Face::default() }),
        })
    } else {
        None
    }
}
```

`LineDecoration` は `left_gutter` (左ガター Element)、`right_gutter` (右ガター Element)、`background` (行背景 Face) の3フィールドからなる。複数プラグインのガター寄与は水平に合成される。

### 4.4 Overlay (フローティング UI)

`contribute_overlay()` でオーバーレイ要素を提供する。

```rust
// WASM
fn contribute_overlay() -> Option<Overlay> {
    Some(Overlay {
        element: element_builder::create_container_styled(child, ...),
        anchor: OverlayAnchor::Absolute(AbsoluteAnchor { x: 10, y: 5, w: 30, h: 10 }),
    })
}

// ネイティブ
fn contribute_overlay(&self, _state: &AppState) -> Option<Overlay> {
    Some(Overlay {
        element: Element::container(child, style),
        anchor: OverlayAnchor::AnchorPoint { coord, prefer_above: true, avoid: vec![] },
    })
}
```

`OverlayAnchor` は2種類:
- `Absolute { x, y, w, h }` — 画面座標に対する絶対位置
- `AnchorPoint { coord, prefer_above, avoid }` — Kakoune 互換のアンカーベース配置 (画面端クランプ、フリップ、衝突回避)

### 4.5 Decorator (ラッパー)

既存の Element を受け取り、変換して返す。

**WASM:**

```rust
fn decorate(target: DecorateTarget, element: ElementHandle) -> ElementHandle {
    // element は handle 0 として arena に注入済み
    element_builder::create_container(element, Some(BorderLineStyle::Single), false, edges)
}
fn decorator_priority() -> u32 { 100 }
```

**ネイティブ:**

```rust
#[decorate(DecorateTarget::Buffer, priority = 100)]
pub fn decorate(_state: &State, element: Element, _core: &AppState) -> Element {
    Element::container(element, Style::from(Face::default()))
}
```

対象: `DecorateTarget::Buffer`、`StatusBar`、`Menu`、`Info`、`BufferLine(n)`

**ガイドライン:** Decorator は受け取った Element の内部構造を仮定してはならない。「Element をそのままラップする」パターン（Container で包む等）は安全だが、「Element を分解して内部を変更する」パターンは Replacement との組み合わせで壊れる可能性がある。

### 4.6 Replacement (差替)

既存コンポーネントを完全に差し替える。プロトコル不整合のリスクが低い対象に限定される。

**ネイティブ:**

```rust
#[replace(ReplaceTarget::MenuPrompt)]
pub fn replace(_state: &State, _core: &AppState) -> Option<Element> {
    Some(Element::text("custom menu", Face::default()))
}
```

**対象:**

| ReplaceTarget | 説明 |
|---|---|
| `MenuPrompt` | プロンプトメニュー |
| `MenuInline` | インラインメニュー |
| `MenuSearch` | 検索メニュー |
| `InfoPrompt` | プロンプト Info |
| `InfoModal` | モーダル Info |
| `StatusBar` | ステータスバー全体 |

**Replacement の責務境界:** Replacement が差し替えるのは**ビュー（Element の構築）のみ**。プロトコル処理 (menu_show → AppState.menu = Some(...) 等) は常に基盤が管理する。

| レベル | 内容 | 例 |
|--------|------|---|
| ビューのみ | 見た目だけ変える | 角丸メニュー、縦型レイアウト |
| ビュー + 状態 | プラグイン固有の状態を持つ | フィルタリングメニュー |
| ビュー + 状態 + 入力処理 | キー入力も処理 | fzf 風ファジーファインダー |

レベル 2・3 では `handle_key()` がキー入力を処理し、`update()` が状態を管理する。

## 5. Element ツリー

### 5.1 Element 型一覧

| 型 | 用途 | WASM builder | ネイティブ |
|---|---|---|---|
| `Text` | テキスト + スタイル | `create_text(content, face)` | `Element::text(s, face)` |
| `StyledLine` | Atom 列 (Kakoune スタイル付き) | `create_styled_line(atoms)` | `Element::styled_line(line)` |
| `Flex` (Column) | 垂直配置 | `create_column(children)` / `create_column_flex(entries, gap)` | `Element::column(children)` |
| `Flex` (Row) | 水平配置 | `create_row(children)` / `create_row_flex(entries, gap)` | `Element::row(children)` |
| `Grid` | 2D テーブル | `create_grid(cols, children, col_gap, row_gap)` | `Element::grid(columns, children)` |
| `Container` | 装飾 (border, shadow, padding) | `create_container(child, border, shadow, padding)` / `create_container_styled(...)` / `create_container_custom_border(...)` | `Element::container(child, style)` |
| `Stack` | Z 軸重ね (base + overlays) | `create_stack(base, overlays)` | `Element::stack(base, overlays)` |
| `Scrollable` | スクロール可能領域 | `create_scrollable(child, offset, vertical)` | `Element::Scrollable { ... }` |
| `Interactive` | マウスヒットテスト | `create_interactive(child, id)` | `Element::Interactive { child, id }` |
| `Empty` | 空要素 | `create_empty()` | `Element::Empty` |
| `BufferRef` | バッファ行の zero-copy 参照 | (ホスト内部のみ) | `Element::buffer_ref(range)` |

### 5.2 WASM: element-builder API

全関数は `element_builder` モジュールからインポートされる。`ElementHandle` (u32) を返し、そのハンドルは現在のプラグイン呼び出しスコープ内でのみ有効。

```rust
use kasane::plugin::element_builder;

let text = element_builder::create_text("hello", face);
let col = element_builder::create_column(&[text]);
let container = element_builder::create_container(col, Some(BorderLineStyle::Single), false,
    Edges { top: 0, right: 1, bottom: 0, left: 1 });
```

Flex レイアウトで比例配分する場合は `create_column_flex` / `create_row_flex` を使い、`FlexEntry { child, flex }` で flex 比率を指定する。

### 5.3 ネイティブ: Element 直接構築

```rust
use kasane_core::plugin_prelude::*;

let text = Element::text("hello", Face::default());
let col = Element::column(vec![
    FlexChild::fixed(text),
    FlexChild::flexible(Element::Empty, 1.0),  // 残り空間を埋める
]);
```

`FlexChild::fixed(element)` は flex=0.0 (固定サイズ)、`FlexChild::flexible(element, factor)` は比例配分。

## 6. 状態アクセスとイベント

### 6.1 AppState 概要

**ネイティブ:** `&AppState` を直接参照できる。主要フィールド:

| フィールド | 型 | 説明 |
|---|---|---|
| `lines` | `Vec<Line>` | バッファ行 |
| `cursor_pos` | `Coord` | カーソル位置 |
| `status_line` | `Line` | ステータスバー |
| `menu` | `Option<MenuState>` | メニュー状態 |
| `infos` | `Vec<InfoState>` | Info ポップアップ |
| `cols`, `rows` | `u16` | 端末サイズ |
| `focused` | `bool` | 端末フォーカス |

状態変更は `DirtyFlags` で通知される:

| フラグ | ビット | 説明 |
|---|---|---|
| `BUFFER` | `1 << 0` | バッファ行・カーソル |
| `STATUS` | `1 << 1` | ステータスバー |
| `MENU_STRUCTURE` | `1 << 2` | メニュー構造 |
| `MENU_SELECTION` | `1 << 3` | メニュー選択 |
| `INFO` | `1 << 4` | Info ポップアップ |
| `OPTIONS` | `1 << 5` | UI オプション |

**WASM:** host-state API 経由でアクセスする (§6.2 参照)。

### 6.2 WASM host-state API

`kasane::plugin::host_state` モジュールの関数一覧:

**基本状態 (Tier 0):**

| 関数 | 戻り値 |
|---|---|
| `get_cursor_line()` | `s32` |
| `get_cursor_col()` | `s32` |
| `get_line_count()` | `u32` |
| `get_cols()` | `u16` |
| `get_rows()` | `u16` |
| `is_focused()` | `bool` |

**バッファ行 (Tier 0.5):**

| 関数 | 戻り値 |
|---|---|
| `get_line_text(line)` | `Option<String>` |
| `is_line_dirty(line)` | `bool` |

**ステータスバー (Tier 1):**

| 関数 | 戻り値 |
|---|---|
| `get_status_prompt()` | `Vec<Atom>` |
| `get_status_content()` | `Vec<Atom>` |
| `get_status_line()` | `Vec<Atom>` |
| `get_status_mode_line()` | `Vec<Atom>` |
| `get_status_default_face()` | `Face` |

**メニュー/Info 状態 (Tier 2):**

| 関数 | 戻り値 |
|---|---|
| `has_menu()` | `bool` |
| `get_menu_item_count()` | `u32` |
| `get_menu_item(index)` | `Option<Vec<Atom>>` |
| `get_menu_selected()` | `s32` |
| `has_info()` | `bool` |
| `get_info_count()` | `u32` |

**一般状態 (Tier 3):**

| 関数 | 戻り値 |
|---|---|
| `get_ui_option(key)` | `Option<String>` |
| `get_cursor_mode()` | `u8` |
| `get_widget_columns()` | `u16` |
| `get_default_face()` | `Face` |
| `get_padding_face()` | `Face` |

**マルチカーソル (Tier 4):**

| 関数 | 戻り値 |
|---|---|
| `get_cursor_count()` | `u32` |
| `get_secondary_cursor_count()` | `u32` |
| `get_secondary_cursor(index)` | `Option<Coord>` |

**設定 (Tier 5):**

| 関数 | 戻り値 |
|---|---|
| `get_config_string(key)` | `Option<String>` |

**Info 詳細 (Tier 6):**

| 関数 | 戻り値 |
|---|---|
| `get_info_title(index)` | `Option<Vec<Atom>>` |
| `get_info_content(index)` | `Option<Vec<Vec<Atom>>>` |
| `get_info_style(index)` | `Option<String>` |
| `get_info_anchor(index)` | `Option<Coord>` |

**メニュー詳細 (Tier 7):**

| 関数 | 戻り値 |
|---|---|
| `get_menu_anchor()` | `Option<Coord>` |
| `get_menu_style()` | `Option<String>` |
| `get_menu_face()` | `Option<Face>` |
| `get_menu_selected_face()` | `Option<Face>` |

### 6.3 ライフサイクルフック

| フック | タイミング | 用途 |
|---|---|---|
| `on_init` | PluginRegistry 登録直後 | 初期化、テーマトークン登録 |
| `on_shutdown` | アプリケーション終了時 | クリーンアップ |
| `on_state_changed(dirty)` | AppState 更新後 | プラグイン内部状態の同期 |

### 6.4 入力処理

```
キー入力
  │
  ▼
observe_key() を全プラグインに通知 (消費不可、内部状態の追跡用)
  │
  ▼
全プラグインの handle_key() を順に呼ぶ (first-wins)
  │  各プラグインは state を見て自己判断:
  │    - state.menu.is_some() なら Menu Replacement が処理
  │    - 自分の Overlay が active なら処理
  │    - それ以外は None を返す
  │
  ▼ 最初に Some(commands) を返したプラグインが勝つ
  │ (全て None の場合)
  ▼
組み込みキーバインド (PageUp/PageDown)
  │ (不一致の場合)
  ▼
デフォルト: Kakoune に転送
```

マウスイベントも同様に observe_mouse() → InteractiveId ヒットテスト → handle_mouse() の順で処理される。

`Element::Interactive { child, id }` で Interactive 領域を定義し、`handle_mouse(event, id, state)` で処理する。

### 6.5 コマンド

プラグインのフック関数は `Vec<Command>` を返すことで副作用を発行する:

| Command | 説明 |
|---|---|
| `SendToKakoune(req)` | Kakoune にリクエストを送信 |
| `Paste` | クリップボード貼り付け |
| `Quit` | アプリケーション終了 |
| `RequestRedraw(flags)` | 再描画を要求 (DirtyFlags で範囲指定) |
| `ScheduleTimer { delay, target, payload }` | タイマー発火後に target プラグインにメッセージ送信 |
| `PluginMessage { target, payload }` | 他のプラグインにメッセージ送信 |
| `SetConfig { key, value }` | ランタイム設定変更 |
| `Pane(PaneCommand)` | Pane 操作 |
| `Workspace(WorkspaceCommand)` | Workspace レイアウト操作 |
| `RegisterThemeTokens(tokens)` | カスタムテーマトークンを登録 |

> **WASM:** WASM コマンドは `command` variant で表現される (`send-keys`, `paste`, `quit`, `request-redraw`, `set-config`, `schedule-timer`, `plugin-message`)。Pane/Workspace/RegisterThemeTokens は現在 WASM 未対応。

## 7. パフォーマンス最適化

### 7.1 PluginCapabilities

`PluginCapabilities` は 14 個のフラグで構成されるビットフラグ。プラグインが `capabilities()` を正しく宣言すると、不要なメソッド呼び出し (特に WASM 境界越え) がスキップされる。

| フラグ | 説明 |
|---|---|
| `SLOT_CONTRIBUTOR` | contribute() / contribute_slot() |
| `LINE_DECORATION` | contribute_line() |
| `OVERLAY` | contribute_overlay() |
| `DECORATOR` | decorate() |
| `REPLACEMENT` | replace() |
| `MENU_TRANSFORM` | transform_menu_item() |
| `CURSOR_STYLE` | cursor_style_override() |
| `INPUT_HANDLER` | handle_key() / handle_mouse() |
| `NAMED_SLOT` | contribute_named_slot() |
| `PANE_LIFECYCLE` | on_pane_created/closed, on_focus_changed |
| `PANE_RENDERER` | render_pane() |
| `SURFACE_PROVIDER` | surfaces() |
| `WORKSPACE_OBSERVER` | on_workspace_changed() |
| `PAINT_HOOK` | paint_hooks() |

ネイティブプラグインのデフォルトは `all()` (全フラグ)。WASM アダプタは WIT 呼び出し結果に基づいて自動設定する。

### 7.2 ビューキャッシュ (L1/L3)

プラグインの contribute() 結果はキャッシュされる:

- **L1 (state_hash):** プラグイン内部状態のハッシュ。前回と同一なら contribute() をスキップ
- **L3 (slot_deps / slot_id_deps):** 指定 Slot が依存する DirtyFlags。該当フラグが立っていなければスキップ

**WASM:**

```rust
fn state_hash() -> u64 {
    MY_STATE.get() as u64
}

fn slot_deps(s: u8) -> u16 {
    kasane_plugin_sdk::route_slot_deps!(s, {
        slot::BUFFER_LEFT => dirty::BUFFER,
        slot::STATUS_RIGHT => dirty::STATUS,
    })
}
```

**ネイティブ:** `#[kasane::plugin]` マクロが `#[state]` 構造体の `#[derive(Hash)]` から `state_hash()` を、slot 関数本体の AST 解析から `slot_deps()` を自動生成する。

### 7.3 PaintHook

Element ツリーを経由せず、paint 後の CellGrid を直接操作するフック。

```rust
// ネイティブのみ
fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
    vec![Box::new(MyHighlightHook)]
}

impl PaintHook for MyHighlightHook {
    fn id(&self) -> &str { "myplugin.highlight" }
    fn deps(&self) -> DirtyFlags { DirtyFlags::BUFFER }
    fn surface_filter(&self) -> Option<SurfaceId> { Some(SurfaceId::BUFFER) }
    fn apply(&self, grid: &mut CellGrid, region: &Rect, state: &AppState) {
        // grid のセルを直接操作
    }
}
```

`deps()` で依存フラグを宣言し、該当フラグが立っていない場合はフックがスキップされる。`surface_filter()` で特定 Surface にのみ適用を限定できる。

## 8. スタイリングとテーマ

### StyleToken (オープンシステム)

`StyleToken` はセマンティックなスタイルトークンで、テーマ設定から Face にマッピングされる。

**組み込みトークン一覧:**

| トークン名 | 用途 |
|---|---|
| `buffer.text` | バッファテキスト |
| `buffer.padding` | バッファパディング |
| `status.line` | ステータスバー |
| `status.mode` | モード表示 |
| `menu.item.normal` | メニュー通常項目 |
| `menu.item.selected` | メニュー選択項目 |
| `menu.scrollbar` / `menu.scrollbar.thumb` | スクロールバー |
| `info.text` / `info.border` | Info ポップアップ |
| `border` / `shadow` | ボーダー/シャドウ |

**カスタムトークン:**

```rust
// プラグイン初期化時
StyleToken::new("myplugin.highlight")

// on_init で登録
fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
    vec![Command::RegisterThemeTokens(vec![
        ("myplugin.highlight".into(), Face { fg: Color::Named(NamedColor::Yellow), ..Face::default() }),
    ])]
}
```

**config.toml 連携:**

```toml
[theme]
"menu.selected" = { fg = "black", bg = "blue" }
"myplugin.highlight" = { fg = "yellow" }
```

## 9. 高度なトピック

### 9.1 Surface の実装

プラグインが `SURFACE_PROVIDER` capability を持つ場合、独自の Surface を提供できる。

```rust
// ネイティブのみ
impl Plugin for MyPlugin {
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::SURFACE_PROVIDER
    }

    fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
        vec![Box::new(MySidebar::new())]
    }

    fn workspace_request(&self) -> Option<Placement> {
        Some(Placement::Dock(DockPosition::Left))
    }
}
```

**Surface trait:**

| メソッド | 説明 |
|---|---|
| `id() -> SurfaceId` | 一意な ID (`PLUGIN_BASE` (100) 以降を使用) |
| `size_hint() -> SizeHint` | サイズ希望 (fixed / fill / fixed_height) |
| `view(ctx: &ViewContext) -> Element` | Element ツリーの構築 |
| `handle_event(event, ctx) -> Vec<Command>` | イベント処理 |
| `on_state_changed(state, dirty) -> Vec<Command>` | 状態変更通知 |
| `state_hash() -> u64` | ビューキャッシュ用ハッシュ |
| `declared_slots() -> &[SlotDeclaration]` | 拡張点の宣言 |

`ViewContext` は `state`、`rect`、`focused`、`registry`、`surface_id` を提供する。

### 9.2 Workspace 操作

Workspace はレイアウトツリーで Surface の配置を管理する。`WorkspaceCommand` で操作する:

| WorkspaceCommand | 説明 |
|---|---|
| `AddSurface { surface_id, placement }` | Surface をワークスペースに追加 |
| `RemoveSurface(id)` | Surface を削除 |
| `Focus(id)` | フォーカス移動 |
| `FocusDirection(dir)` | 方向フォーカス |
| `Resize { delta }` | 分割比率調整 |
| `Swap(id1, id2)` | Surface の入れ替え |
| `Float { surface_id, rect }` | フローティング化 |
| `Unfloat(id)` | タイルレイアウトに戻す |

**Placement (配置方法):**

| Placement | 説明 |
|---|---|
| `SplitFocused { direction, ratio }` | フォーカス中の Surface を分割 |
| `SplitFrom { target, direction, ratio }` | 特定 Surface から分割 |
| `Tab` / `TabIn { target }` | タブとして追加 |
| `Dock(position)` | Left/Right/Bottom/Panel にドック |
| `Float { rect }` | フローティングウィンドウとして追加 |

`on_workspace_changed(query)` で Workspace 変更通知を受け取り、`WorkspaceQuery` で現在のレイアウトを問い合わせできる。

### 9.3 カスタムスロット定義

Surface が `declared_slots()` で SlotDeclaration を返すことで、他のプラグインが寄与できるカスタムスロットを定義する。

```rust
impl Surface for MySurface {
    fn declared_slots(&self) -> &[SlotDeclaration] {
        &[
            SlotDeclaration::new("myplugin.sidebar.top", SlotPosition::Before),
            SlotDeclaration::new("myplugin.sidebar.bottom", SlotPosition::After),
        ]
    }
}
```

他のプラグインは `contribute_named_slot("myplugin.sidebar.top", state)` で Element を寄与する。WASM では `contribute_named(slot_name)` を実装する。

### 9.4 プラグイン間通信

`Command::PluginMessage { target, payload }` で他のプラグインにメッセージを送信する。受信側は:

- **ネイティブ:** `update(msg: Box<dyn Any>, state)` でダウンキャストして処理
- **WASM:** `update(payload: Vec<u8>)` でバイト列として受信

`Command::ScheduleTimer { delay, target, payload }` でタイマーを設定し、遅延後にメッセージを送信することも可能。

### 9.5 Pane ライフサイクル

`PANE_LIFECYCLE` capability を持つプラグインは Pane の作成/削除/フォーカス変更を観測できる:

| フック | 説明 |
|---|---|
| `on_pane_created(pane_id, state)` | Pane 作成通知 |
| `on_pane_closed(pane_id)` | Pane 削除通知 |
| `on_focus_changed(from, to, state)` | フォーカス変更通知 |

`PANE_RENDERER` capability では `render_pane(pane_id, cols, rows)` でプラグイン所有の Pane コンテンツを描画できる。

## 10. 登録と配布

### 登録順序

Kasane は以下の順序でプラグインを登録する:

1. **バンドル WASM** — バイナリに埋め込まれたデフォルトプラグイン (cursor_line, color_preview, sel_badge 等)
2. **FS 発見 WASM** — `~/.local/share/kasane/plugins/*.wasm` から自動発見。同じ ID のバンドルプラグインを上書き可能
3. **ユーザーコールバック** — `kasane::run(|registry| { ... })` で登録されるネイティブプラグイン

### 配布方法

- **WASM:** `.wasm` ファイルを `~/.local/share/kasane/plugins/` にコピー
- **ネイティブ:** `kasane::run()` を使ったカスタムバイナリとして配布

### config.toml での制御

```toml
[plugins]
disabled = ["color_preview"]  # プラグイン ID でバンドル・FS 発見ともにスキップ
```

### テスト

`PluginRegistry` を直接使ってユニットテストが書ける:

```rust
#[test]
fn my_plugin_contributes_gutter() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(MyPluginPlugin::new()));

    let state = AppState::default();
    let _ = registry.init_all(&state);

    let elements = registry.collect_slot(Slot::BufferLeft, &state);
    assert_eq!(elements.len(), 1);
}
```

## 11. 参照実装一覧

| プラグイン | パス | 行数 | 主な機能 |
|---|---|---|---|
| cursor-line (WASM) | `kasane-wasm/guests/cursor-line/` | 73行 | `contribute_line()`, `state_hash()` — カーソル行ハイライト |
| sel-badge (WASM) | `kasane-wasm/guests/sel-badge/` | 73行 | `contribute()` (STATUS_RIGHT), `route_slots!` — 選択数バッジ |
| line-numbers (WASM) | `kasane-wasm/guests/line-numbers/` | 92行 | `contribute()` (BUFFER_LEFT), `element_builder` — 行番号 |
| color-preview (WASM) | `kasane-wasm/guests/color-preview/` | 567行 | `contribute_line()`, `contribute_overlay()`, `handle_mouse()` — 色検出・ピッカー |
| line-numbers (ネイティブ) | `examples/line-numbers/` | 37行 | `#[kasane_plugin]`, `#[slot]`, `kasane::run()` — 行番号 |

## 12. 付録: WASM vs ネイティブ比較表

| 観点 | WASM | ネイティブ |
|---|---|---|
| **安全性** | サンドボックス分離、ホストクラッシュ防止 | ホストと同一プロセス、unsafe も可能 |
| **パフォーマンス** | WASM 境界越えコスト (μs 級)、CapabilityFlags で軽減 | 直接関数呼び出し、ゼロオーバーヘッド |
| **API アクセス** | host-state + element-builder (WIT 定義) | `&AppState` 直接参照、全 Rust API |
| **配布** | `.wasm` ファイルを `plugins/` に配置 | カスタムバイナリとしてビルド・配布 |
| **開発体験** | SDK マクロでボイラープレート軽減、`wit-bindgen` 依存 | `#[kasane::plugin]` マクロで trait impl 自動生成 |
| **Surface/PaintHook** | 未対応 (ネイティブのみ) | 完全対応 |
| **プラグイン間通信** | `Vec<u8>` バイト列 | `Box<dyn Any>` ダウンキャスト |
