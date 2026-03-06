# 宣言的 UI アーキテクチャ設計書

本ドキュメントでは、kasane を宣言的 UI 基盤として再設計するための詳細設計を記述する。
決定の根拠と比較検討は [decisions.md](./decisions.md) の ADR-009 を参照。
パフォーマンス特性とボトルネック対策は [performance.md](./performance.md) を参照。

## 設計思想

kasane の目指す姿は「プラグイン作成者のための UI 基盤」である。

- **宣言的**: プラグインは「何を表示したいか」を記述し、「どう描画するか」はフレームワークが決定する
- **拡張性**: Slot / Decorator / Replacement の三段階でプラグインが UI を拡張できる
- **設定可能性**: テーマ・レイアウト・キーバインドをユーザーが設定で変更できる
- **基盤と設定の一貫性**: 基盤は汎用メカニズム (OverlayAnchor, Container, Flex 等) を提供し、設定 (config.toml) とプラグイン (Plugin trait) の両方がその上に構築される。ユーザーが設定だけで済む場合に Rust を書かせない
- **段階的分離**: 初期は Kakoune 結合のまま構築し、安定後に汎用部分を分離する
- **コンパイラ駆動**: proc macro がコンパイル時に依存解析・レイアウトキャッシュ・更新コード生成を行い、ランタイムコストを最小化する ([ADR-010](./decisions.md#adr-010-コンパイラ駆動最適化--svelte-的二層レンダリング))

## 全体アーキテクチャ

```
                Kakoune (kak -ui json)
                    │ stdout (JSON-RPC)
                    ▼
             Protocol Parser (protocol.rs)
                    │
                    ▼ Msg::Kakoune(KakouneRequest)
┌───────────────────────────────────────────────────────────────────┐
│                      TEA イベントループ                            │
│                                                                   │
│   Event ──→ Msg ──→ update(&mut State, Msg) ──→ Vec<Command>     │
│                          │                          │             │
│                          ▼                          ▼             │
│                       State                   副作用の実行         │
│                          │                (SendToKakoune 等)       │
│                          ▼                                        │
│   view(&State) ──→ Element ツリー (プラグイン Slot/Decorator 合成)  │
│                         │                                         │
│                         ▼                                         │
│                   レイアウト計算 (Flex + Overlay + Grid)            │
│                         │                                         │
│                         ▼                                         │
│                   paint() ──→ CellGrid                            │
│                                  │                                │
│                                  ▼                                │
│                            差分検出 → 端末出力                     │
└───────────────────────────────────────────────────────────────────┘
                    │ stdin (JSON-RPC)
                    ▼
             Kakoune (kak -ui json)
```

## Element ツリー

### 基本型

```rust
/// UI の宣言的記述。所有型 (ライフタイムなし)。
enum Element {
    /// テキスト (最小単位)
    Text(String, Style),

    /// Kakoune の Atom 列 (スタイル付きテキスト)
    StyledLine(Vec<Atom>),

    /// 子要素の直線的配置 (Flexbox)
    Flex {
        direction: Direction,
        children: Vec<FlexChild>,
        gap: u16,
        align: Align,
        cross_align: Align,
    },

    /// 子要素を重ねて描画 (Z 軸)
    Stack {
        base: Box<Element>,
        overlays: Vec<Overlay>,
    },

    /// 表形式の配置
    Grid {
        columns: Vec<GridColumn>,
        rows: Vec<Vec<Element>>,
    },

    /// スクロール可能な領域
    Scrollable {
        child: Box<Element>,
        offset: u16,
        direction: Direction,
    },

    /// 装飾 (border, shadow, padding)
    Container {
        child: Box<Element>,
        border: Option<BorderStyle>,
        shadow: bool,
        padding: Edges,
        style: Style,
    },

    /// マウスヒットテスト対象
    Interactive {
        child: Box<Element>,
        id: InteractiveId,
    },

    /// 何も表示しない (条件付き表示の「偽」側)
    Empty,
}

// 注: バッファ行の大量 clone を避けるため、実装段階で
// Element::BufferRef パターンを適用する。
// paint 時に &AppState から直接 CellGrid に描画し、clone コストをゼロにする。
// 詳細は performance.md の「バッファ行の clone」を参照。

/// Flex の子要素
struct FlexChild {
    element: Element,
    flex: f32,            // 0.0 = 固定, >0.0 = 比例配分
    min_size: Option<u16>,
    max_size: Option<u16>,
}
```

### Overlay の位置指定

```rust
struct Overlay {
    element: Element,
    anchor: OverlayAnchor,
}

enum OverlayAnchor {
    /// 画面座標に対する絶対位置
    Absolute(Coord),
    /// 親要素に対する相対位置
    Relative { x: Align, y: Align },
    /// Kakoune 互換の anchor ベース配置 (compute_pos 相当)
    AnchorPoint {
        coord: Coord,
        prefer_above: bool,
        avoid: Vec<Rect>,
    },
}
```

### Grid の列定義

```rust
struct GridColumn {
    width: GridWidth,
}

enum GridWidth {
    Fixed(u16),
    Flex(f32),
    Auto,   // 内容に合わせる
}
```

## レイアウト計算

二段階アルゴリズム (Flutter モデル):

### Phase 1: Measure (下→上)

各要素が「与えられた制約内で自分はこのサイズ」と報告する。

```rust
struct Constraints {
    min_width: u16, max_width: u16,
    min_height: u16, max_height: u16,
}

fn measure(element: &Element, constraints: Constraints) -> Size;
```

### Phase 2: Place (上→下)

親が子の具体的な位置を決定する。

```rust
struct LayoutResult {
    area: Rect,
    children: Vec<LayoutResult>,
}

fn place(element: &Element, area: Rect) -> LayoutResult;
```

### Flex レイアウトの計算手順

1. 固定子 (flex=0.0) を先に測定し、必要なサイズを確定
2. 残り空間を flex 値の比率で可変子に分配
3. min/max 制約を適用し、溢れた分を再分配
4. 各子の位置を direction に従って配置

### Overlay の配置

Overlay は通常の Flex レイアウトとは独立に計算する:

1. base 要素を通常通りレイアウト
2. 各 Overlay の要素を測定 (サイズ確定)
3. AnchorPoint の場合は既存の `compute_pos` ロジックで位置決定 (画面端クランプ、フリップ、衝突回避)

## 描画: paint()

レイアウト計算後、Element ツリーを CellGrid に描画する。`&AppState` を渡すことで BufferRef パターンが可能になる。

```rust
fn paint(
    element: &Element,
    layout: &LayoutResult,
    grid: &mut CellGrid,
    state: &AppState,
    theme: &Theme,
) {
    match element {
        Element::Text(text, style) => {
            let face = theme.resolve(style);
            grid.put_str(layout.area, text, &face);
        }
        Element::StyledLine(atoms) => {
            grid.put_atoms(layout.area, atoms);
        }
        Element::BufferRef { line_range } => {
            // clone なし。state.core.lines から直接描画
            for (i, line) in state.core.lines[line_range.clone()].iter().enumerate() {
                grid.put_atoms(layout.area.row(i), line);
            }
        }
        Element::Flex { children, .. } => {
            for (child, child_layout) in children.iter().zip(&layout.children) {
                paint(&child.element, child_layout, grid, state, theme);
            }
        }
        Element::Stack { base, overlays } => {
            paint(base, &layout.children[0], grid, state, theme);
            for (overlay, overlay_layout) in overlays.iter().zip(&layout.children[1..]) {
                paint(&overlay.element, overlay_layout, grid, state, theme);
            }
        }
        // Grid, Scrollable, Container, Interactive は同様に再帰
        _ => {}
    }
}
```

**フレームワーク vs プラグインの責務:**

| 責務 | フレームワーク | プラグイン |
|------|-------------|----------|
| Element ツリーの合成 | Slot 収集、Decorator/Replacement 適用 | contribute(), decorate(), replace() を実装 |
| レイアウト計算 | measure() + place() を実行 | 関与しない (フレームワークに委任) |
| CellGrid への描画 | paint() を実行、BufferRef を解決 | 関与しない (Element で宣言するのみ) |
| イベントルーティング | キーディスパッチ、InteractiveId ヒットテスト | handle_key(), handle_mouse() を実装 |
| 差分描画・端末出力 | grid.diff() + backend.draw() | 関与しない |

## 状態管理 (TEA)

### コア状態

```rust
struct AppState {
    /// Kakoune プロトコル由来の状態
    core: CoreState,
    /// プラグイン状態 (型消去して保持)
    plugin_states: HashMap<PluginId, Box<dyn Any>>,
    /// フォーカス管理
    focus: Focus,
}

struct CoreState {
    lines: Vec<Line>,
    cursor_pos: Coord,
    cursor_mode: CursorMode,
    status_line: Line,
    status_mode_line: Line,
    default_face: Face,
    padding_face: Face,
    menu: Option<MenuState>,
    info: Option<InfoState>,
    ui_options: HashMap<String, String>,
    cols: u16,
    rows: u16,
}

enum Focus {
    Buffer,
    Plugin(PluginId),
    Menu,
}
```

### メッセージ型

```rust
enum Msg {
    Kakoune(KakouneRequest),
    Input(InputEvent),
    Resize(u16, u16),
    Plugin(PluginId, Box<dyn Any>),  // プラグインの Msg は型消去
}
```

### コマンド型 (副作用)

```rust
enum Command {
    SendToKakoune(KasaneRequest),
    Quit,
    Broadcast(BroadcastEvent),
    Async(Box<dyn FnOnce() -> Result<Msg>>),
}
```

### update 関数

```rust
fn update(state: &mut AppState, msg: Msg) -> Vec<Command> {
    match msg {
        Msg::Kakoune(req) => {
            state.core.apply(req);  // 既存ロジック活用
            vec![]
        }
        Msg::Input(InputEvent::Key(key)) => {
            // 1. グローバルキーバインド
            // 2. フォーカスされたプラグインに問い合わせ
            // 3. デフォルト: Kakoune に転送
        }
        Msg::Input(InputEvent::Mouse(mouse)) => {
            // InteractiveId によるヒットテストで対象を特定
            // 対象プラグインの handle_mouse() を呼ぶ
        }
        Msg::Plugin(id, plugin_msg) => {
            // 該当プラグインの update() を呼ぶ
        }
    }
}
```

### view 関数

```rust
fn view(state: &AppState, registry: &PluginRegistry) -> Element {
    let mut root = core_view(&state.core, registry);
    root = registry.apply_decorators(root, &state.core);
    root = registry.apply_replacements(root, &state.core);
    root
}
```

## プラグインシステム

### Plugin trait

```rust
trait Plugin: Any {
    /// プラグインの一意な識別子
    fn id(&self) -> PluginId;

    /// イベント処理 (型消去された Msg をダウンキャストして処理)
    fn update(&mut self, msg: Box<dyn Any>, core: &CoreState) -> Vec<Command>;

    /// キーイベント処理 (フォーカス時に呼ばれる)
    fn handle_key(&mut self, key: KeyEvent, core: &CoreState) -> Option<Vec<Command>>;

    /// マウスイベント処理 (InteractiveId がマッチした時に呼ばれる)
    fn handle_mouse(&mut self, event: MouseEvent, core: &CoreState) -> Option<Vec<Command>>;

    /// Slot への Element 提供
    fn contribute(&self, slot: Slot, core: &CoreState) -> Option<Element>;

    /// Decorator: 既存 Element のラップ
    fn decorate(&self, target: DecorateTarget, element: Element, core: &CoreState) -> Element {
        element  // デフォルト: 変更なし
    }

    /// Replacement: 既存コンポーネントの差替
    fn replace(&self, target: ReplaceTarget, core: &CoreState) -> Option<Element> {
        None  // デフォルト: 差替なし
    }

    /// グローバルキーバインド登録
    fn keybindings(&self) -> Vec<(KeyEvent, Box<dyn Any>)> {
        vec![]
    }
}
```

### proc macro によるプラグイン定義

```rust
#[kasane::plugin]
mod line_numbers {
    /// プラグイン固有の状態
    #[state]
    struct State {
        enabled: bool,
        width: u16,
    }

    /// プラグイン固有のメッセージ
    #[event]
    enum Msg {
        Toggle,
        SetWidth(u16),
    }

    /// 状態更新
    fn update(state: &mut State, msg: Msg, core: &CoreState) -> Vec<Command> {
        match msg {
            Msg::Toggle => { state.enabled = !state.enabled; vec![] }
            Msg::SetWidth(w) => { state.width = w; vec![] }
        }
    }

    /// Slot に Element を提供
    #[slot(Slot::BufferLeft)]
    fn view(state: &State, core: &CoreState) -> Option<Element> {
        if !state.enabled { return None; }
        // 行番号列を構築して返す
    }

    /// グローバルキーバインド
    #[keybind("ctrl-l")]
    fn toggle() -> Msg {
        Msg::Toggle
    }
}
```

`#[kasane::plugin]` macro が自動生成するもの:
- `Plugin` trait の実装 (型消去のディスパッチコード含む)
- State のシリアライズ/デシリアライズ (設定永続化)
- キーバインド登録コード
- Config 統合コード (`[plugin.line_numbers]` セクション)

### proc macro によるコンポーネント定義

```rust
#[kasane::component]
fn file_tree(entries: &[Entry], selected: usize) -> Element {
    scrollable(
        column(entries.iter().enumerate().map(|(i, entry)| {
            let style = if i == selected { "selected" } else { "normal" };
            text(&entry.name).style(style)
        }))
    )
}
```

`#[kasane::component]` macro は Svelte 的な「コンパイラに仕事をさせる」思想に基づき、宣言的な view() から最適化されたコードを段階的に生成する ([ADR-010](./decisions.md#adr-010-コンパイラ駆動最適化--svelte-的二層レンダリング)):

**段階 1: 入力メモ化**

入力パラメータの前回値を保持し、全入力が同一なら Element 構築をスキップする:

```rust
#[kasane::component]
fn file_tree(entries: &[Entry], selected: usize) -> Element { ... }
// → entries, selected が前回と同じなら、キャッシュ済み Element を返す
```

**段階 2: 静的レイアウトキャッシュ**

proc macro が構造の静的部分を検出し、レイアウトを一度だけ計算する:

```rust
#[kasane::component]
fn status_bar(mode: &str, file: &str, pos: Coord) -> Element {
    flex(Row, [
        text(mode).style("mode"),       // 内容は動的、構造は静的
        text(file).style("file"),
        text(&format!("{}:{}", pos.row, pos.col)).style("position"),
    ])
}
// → flex(Row, [...]) の構造は入力に依存しない
// → layout 結果を一度計算してキャッシュ (リサイズ時のみ再計算)
```

**段階 3: 細粒度更新コード生成**

proc macro が各 Element の依存する入力パラメータを AST レベルで静的解析し、変更されたセルのみ CellGrid を直接更新するコードを生成する:

```rust
// 上の status_bar に対し、proc macro が概念的に以下を生成:
struct StatusBarCache {
    prev_mode: String,
    prev_file: String,
    prev_pos: Coord,
    layout: LayoutResult,  // 段階 2 のキャッシュ
}

fn status_bar_update(cache: &mut StatusBarCache, mode: &str, file: &str, pos: Coord, grid: &mut CellGrid) {
    if cache.prev_mode != mode {
        grid.put_str(cache.layout.children[0].area, mode, &theme.mode);
        cache.prev_mode = mode.to_string();
    }
    if cache.prev_file != file {
        grid.put_str(cache.layout.children[1].area, file, &theme.file);
        cache.prev_file = file.to_string();
    }
    if cache.prev_pos != pos {
        let s = format!("{}:{}", pos.row, pos.col);
        grid.put_str(cache.layout.children[2].area, &s, &theme.position);
        cache.prev_pos = pos;
    }
    // Element ツリーの構築、layout()、全体 paint() を完全にスキップ
}
```

**二層レンダリングモデル:**

`#[kasane::component]` による最適化は「コンパイル済みパス」として機能し、従来の Element ツリーによる「インタープリタパス」と共存する:

```
              +---------------------+
              |   宣言的 API 層      |  ← プラグイン作者が触る
              |  (Element, view())   |
              +------+--------------+
                     |
         +-----------+----------+
         v                      v
  コンパイル済みパス       インタープリタパス
  (proc macro 生成)       (汎用 Element ツリー)
         |                      |
  静的構造 → 直接         Element → layout()
    CellGrid 更新          → paint() → CellGrid
```

- **コンパイル済みパス**: `#[kasane::component]` が静的解析できる部分。Element ツリーを経由せず直接 CellGrid を更新
- **インタープリタパス**: プラグインが `Plugin::contribute()` で動的に Element を提供する部分。従来のフルパス
- **フォールバック**: `#[kasane::component]` なしのコードはインタープリタパスで動作。最適化はオプトインで、正しさは常にインタープリタパスが保証する

## 拡張モデル

### 三段階の使い分けガイド

プラグイン作者は、以下の判断基準で拡張メカニズムを選択する:

```
やりたいこと                              → 使うメカニズム
───────────────────────────────────────────────────────
定義済みの場所に UI を追加したい           → Slot
  例: 行番号、スクロールバー、タブバー

既存 UI の見た目を変更したい               → Decorator
  例: ボーダー追加、背景色変更、テーマ適用

既存 UI を根本的に別の UI にしたい         → Replacement
  例: fzf 風メニュー、カスタムステータスバー
```

**原則: 自由度が低いメカニズムを優先する。** Slot で済むなら Decorator は使わない。Decorator で済むなら Replacement は使わない。自由度が低い方が安全で、他のプラグインとの共存も容易。

### 三段階の合成ルール

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

**Decorator のガイドライン:** Decorator は受け取った Element の内部構造を仮定してはならない。「Element をそのままラップする」パターン（Container で包む等）は安全だが、「Element を分解して内部を変更する」パターンは Replacement との組み合わせで壊れる可能性がある。

### Replacement の責務境界

Replacement が差し替えるのは**ビュー（Element の構築）のみ**である。プロトコル処理は常に基盤が管理する:

```
プロトコル受信（基盤が管理）:
  menu_show  → AppState.menu = Some(MenuState)   ← 変わらない
  menu_select → MenuState.selected = n            ← 変わらない
  menu_hide  → AppState.menu = None               ← 変わらない

ビュー構築（Replacement が差し替え可能）:
  MenuState → Element                             ← プラグインが独自実装

プロトコル送信（プラグインが Command で発行可能）:
  Command::SendToKakoune(MenuSelect { index })    ← プラグインが自分で発行
```

Replacement のレベル:

| レベル | 内容 | 例 |
|--------|------|---|
| ビューのみ | 見た目だけ変える。基盤のビヘイビアをそのまま使用 | 角丸メニュー、縦型レイアウト |
| ビュー + 状態 | プラグイン固有の状態を持ち、それに基づいてビューを構築 | フィルタリングメニュー |
| ビュー + 状態 + 入力処理 | キー入力も処理して独自のインタラクションを実現 | fzf 風ファジーファインダー |

レベル 2・3 では、プラグインの `handle_key()` がキー入力を処理し、`update()` が状態を管理する。これらは Plugin trait の既存メカニズムで実現される。

### Slot (挿入点)

```rust
enum Slot {
    BufferLeft,       // バッファの左側 (行番号, git ガター等)
    BufferRight,      // バッファの右側 (ミニマップ等)
    AboveBuffer,      // バッファの上 (タブバー等)
    BelowBuffer,      // バッファの下 (ターミナル等)
    AboveStatus,      // ステータスバーの上
    StatusLeft,       // ステータスバー左側
    StatusRight,      // ステータスバー右側
    Overlay,          // 浮動ウィンドウ
}
```

フレームワークの view 内で Slot が使われる:

```rust
fn core_view(core: &CoreState, slots: &SlotRegistry) -> Element {
    flex(Column, [
        ..slots.get(Slot::AboveBuffer),
        flex(Row, [
            ..slots.get(Slot::BufferLeft),
            child(buffer_view(&core.lines), flex: 1.0),
            ..slots.get(Slot::BufferRight),
        ]),
        ..slots.get(Slot::BelowBuffer),
        ..slots.get(Slot::AboveStatus),
        flex(Row, [
            child(status_line(&core.status_line), flex: 1.0),
            ..slots.get(Slot::StatusLeft),
            ..slots.get(Slot::StatusRight),
            child(mode_line(&core.status_mode_line), flex: 0.0),
        ]),
    ])
    .overlays(slots.get(Slot::Overlay))
}
```

### Decorator (ラッパー)

既存の Element を受け取り、変換して返す。

```rust
enum DecorateTarget {
    Buffer,              // バッファ表示全体
    StatusBar,           // ステータスバー全体
    Menu,                // メニュー表示全体
    Info,                // 情報ポップアップ全体
    BufferLine(usize),   // 個別のバッファ行
}
```

Decorator の適用順序は priority 値で制御する (高い値 = 内側、先に適用):

```rust
#[decorate(DecorateTarget::Buffer, priority = 100)]
fn decorate(buffer: Element, state: &State, core: &CoreState) -> Element {
    flex(Row, [
        child(line_numbers(core), flex: 0.0),
        child(buffer, flex: 1.0),
    ])
}
```

### Replacement (差替)

既存コンポーネントを完全に差し替える。プロトコル不整合のリスクが低い対象に限定する。

```rust
enum ReplaceTarget {
    MenuPrompt,    // プロンプトメニュー表示
    MenuInline,    // インラインメニュー表示
    MenuSearch,    // 検索メニュー表示
    InfoPrompt,    // プロンプト情報ポップアップ
    InfoModal,     // モーダル情報ポップアップ
    StatusBar,     // ステータスバー全体
}
```

Replacement は明示的な opt-in:

```rust
#[replace(ReplaceTarget::MenuPrompt)]
fn view(state: &State, core: &CoreState, menu: &MenuState) -> Element {
    // カスタムメニュー実装
}
```

同一ターゲットに複数の Replacement が競合した場合、最後に登録されたものが優先される。ユーザーが設定で選択可能。

## イベント伝播

### キーイベント (全プラグイン問い合わせ)

明示的な「フォーカス」概念を持たず、全プラグインの `handle_key()` を優先度順に問い合わせる。各プラグインは `AppState` を参照して自分が処理すべきか判断する。

```
キー入力
  │
  ▼
グローバルキーバインドに一致？ ──→ 該当プラグインの Msg を update() へ
  │ (不一致)
  ▼
全プラグインの handle_key() を優先度順に呼ぶ
  │  各プラグインは state を見て自己判断:
  │    - state.menu.is_some() なら Menu Replacement が処理
  │    - 自分の Overlay が active なら処理
  │    - それ以外は None を返す
  │
  ▼ 最初に Some(commands) を返したプラグインが勝つ
  │ (全て None の場合)
  ▼
デフォルト: Kakoune に転送
```

**設計根拠:**
- TEA の原則に合致: `AppState` が真実の源泉。プラグインは state を見て自己判断する
- フォーカス管理の複雑さを回避: 「誰がいつフォーカスを移すか」の暗黙的な状態遷移が不要
- Replacement プラグインの自然なサポート: Menu Replacement は `state.menu.is_some()` のとき自動的にキーを受け取る
- カーソル位置は state から決定的に計算: `cursor_mode == Prompt` → ステータスバー上、`menu.is_some()` → メニュー、それ以外 → バッファ

### マウスイベント (InteractiveId ヒットテスト)

```
マウスクリック (x, y)
  │
  ▼
レイアウト結果から InteractiveId を特定
  │
  ▼
該当プラグインの handle_mouse() を呼ぶ
  │ (未処理の場合)
  ▼
デフォルト: Kakoune に転送
```

Element に InteractiveId を付与:

```rust
fn view(state: &State, core: &CoreState) -> Element {
    file_tree_view(state).interactive(InteractiveId::FileTree)
}
```

## スタイリング

### セマンティックスタイルトークン

```rust
enum StyleToken {
    // コア定義
    BufferText,
    BufferPadding,
    StatusLine,
    StatusMode,
    MenuItemNormal,
    MenuItemSelected,
    Border,
    Shadow,
    // プラグイン定義
    Custom(String),
}
```

### テーマ

トークン → Face のマッピング。Kakoune の face をデフォルト値として使い、テーマ設定でオーバーライドする。

```toml
# config.toml
[theme]
"menu.selected" = { fg = "black", bg = "blue" }
"plugin.line_numbers.number" = { fg = "gray" }
```

## 段階的実装計画

### Phase 1: 宣言的 UI 基盤 (Kakoune 結合)

- Element 型の定義
- Flex レイアウト計算
- Overlay レイアウト (既存 compute_pos の統合)
- TEA イベントループへの移行 (既存 AppState::apply を活用)
- 既存レンダリングを Element ベースに書き換え
- Plugin trait の定義
- Slot メカニズムの実装

**コンパイラ駆動最適化に向けた設計上の考慮** (実装しないが意識する):
- Element の各 variant が「静的構造」と「動的内容」を分離できる設計にしておく
- view() 関数を純粋に保つ (副作用なし、`&State` のみ参照)。これが Phase 2 のコンパイル時解析の前提条件

### Phase 2: プラグイン基盤 + コンパイラ駆動最適化

- proc macro (`#[kasane::plugin]`, `#[kasane::component]`)
- Decorator / Replacement メカニズム
- Grid レイアウト
- InteractiveId によるマウスヒットテスト
- セマンティックスタイルトークンとテーマシステム

**`#[kasane::component]` の最適化段階** ([ADR-010](./decisions.md#adr-010-コンパイラ駆動最適化--svelte-的二層レンダリング)):
- 段階 1: 入力メモ化 — 全入力が前回と同じなら Element 構築をスキップ
- 段階 2: 静的レイアウトキャッシュ — 構造が入力に依存しない部分の layout を一度だけ計算
- 段階 3: 細粒度更新コード生成 — Element 単位の依存追跡により直接 CellGrid を更新 (二層レンダリングモデル)

各段階は計測に基づいて順次導入する。段階 1 が十分な場合、段階 2・3 は見送る。

### Phase 3: プロトコル分離

- kasane-core から Kakoune 固有コードを kasane-kakoune に分離
- 汎用 UI 基盤としての API 安定化
- プラグイン API のバージョニング
