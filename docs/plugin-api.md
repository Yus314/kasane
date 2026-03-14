# Kasane プラグイン API リファレンス

本ドキュメントは Kasane のプラグイン API を引くためのリファレンスである。
最短で動くプラグインを書きたい場合は [plugin-development.md](./plugin-development.md) を、合成順序や正しさ条件を確認したい場合は [semantics.md](./semantics.md) を参照。

## 1. 拡張ポイント

### 1.1 コア surface と組み込み slot

コア UI は surface を中心に構成される。プラグインが利用する拡張点は各 surface が宣言する。

| SurfaceId | Surface | 説明 |
|---|---|---|
| `BUFFER` (0) | `KakouneBufferSurface` | メインのバッファ表示 |
| `STATUS` (1) | `StatusBarSurface` | ステータスバー |
| `MENU` (2) | `MenuSurface` | メニュー |
| `INFO_BASE`+ (10+) | `InfoSurface` | Info ポップアップ |
| `PLUGIN_BASE`+ (100+) | Plugin-defined | プラグイン提供 surface |

| SlotId | 位置 | 宣言元 Surface |
|---|---|---|
| `kasane.buffer.left` | バッファ左側 | `KakouneBufferSurface` |
| `kasane.buffer.right` | バッファ右側 | `KakouneBufferSurface` |
| `kasane.buffer.above` | バッファ上部 | `KakouneBufferSurface` |
| `kasane.buffer.below` | バッファ下部 | `KakouneBufferSurface` |
| `kasane.buffer.overlay` | バッファ上のオーバーレイ | `KakouneBufferSurface` |
| `kasane.status.above` | ステータスバー上部 | `StatusBarSurface` |
| `kasane.status.left` | ステータスバー左側 | `StatusBarSurface` |
| `kasane.status.right` | ステータスバー右側 | `StatusBarSurface` |

### 1.2 メカニズムの選び方

| やりたいこと | 使うメカニズム |
|---|---|
| 定義済みの場所に UI を追加したい | `Slot` |
| バッファの各行を装飾したい | `LineDecoration` |
| フローティング UI を表示したい | `Overlay` |
| 既存 UI の見た目を変更したい | `Decorator` |
| 既存 UI を別の UI に差し替えたい | `Replacement` |
| メニュー項目単位で変換したい | `transform_menu_item()` |
| Element ツリーを経由せず直接描画したい | `PaintHook` |

原則として、自由度が低いメカニズムを優先する。`Slot` で済むなら `Decorator` は使わず、`Decorator` で済むなら `Replacement` は使わない。

### 1.3 合成ルール

拡張の合成順序は次の通りである。

1. seed となるデフォルト view または replacement を選ぶ
2. decorator を priority 順に適用する
3. slot contribution と overlay を合成する

詳細な意味論は [semantics.md](./semantics.md) の `Plugin 合成意味論` を参照。

### 1.4 Slot

`Slot` はフレームワークが用意した挿入点に `Element` を寄与する最も制約の強い拡張である。

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

**Native:**

```rust
#[slot(Slot::BufferLeft)]
pub fn gutter(_state: &State, core: &AppState) -> Option<Element> {
    Some(Element::text("★", Face::default()))
}
```

`slot::BUFFER_LEFT` (=0) から `slot::OVERLAY` (=7) までの定数は `kasane_plugin_sdk::slot` モジュールで定義されている。

### 1.5 LineDecoration

`LineDecoration` はバッファ各行にガターや背景を寄与する。

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

`LineDecoration` は `left_gutter`、`right_gutter`、`background` の 3 要素で構成される。複数プラグインのガター寄与は水平に合成される。

### 1.6 Overlay

`Overlay` は通常のレイアウトフローとは別に重畳される浮動要素である。

```rust
// WASM
fn contribute_overlay() -> Option<Overlay> {
    Some(Overlay {
        element: element_builder::create_container_styled(child, ...),
        anchor: OverlayAnchor::Absolute(AbsoluteAnchor { x: 10, y: 5, w: 30, h: 10 }),
    })
}

// Native
fn contribute_overlay(&self, _state: &AppState) -> Option<Overlay> {
    Some(Overlay {
        element: Element::container(child, style),
        anchor: OverlayAnchor::AnchorPoint { coord, prefer_above: true, avoid: vec![] },
    })
}
```

`OverlayAnchor` には次の 2 種類がある。

- `Absolute { x, y, w, h }`: 画面座標に対する絶対位置
- `AnchorPoint { coord, prefer_above, avoid }`: Kakoune 互換のアンカーベース配置

### 1.7 Decorator

