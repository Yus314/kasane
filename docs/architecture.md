# アーキテクチャ設計書

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
│  │  セルグリッド管理     │ │  glyphon)                │   │
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
│           │ TUI: stdout         ▼ TUI: stdin              │
│           │ GUI: winit + GPU      GUI: winit               │
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
│       ├── lib.rs              # クレートルート
│       ├── element.rs          # Element ツリー型定義 (宣言的 UI の中核)
│       ├── plugin.rs           # Plugin trait、PluginRegistry、Slot/Decorator/Replacement
│       ├── input.rs            # 入力イベント → Kakoune キー変換
│       ├── config.rs           # TOML 設定パーサー (ThemeConfig, MenuConfig, SearchConfig 含む)
│       ├── io.rs               # I/O ユーティリティ
│       ├── perf.rs             # パフォーマンス計測ユーティリティ
│       ├── bin/
│       │   └── alloc_budget.rs # アロケーション予算分析ツール
│       ├── protocol/           # JSON-RPC パーサー
│       │   ├── mod.rs          # モジュールルート
│       │   ├── color.rs        # カラー解析
│       │   ├── message.rs      # メッセージ型定義
│       │   ├── parse.rs        # JSON-RPC パース実装
│       │   └── tests.rs        # テスト
│       ├── test_utils.rs       # テストユーティリティ (create_test_state 等)
│       ├── state/              # アプリケーション状態管理 (TEA: State + Msg + update)
│       │   ├── mod.rs          # AppState、CoreState 定義
│       │   ├── apply.rs        # Kakoune メッセージの適用
│       │   ├── update.rs       # TEA update 関数、Msg/DirtyFlags 定義
│       │   ├── info.rs         # InfoState、InfoIdentity (info ポップアップ状態)
│       │   ├── menu.rs         # MenuState、MenuParams (メニュー状態)
│       │   └── tests.rs        # テスト
│       ├── layout/             # レイアウトエンジン (Flex + Grid + Overlay)
│       │   ├── mod.rs          # 共通型 (Rect, Size, Constraints, MenuPlacement 等)
│       │   ├── flex.rs         # Flexbox レイアウト計算 (measure + place)
│       │   ├── grid.rs         # Grid レイアウト計算 (measure_grid + place_grid)
│       │   ├── position.rs     # Overlay 位置計算 (compute_pos, layout_menu_inline)
│       │   ├── info.rs         # Info ポップアップ配置 (layout_info, avoid リスト)
│       │   ├── hit_test.rs     # InteractiveId マウスヒットテスト (Z-order 逆順走査)
│       │   ├── text.rs         # テキスト幅計算
│       │   └── word_wrap.rs    # 単語折返しレイアウト
│       └── render/             # レンダリングエンジン
│           ├── mod.rs          # RenderBackend trait
│           ├── grid.rs         # CellGrid — セルの二次元配列、差分描画
│           ├── paint.rs        # paint() — Element + LayoutResult → CellGrid 描画
│           ├── patch.rs        # PaintPatch trait + 組み込みパッチ (StatusBar/MenuSelection/Cursor)
│           ├── scene.rs        # DrawCommand — GUI シーンベース描画用コマンド
│           ├── theme.rs        # Theme (StyleToken → Face マッピング、face spec パーサー)
│           ├── markup.rs       # マークアップパーサー ({face_spec}text{default})
│           ├── test_helpers/   # テストヘルパー
│           │   ├── mod.rs      # 共通テストヘルパー
│           │   └── info.rs     # Info ポップアップ用テストヘルパー
│           ├── tests.rs        # テスト
│           ├── menu.rs         # メニュー描画
│           └── view/           # view() — Element ツリー構築
│               ├── mod.rs      # view() 関数、build_* 関数群
│               ├── info.rs     # Info ポップアップの Element 構築
│               ├── menu.rs     # メニューの Element 構築
│               └── tests.rs    # テスト
├── kasane-tui/                 # crossterm ベースの TUI バックエンド
│   └── src/
│       ├── lib.rs              # クレートルート
│       ├── backend.rs          # RenderBackend の TUI 実装 (CellGrid → crossterm 出力)
│       └── input.rs            # crossterm イベント変換
├── kasane-macros/              # proc macro (#[kasane::plugin], #[kasane::component])
│   └── src/
│       ├── lib.rs              # proc macro エントリポイント
│       ├── plugin.rs           # #[kasane_plugin] — Plugin trait 実装の自動生成
│       └── component.rs        # #[kasane_component] — deps() アノテーション、AST フィールドアクセス解析、allow() エスケープハッチ、FIELD_FLAG_MAP 検証
├── kasane-gui/                 # GPU バックエンド (winit + wgpu + glyphon)
│   └── src/
│       ├── lib.rs              # クレートルート、run_gui() エントリポイント
│       ├── app.rs              # winit ApplicationHandler 実装
│       ├── backend.rs          # RenderBackend の GUI 実装
│       ├── input.rs            # winit イベント → InputEvent 変換
│       ├── animation.rs        # スクロールアニメーション
│       ├── colors.rs           # カラーパレット解決 (ColorsConfig → wgpu Color)
│       ├── gpu/                # wgpu + glyphon GPU レンダリング
│       │   ├── mod.rs          # wgpu Device/Queue/Surface 初期化
│       │   ├── cell_renderer.rs # セルグリッド描画 (背景+テキスト+カーソル)
│       │   ├── scene_renderer.rs # SceneRenderer — DrawCommand ベース描画
│       │   ├── metrics.rs      # フォントメトリクス・セル寸法計算
│       │   ├── bg_pipeline.rs  # 背景描画パイプライン
│       │   ├── border_pipeline.rs # ボーダー描画パイプライン
│       │   ├── bg.wgsl         # 背景シェーダー
│       │   └── rounded_rect.wgsl # 角丸矩形シェーダー
│       └── cpu/
│           └── mod.rs          # CPU フォールバック (未実装)
└── kasane/                     # メインバイナリ + ライブラリ (CLI パース、バックエンド選択)
    └── src/
        ├── lib.rs              # kasane::run() エントリポイント (外部プラグインバイナリ用)
        ├── main.rs             # デフォルトバイナリ (kasane::run(|_| {}) を呼ぶのみ)
        ├── cli.rs              # CLI 引数パーサー
        └── process.rs          # Kakoune 子プロセス管理
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