`Decorator` は既存 `Element` を受け取り、ラップまたは変換して返す。

**WASM:**

```rust
fn decorate(target: DecorateTarget, element: ElementHandle) -> ElementHandle {
    element_builder::create_container(element, Some(BorderLineStyle::Single), false, edges)
}

fn decorator_priority() -> u32 { 100 }
```

**Native:**

```rust
#[decorate(DecorateTarget::Buffer, priority = 100)]
pub fn decorate(_state: &State, element: Element, _core: &AppState) -> Element {
    Element::container(element, Style::from(Face::default()))
}
```

対象は `Buffer`、`StatusBar`、`Menu`、`Info`、`BufferLine(n)` など。

ガイドライン:

- 受け取った `Element` の内部構造を仮定しない
- `Element` をそのままラップする形を優先する
- 完全差し替えが目的なら `Replacement` を使う

### 1.8 Replacement

`Replacement` は既存コンポーネントの view 構築を完全に差し替える。

```rust
#[replace(ReplaceTarget::MenuPrompt)]
pub fn replace(_state: &State, _core: &AppState) -> Option<Element> {
    Some(Element::text("custom menu", Face::default()))
}
```

| ReplaceTarget | 説明 |
|---|---|
| `MenuPrompt` | プロンプトメニュー |
| `MenuInline` | インラインメニュー |
| `MenuSearch` | 検索メニュー |
| `InfoPrompt` | プロンプト Info |
| `InfoModal` | モーダル Info |
| `StatusBar` | ステータスバー全体 |

`Replacement` が差し替えるのは view のみであり、プロトコル処理や core state machine は差し替えない。

### 1.9 Transform

`transform_menu_item()` はメニュー項目単位の変換であり、`MENU_TRANSFORM` capability に対応する。項目ごとのラベルや style を局所的に変換したい場合に使う。全メニュー構造の差し替えが必要なら `Replacement` を使う。

## 2. Element API

### 2.1 Element variants

| 型 | 用途 | WASM builder | Native |
|---|---|---|---|
| `Text` | テキスト + スタイル | `create_text(content, face)` | `Element::text(s, face)` |
| `StyledLine` | Atom 列 | `create_styled_line(atoms)` | `Element::styled_line(line)` |
| `Flex` (Column) | 垂直配置 | `create_column(children)` / `create_column_flex(entries, gap)` | `Element::column(children)` |
| `Flex` (Row) | 水平配置 | `create_row(children)` / `create_row_flex(entries, gap)` | `Element::row(children)` |
| `Grid` | 2D テーブル | `create_grid(cols, children, col_gap, row_gap)` | `Element::grid(columns, children)` |
| `Container` | border/shadow/padding | `create_container(...)` / `create_container_styled(...)` | `Element::container(child, style)` |
| `Stack` | Z 軸重ね | `create_stack(base, overlays)` | `Element::stack(base, overlays)` |
| `Scrollable` | スクロール可能領域 | `create_scrollable(child, offset, vertical)` | `Element::Scrollable { ... }` |
| `Interactive` | マウスヒットテスト | `create_interactive(child, id)` | `Element::Interactive { child, id }` |
| `Empty` | 空要素 | `create_empty()` | `Element::Empty` |
| `BufferRef` | バッファ行参照 | ホスト内部のみ | `Element::buffer_ref(range)` |

### 2.2 WASM element-builder API

すべての関数は `element_builder` モジュールからインポートする。返り値の `ElementHandle` は現在のプラグイン呼び出しスコープ内でのみ有効。

```rust
use kasane::plugin::element_builder;

let text = element_builder::create_text("hello", face);
let col = element_builder::create_column(&[text]);
let container = element_builder::create_container(
    col,
    Some(BorderLineStyle::Single),
    false,
    Edges { top: 0, right: 1, bottom: 0, left: 1 },
);
```

比例配分を使う場合は `create_column_flex` / `create_row_flex` と `FlexEntry { child, flex }` を使う。

### 2.3 Native element construction

```rust
use kasane_core::plugin_prelude::*;

let text = Element::text("hello", Face::default());
let col = Element::column(vec![
    FlexChild::fixed(text),
    FlexChild::flexible(Element::Empty, 1.0),
]);
```

`FlexChild::fixed(element)` は fixed、`FlexChild::flexible(element, factor)` は比例配分である。

## 3. 状態アクセスとイベント

### 3.1 AppState overview

Native plugin は `&AppState` を直接参照できる。

| フィールド | 型 | 説明 |
|---|---|---|
| `lines` | `Vec<Line>` | バッファ行 |
| `cursor_pos` | `Coord` | カーソル位置 |
| `status_line` | `Line` | ステータスバー |
| `menu` | `Option<MenuState>` | メニュー状態 |
| `infos` | `Vec<InfoState>` | Info ポップアップ |
| `cols`, `rows` | `u16` | 端末サイズ |
| `focused` | `bool` | フォーカス状態 |

Dirty flags は主に次の観測面を通知する。

| フラグ | 説明 |
|---|---|
| `BUFFER` | バッファ行・カーソル |
| `STATUS` | ステータスバー |
| `MENU_STRUCTURE` | メニュー構造 |
| `MENU_SELECTION` | メニュー選択 |
| `INFO` | Info ポップアップ |
| `OPTIONS` | UI オプション |

意味論上の分類は [semantics.md](./semantics.md) を参照。

### 3.2 WASM host-state API

`kasane::plugin::host_state` は段階的な読み取り API を提供する。

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

### 3.3 Lifecycle hooks

| フック | タイミング | 用途 |
|---|---|---|
| `on_init` | `PluginRegistry` 登録直後 | 初期化、テーマトークン登録 |
| `on_shutdown` | アプリケーション終了時 | クリーンアップ |
| `on_state_changed(dirty)` | `AppState` 更新後 | プラグイン内部状態の同期 |

### 3.4 Input handling

キー入力の処理順は次の通りである。

1. `observe_key()` を全プラグインへ通知する
2. `handle_key()` を順に呼ぶ
3. 最初に `Some(commands)` を返したプラグインが勝つ
4. すべて `None` の場合は組み込みキーバインドへ進む
5. それでも処理されなければ Kakoune に転送する

マウス入力は `observe_mouse()` の後、`InteractiveId` ヒットテストを経て `handle_mouse(event, id, state)` に渡される。

### 3.5 Commands

フック関数は `Vec<Command>` を返して副作用要求を発行する。

| Command | 説明 |
|---|---|
| `SendToKakoune(req)` | Kakoune にリクエストを送信 |
| `Paste` | クリップボード貼り付け |
| `Quit` | アプリケーション終了 |
| `RequestRedraw(flags)` | 再描画を要求 |
| `ScheduleTimer { delay, target, payload }` | タイマー後に target へメッセージ送信 |
| `PluginMessage { target, payload }` | 他プラグインへメッセージ送信 |
| `SetConfig { key, value }` | ランタイム設定変更 |
| `Pane(PaneCommand)` | Pane 操作 |
| `Workspace(WorkspaceCommand)` | Workspace 操作 |
| `RegisterThemeTokens(tokens)` | カスタムテーマトークン登録 |

WASM では `command` variant で表現される。`Pane`、`Workspace`、`RegisterThemeTokens` は現時点では WASM 未対応。

## 4. Capability とキャッシュ

### 4.1 PluginCapabilities

`PluginCapabilities` は plugin が実装する機能を宣言するビットフラグであり、不要なメソッド呼び出しをスキップするために使われる。

| フラグ | 説明 |
|---|---|
| `SLOT_CONTRIBUTOR` | `contribute()` / `contribute_slot()` |
| `LINE_DECORATION` | `contribute_line()` |
| `OVERLAY` | `contribute_overlay()` |
| `DECORATOR` | `decorate()` |
| `REPLACEMENT` | `replace()` |
| `MENU_TRANSFORM` | `transform_menu_item()` |
| `CURSOR_STYLE` | `cursor_style_override()` |
| `INPUT_HANDLER` | `handle_key()` / `handle_mouse()` |
| `NAMED_SLOT` | `contribute_named_slot()` |
| `PANE_LIFECYCLE` | pane lifecycle hooks |
| `PANE_RENDERER` | `render_pane()` |
| `SURFACE_PROVIDER` | `surfaces()` |
| `WORKSPACE_OBSERVER` | `on_workspace_changed()` |
| `PAINT_HOOK` | `paint_hooks()` |

Native plugin のデフォルトは `all()`、WASM adapter は WIT 呼び出し結果から設定される。

### 4.2 State hash and slot deps

plugin の寄与結果は主に次の 2 層でキャッシュされる。

- `state_hash()`: プラグイン内部状態のハッシュ
- `slot_deps()` / `slot_id_deps()`: 指定 slot が依存する `DirtyFlags`

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

Native plugin では `#[kasane::plugin]` macro が `#[derive(Hash)]` と AST 解析から `state_hash()` と `slot_deps()` の一部を自動生成する。

### 4.3 PaintHook

`PaintHook` は `Element` ツリーを経由せず、paint 後の `CellGrid` を直接操作する native-only hook である。