### 三層レイヤー責務モデル

機能の責務境界を以下の三層で分類する。詳細は [layer-responsibilities.md](./layer-responsibilities.md) を参照。

| 層 | 定義 | 判断基準 |
|---|---|---|
| 上流 (Kakoune) | プロトコルレベルの関心事 | プロトコル変更が必要か？ |
| コア (kasane-core) | プロトコルの忠実なレンダリング + フロントエンドネイティブ能力 | 唯一の正しい実装が存在するか？ |
| プラグイン | ポリシーが分かれうる機能 (バンドル WASM / FS 発見 WASM / ネイティブ) | 上記以外 |

### 宣言的 UI レイヤーの責務

kasane-core は宣言的 UI レイヤー (ADR-009) を実装済み。詳細は [plugin-development.md](./plugin-development.md) を参照。

| コンポーネント | 担当 | 説明 |
|--------------|------|------|
| Element ツリー構築 | kasane-core | view(&State) → Element。プラグインの Slot/Decorator/Replacement を合成 |
| レイアウト計算 | kasane-core | Flex + Overlay。Element ツリーからセル座標を計算 |
| paint() (TUI) | kasane-core | Element + LayoutResult → CellGrid への描画 |
| scene_paint() (GUI) | kasane-core | Element + LayoutResult → DrawCommand 列への変換 |
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
| CellGrid → 端末出力 | — | crossterm | — (シーンベース描画) |
| DrawCommand → GPU 描画 | — | — | wgpu + glyphon |
| ボーダー描画 | — | 罫線文字 | GPU 描画 (角丸/シャドウ) |
| キー入力取得 | — | crossterm | winit |
| マウス入力取得 | — | crossterm | winit |
| クリップボード | — | arboard | arboard (ネイティブ) |
| IME | — | ターミナル経由 | winit + 自前 |
| D&D | — | 不可 | winit |

**レンダリングパスの違い:**

- **TUI パス:** `view_cached() → place() → paint() → CellGrid → grid.diff() → backend.draw()`
- **GUI パス:** `view_sections_cached() → scene_paint_section() → SceneCache → SceneRenderer (GPU)`

TUI はセルグリッドベースの差分描画を行い、crossterm でエスケープシーケンスに変換する。GUI は Element ツリーから DrawCommand (シーン記述) を生成し、SceneRenderer が wgpu + glyphon で直接 GPU 描画する。

**キャッシュレイヤー (ADR-010):**

| レイヤー | 対象 | 説明 |
|---------|------|------|
| ViewCache | Element ツリー | DirtyFlags に基づくセクション別 (base/menu/info) Element メモ化。ComponentCache\<T\> による汎用キャッシュ |
| LayoutCache | レイアウト結果 | base_layout, status_row, root_area のキャッシュ。セクション別再描画に使用 |
| SceneCache | DrawCommand 列 | GUI 用セクション別 DrawCommand キャッシュ。ViewCache と同じ無効化ルール |
| PaintPatch | CellGrid 部分更新 | コンパイル済みパッチによる高速パス (StatusBarPatch, MenuSelectionPatch, CursorPatch) |

**TUI 高速パス (render_pipeline_patched):**

```
DirtyFlags チェック
├── PaintPatch 適用可能 → パッチ (2〜80 セル更新)
├── セクション別再描画可能 → セクション単位 repaint
└── フォールバック → フルパイプライン (view → place → paint → diff)
```

## レイアウト計算の詳細

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