```rust
fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
    vec![Box::new(MyHighlightHook)]
}

impl PaintHook for MyHighlightHook {
    fn id(&self) -> &str { "myplugin.highlight" }
    fn deps(&self) -> DirtyFlags { DirtyFlags::BUFFER }
    fn surface_filter(&self) -> Option<SurfaceId> { Some(SurfaceId::BUFFER) }
    fn apply(&self, grid: &mut CellGrid, region: &Rect, state: &AppState) {
        // mutate grid directly
    }
}
```

## 5. Styling

### 5.1 StyleToken

`StyleToken` はテーマ設定から `Face` にマッピングされるセマンティックなスタイルトークンである。

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
| `border` / `shadow` | ボーダー / シャドウ |

カスタムトークンは plugin 側で作成して登録できる。

```rust
StyleToken::new("myplugin.highlight")

fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
    vec![Command::RegisterThemeTokens(vec![
        ("myplugin.highlight".into(), Face {
            fg: Color::Named(NamedColor::Yellow),
            ..Face::default()
        }),
    ])]
}
```

### 5.2 config.toml integration

```toml
[theme]
"menu.selected" = { fg = "black", bg = "blue" }
"myplugin.highlight" = { fg = "yellow" }
```

## 6. 高度な API

### 6.1 Surface provider

`SURFACE_PROVIDER` capability を持つ native plugin は独自の surface を提供できる。

```rust
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

| メソッド | 説明 |
|---|---|
| `id() -> SurfaceId` | 一意な ID |
| `size_hint() -> SizeHint` | サイズ希望 |
| `view(ctx: &ViewContext) -> Element` | `Element` ツリー構築 |
| `handle_event(event, ctx) -> Vec<Command>` | イベント処理 |
| `on_state_changed(state, dirty) -> Vec<Command>` | 状態変化通知 |
| `state_hash() -> u64` | view cache 用ハッシュ |
| `declared_slots() -> &[SlotDeclaration]` | 拡張点宣言 |

`ViewContext` は `state`、`rect`、`focused`、`registry`、`surface_id` を提供する。

### 6.2 Workspace commands

`WorkspaceCommand` は surface の配置とレイアウトを操作する。

| WorkspaceCommand | 説明 |
|---|---|
| `AddSurface { surface_id, placement }` | surface を追加 |
| `RemoveSurface(id)` | surface を削除 |
| `Focus(id)` | フォーカス移動 |
| `FocusDirection(dir)` | 方向フォーカス |
| `Resize { delta }` | 分割比率調整 |
| `Swap(id1, id2)` | surface 入れ替え |
| `Float { surface_id, rect }` | フローティング化 |
| `Unfloat(id)` | タイルに戻す |

| Placement | 説明 |
|---|---|
| `SplitFocused { direction, ratio }` | フォーカス中 surface を分割 |
| `SplitFrom { target, direction, ratio }` | 特定 surface から分割 |
| `Tab` / `TabIn { target }` | タブ追加 |
| `Dock(position)` | Left/Right/Bottom/Panel にドック |
| `Float { rect }` | フローティングとして追加 |

### 6.3 Custom slots

surface が `declared_slots()` を返すことで、他 plugin が寄与できるカスタム slot を定義できる。

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

他 plugin は `contribute_named_slot("myplugin.sidebar.top", state)` を使う。WASM では `contribute_named(slot_name)` を実装する。

### 6.4 Plugin messages and timers

`Command::PluginMessage { target, payload }` で plugin 間メッセージ送信ができる。

- Native: `update(msg: Box<dyn Any>, state)` でダウンキャスト
- WASM: `update(payload: Vec<u8>)` でバイト列受信

`Command::ScheduleTimer { delay, target, payload }` は遅延メッセージ送信を行う。

### 6.5 Pane lifecycle

`PANE_LIFECYCLE` capability を持つ plugin は pane の作成、削除、フォーカス変更を観測できる。

| フック | 説明 |
|---|---|
| `on_pane_created(pane_id, state)` | pane 作成通知 |
| `on_pane_closed(pane_id)` | pane 削除通知 |
| `on_focus_changed(from, to, state)` | フォーカス変更通知 |

`PANE_RENDERER` capability では `render_pane(pane_id, cols, rows)` で plugin 所有 pane を描画できる。

## 7. 関連文書

- [plugin-development.md](./plugin-development.md) — 最短ガイド
- [semantics.md](./semantics.md) — 合成順序と意味論
- [architecture.md](./architecture.md) — surface と backend の位置づけ
- [index.md](./index.md) — docs 全体の入口
