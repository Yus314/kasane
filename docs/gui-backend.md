# GUI バックエンド実装計画書

本ドキュメントでは、Kasane の GUI バックエンド (`kasane-gui`) の技術選定・設計・実装計画を定義する。初期ターゲットプラットフォームは **Linux (Wayland / X11)** のみ。winit + wgpu がプラットフォームを抽象化するため macOS/Windows への拡張は将来容易だが、テスト環境がない段階での対応は行わない。

**関連ドキュメント:**

- [decisions.md](./decisions.md) — ADR-001 (TUI+GUI ハイブリッド) で GUI バックエンドの方針を決定済み
- [architecture.md](./architecture.md) — crate 構成図、バックエンドの責務表
- [roadmap.md](./roadmap.md) — Phase 4b として位置づけ
- [declarative-ui.md](./declarative-ui.md) — Element ツリー + TEA + プラグイン基盤

## 技術スタック

| ライブラリ | バージョン | 役割 |
|-----------|-----------|------|
| winit | 0.30 | ウィンドウ管理・入力イベント・IME |
| wgpu | 28 | GPU 描画 API (Vulkan/Metal/DX12/GL 抽象) |
| glyphon | 0.10 | テキストレンダリング (cosmic-text + swash + etagere アトラス) |
| softbuffer | 0.4 | CPU フォールバック用フレームバッファ (Phase G3 CPU フォールバック用・未実装) |
| tiny-skia | 0.11 | CPU フォールバック用 2D ラスタライザ (Phase G3 CPU フォールバック用・未実装) |
| arboard | 3 | クリップボード (workspace 既存依存) |

**選定根拠:** cosmic-term (COSMIC Desktop 公式ターミナル) が同一スタックを本番運用しており、モノスペースグリッド描画の実績がある。glyphon は cosmic-text のフォントシェーピング (rustybuzz) + swash ラスタライズ + etagere アトラスパッキングを wgpu パイプラインに統合する。Kasane のグリッドサイズ (最大 ~200x50 = 10,000 セル) は十分にパフォーマンス範囲内。

**不採用の選択肢:**

| 候補 | 不採用理由 |
|------|-----------|
| OpenGL (glutin + glow) | macOS が OpenGL を非推奨化。wgpu が内部で OpenGL ES バックエンドを持つ |
| Native API (Metal/Vulkan 直接) | プラットフォーム毎に個別レンダラーが必要。保守コストが倍増 |
| CPU のみ (softbuffer + tiny-skia) | 60fps スムーズスクロールの主パスとしては不足。フォールバックとして採用 |
| egui | イミディエイトモードが TEA リテインドモードと競合。モノスペースグリッドに非特化 |
| Vello (Linebender) | グリフキャッシュなし (毎フレームベクターパス描画)、API 不安定 (3-5ヶ月毎に破壊的変更)、compute shader 必須 |

## アーキテクチャ

### crate 構成

`kasane-gui` を新規クレートとして追加し、`kasane` バイナリからは feature flag `gui` で条件的に依存する。

```
kasane/
├── Cargo.toml                    # [workspace] — members に kasane-gui 追加
├── kasane-core/                  # 変更なし (共有コア)
├── kasane-tui/                   # 変更なし (TUI バックエンド)
├── kasane-macros/                # 変更なし (proc macro)
├── kasane-gui/                   # GUI バックエンド
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # pub run_gui() エントリポイント
│       ├── app.rs                # winit ApplicationHandler 実装
│       ├── backend.rs            # RenderBackend の GUI 実装
│       ├── input.rs              # winit WindowEvent → InputEvent 変換
│       ├── animation.rs          # スクロールアニメーション
│       ├── colors.rs             # カラーパレット解決
│       ├── gpu/
│       │   ├── mod.rs            # wgpu Device/Queue/Surface 初期化
│       │   ├── cell_renderer.rs  # セルグリッド描画 (背景+テキスト+カーソル)
│       │   ├── scene_renderer.rs # SceneRenderer — DrawCommand ベース描画
│       │   ├── metrics.rs        # フォントメトリクス・セル寸法計算
│       │   ├── bg_pipeline.rs    # 背景描画パイプライン
│       │   ├── border_pipeline.rs # ボーダー描画パイプライン
│       │   ├── bg.wgsl           # 背景シェーダー
│       │   └── rounded_rect.wgsl # 角丸矩形シェーダー
│       └── cpu/
│           └── mod.rs            # CPU フォールバック (未実装)
└── kasane/                       # メインバイナリ
    ├── Cargo.toml                # [features] gui = ["dep:kasane-gui"]
    └── src/
        ├── main.rs               # エントリポイント (run_tui/run_gui 分岐)
        ├── cli.rs                # CLI 引数パーサー
        └── process.rs            # Kakoune 子プロセス管理
```

**Cargo.toml (kasane バイナリ):**

```toml
[features]
default = []
gui = ["dep:kasane-gui"]

[dependencies]
kasane-core = { path = "../kasane-core" }
kasane-tui = { path = "../kasane-tui" }
kasane-gui = { path = "../kasane-gui", optional = true }
```

GUI 機能を含まないビルドでは wgpu のコンパイル時間を回避できる。`cargo build` はデフォルトで TUI のみ、`cargo build --features gui` で GUI を含む。

### イベントループ設計

**方式 C: run_tui/run_gui 分岐** を採用する。

winit の `run_app()` はメインスレッドを完全に占有するため、TUI の既存 `recv_timeout` ループとは共存できない。CLI 引数 `--ui gui` でイベントループ全体を切り替える。

```rust
// kasane/src/main.rs
fn main() -> Result<()> {
    let config = Config::load();
    let args = parse_args();
    match args.ui {
        UiMode::Tui => run_tui(config, args)?,    // 既存ロジック
        UiMode::Gui => run_gui(config, args)?,     // kasane-gui::run_gui()
    }
    Ok(())
}
```

**不採用:** 方式 B (`pump_events`) — macOS で動作しない (Cocoa/AppKit の制約。winit ドキュメントに "not supported on iOS, macOS, Web" と明記)。

**GUI 側スレッド構成:**

```
┌── winit イベントループ (メインスレッド) ──────────────────────┐
│                                                              │
│  ApplicationHandler::window_event()                          │
│    → KeyboardInput / CursorMoved / MouseInput / Resized      │
│    → InputEvent 変換 → update() → render                     │
│                                                              │
│  ApplicationHandler::user_event(KakouneEvent)                │
│    → Kakoune JSON-RPC メッセージ処理                           │
│    → update() → dirty フラグ設定                              │
│                                                              │
│  ApplicationHandler::about_to_wait()                         │
│    → pending events 一括処理                                  │
│    → スクロールアニメーションティック (16ms チェック)              │
│    → dirty なら request_redraw()                              │
│                                                              │
│  WindowEvent::RedrawRequested                                │
│    → view → place → paint → CellGrid → GPU/CPU 描画          │
│                                                              │
├── Kakoune Reader スレッド ────────────────────────────────────┤
│  loop {                                                      │
│    kak stdout → JSON-RPC パース                               │
│    → event_loop_proxy.send_event(KakouneEvent(req))          │
│  }                                                           │
│  ▲ EventLoopProxy が winit ループに UserEvent を注入           │
└──────────────────────────────────────────────────────────────┘
```

**ApplicationHandler の主要コールバック:**

```rust
impl ApplicationHandler<KakouneEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // ウィンドウ作成、wgpu 初期化
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, event: KakouneEvent) {
        self.pending_kakoune_events.push(event);
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        // pending Kakoune イベントを一括処理 (バッチング)
        for event in self.pending_kakoune_events.drain(..) {
            let (flags, cmds) = update(&mut self.state, event.into_msg(), ...);
            self.dirty |= flags;
            self.execute_commands(cmds);
        }
        // スクロールアニメーションティック
        if let Some(ref mut anim) = self.state.scroll_animation {
            if self.last_tick.elapsed() >= Duration::from_millis(16) {
                // ...
            }
        }
        if !self.dirty.is_empty() {
            self.window.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                self.render_frame();  // view → place → paint → GPU draw
                self.dirty = DirtyFlags::empty();
            }
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => { /* グリッドサイズ再計算 */ }
            WindowEvent::KeyboardInput { .. } => { /* InputEvent 変換 */ }
            WindowEvent::CursorMoved { .. } => { /* ピクセル→グリッド座標 */ }
            WindowEvent::MouseInput { .. } => { /* プレス/リリース */ }
            WindowEvent::MouseWheel { .. } => { /* スクロール */ }
            WindowEvent::Focused(f) => { /* FocusGained/Lost */ }
            WindowEvent::Ime(ime) => { /* IME 処理 */ }
            WindowEvent::DroppedFile(path) => { /* :edit {path} */ }
            WindowEvent::ScaleFactorChanged { .. } => { /* HiDPI 更新 */ }
            _ => {}
        }
    }
}
```

### レンダリングパイプライン

kasane-core のレンダリングパイプラインは TUI/GUI で完全に共有される。差分は最終描画のみ。

```
┌─────────────────────────────────────────────────────┐
│                  kasane-core (共有)                    │
│                                                       │
│  JSON-RPC → AppState.update() → view() → place()     │
│  → paint() → CellGrid → diff() → Vec<CellDiff>       │
└──────────────────────────┬────────────────────────────┘
                           │ diffs: &[CellDiff]
              ┌────────────┴────────────┐
              ▼                         ▼
   ┌──────────────────┐    ┌───────────────────────────┐
   │   kasane-tui       │    │      kasane-gui            │
   │                    │    │                             │
   │  crossterm 出力     │    │  ┌─────────────────────┐   │
   │  エスケープシーケンス │    │  │ GpuRenderer          │   │
   │                    │    │  │  wgpu + glyphon       │   │
   │                    │    │  ├─────────────────────┤   │
   │                    │    │  │ CpuRenderer          │   │
   │                    │    │  │  softbuffer + tiny-skia│  │
   │                    │    │  └─────────────────────┘   │
   └──────────────────┘    └───────────────────────────┘
```

**GPU 描画パス (1フレーム):**

1. **背景パス:** 全セルの `face.bg` を単色矩形として描画
2. **テキストパス:** グリフアトラスからテクスチャ付きクワッドをインスタンス描画
3. **カーソルパス:** `CursorStyle` に応じた GPU 矩形オーバーレイ
4. **オーバーレイパス:** フローティングウィンドウ背後のアルファシャドウ (Phase G3)

**差分更新:** GPU 側に永続的なセルグリッドバッファを保持し、`CellDiff` で変更されたセルのインスタンスデータのみを更新する。フルリドローを避け、帯域幅を節約する。

**セル描画アーキテクチャ:** Alacritty と同様のインスタンス・パー・セル方式。背景パス (単色クワッド) とフォアグラウンドパス (テクスチャクワッド) の二段構成。ワイド文字 (CJK/絵文字) は `Cell.width == 2` のセルを `2 * cell_width` で描画し、`Cell.width == 0` (継続セル) はスキップする。

### フォントとセル寸法の計算

glyphon の `FontSystem` を使用してフォントメトリクスを取得し、セル寸法を算出する。

```
cell_width  = "M" の advance width (フォントサイズから算出)
cell_height = ascent + descent + line_gap
cols = floor(window_width / cell_width)
rows = floor(window_height / cell_height)
```

**HiDPI 対応:** winit の `scale_factor` をフォントサイズとセルメトリクスに乗算し、物理ピクセルで描画する。`ScaleFactorChanged` イベントでグリフアトラスを再構築する。

## RenderBackend 拡張

> **注:** 実装では SceneRenderer + DrawCommand ベースのアーキテクチャを採用したため、以下の `draw_overlay()` 等の拡張メソッドは RenderBackend trait には追加されていない。GUI のレンダリングは DrawCommand 列を SceneRenderer が処理する方式となっている。以下は当初の設計案として記録を残す。

当初の設計案: `RenderBackend` trait (`kasane-core/src/render/mod.rs`) に以下のメソッドを追加する構想だった。

```rust
pub trait RenderBackend {
    // --- 既存メソッド (変更なし) ---
    fn size(&self) -> (u16, u16);
    fn begin_frame(&mut self) -> anyhow::Result<()> { Ok(()) }
    fn end_frame(&mut self) -> anyhow::Result<()> { Ok(()) }
    fn draw(&mut self, diffs: &[CellDiff]) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
    fn show_cursor(&mut self, x: u16, y: u16, style: CursorStyle) -> anyhow::Result<()>;
    fn hide_cursor(&mut self) -> anyhow::Result<()>;
    fn clipboard_get(&mut self) -> Option<String> { None }
    fn clipboard_set(&mut self, _text: &str) -> bool { false }

    // --- 新規メソッド ---

    /// フローティングウィンドウのオーバーレイ矩形を描画する。
    /// GUI: アルファシャドウの二重パス (影 → ウィンドウ背景)。
    /// TUI: デフォルト実装 (no-op、シャドウは CellGrid 内の半角ブロック文字で処理)。
    fn draw_overlay(&mut self, _overlays: &[OverlayRect]) -> anyhow::Result<()> {
        Ok(())
    }

    /// ディスプレイのスケールファクターを返す。
    /// GUI: winit の scale_factor。TUI: 1.0。
    fn scale_factor(&self) -> f64 {
        1.0
    }

    /// ウィンドウタイトルを設定する。
    /// GUI: winit Window::set_title()。TUI: デフォルト実装 (no-op)。
    fn set_title(&mut self, _title: &str) {}
}

/// オーバーレイ矩形 (シャドウ・カーソル等の GPU 描画用)。
#[derive(Debug, Clone)]
pub struct OverlayRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub kind: OverlayKind,
}

#[derive(Debug, Clone)]
pub enum OverlayKind {
    /// フローティングウィンドウの影 (アルファ値付き)
    Shadow { alpha: f32 },
    /// カーソルのセルオーバーレイ
    Cursor { style: CursorStyle },
}
```

**カーソル描画:** GUI ではターミナルネイティブカーソルが存在しないため、`CursorStyle` に応じた GPU 矩形をセルに重ねて描画する。

| CursorStyle | GUI 描画 |
|-------------|---------|
| `Block` | セル全体を塗りつぶし (fg/bg 反転) |
| `Bar` | セル左端の細い垂直線 |
| `Underline` | セル下端の水平線 |
| `Outline` | セル外枠のみ (非フォーカス時) |

**シャドウ描画:** `paint.rs` のシャドウ描画 (半角ブロック文字) は TUI 用にそのまま維持。GUI ではフローティングウィンドウの矩形情報を `place()` の結果から抽出し、`draw_overlay()` でアルファブレンドされた影矩形を描画する。

## 設定の拡張

`config.toml` に `[window]`、`[font]`、`[colors]` セクションを追加する。既存セクションとの整合性のためトップレベルに配置する (GUI ネスト不要)。

### config.toml 例

```toml
[window]
initial_cols = 80        # 初期ウィンドウ幅 (セル数)
initial_rows = 24        # 初期ウィンドウ高さ (セル数)

[font]
font_family = "JetBrains Mono"
font_size = 14.0
font_style = "Regular"                           # Regular / Bold / Italic
anti_aliasing = "grayscale"                      # grayscale / subpixel / none
font_fallback_list = ["Noto Sans CJK JP", "Noto Color Emoji"]
line_height = 1.2
letter_spacing = 0.0
cell_width_override = 0                          # 0 = 自動計算

[colors]
default_fg = "#d4d4d4"
default_bg = "#1e1e1e"
# 16 named colors (Kakoune デフォルト準拠)
black          = "#000000"
red            = "#cc0000"
green          = "#4e9a06"
yellow         = "#c4a000"
blue           = "#3465a4"
magenta        = "#75507b"
cyan           = "#06989a"
white          = "#d3d7cf"
bright_black   = "#555753"
bright_red     = "#ef2929"
bright_green   = "#8ae234"
bright_yellow  = "#fce94f"
bright_blue    = "#729fcf"
bright_magenta = "#ad7fa8"
bright_cyan    = "#34e2e2"
bright_white   = "#eeeeec"
```

### Config 構造体拡張

`kasane-core/src/config.rs` の `Config` に新セクションを追加する。

```rust
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub scroll: ScrollConfig,
    pub log: LogConfig,
    pub theme: ThemeConfig,
    pub menu: MenuConfig,
    pub search: SearchConfig,
    pub clipboard: ClipboardConfig,
    pub mouse: MouseConfig,
    pub window: WindowConfig,      // 新規
    pub font: FontConfig,          // 新規
    pub colors: ColorsConfig,      // 新規
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct WindowConfig {
    pub initial_cols: u16,
    pub initial_rows: u16,
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig {
            initial_cols: 80,
            initial_rows: 24,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct FontConfig {
    pub font_family: String,
    pub font_size: f32,
    pub font_style: String,
    pub anti_aliasing: String,
    pub font_fallback_list: Vec<String>,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub cell_width_override: u16,
}

impl Default for FontConfig {
    fn default() -> Self {
        FontConfig {
            font_family: "monospace".to_string(),
            font_size: 14.0,
            font_style: "Regular".to_string(),
            anti_aliasing: "grayscale".to_string(),
            font_fallback_list: Vec::new(),
            line_height: 1.2,
            letter_spacing: 0.0,
            cell_width_override: 0,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct ColorsConfig {
    pub default_fg: String,
    pub default_bg: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}
```

**`Color::Default` の解決:** GUI バックエンドでは描画時に `Color::Default` を `ColorsConfig` のパレットで解決する。TUI では従来どおりターミナルのデフォルト色 (`CtColor::Reset`) にマッピングする。`Color::Named(NamedColor::Red)` 等も GUI では `ColorsConfig` のパレット値に変換する。

## 実装フェーズ

### Phase G1 (MVP) — ✓ 完了 (commit 43acdc0)

セル描画 + キー入力 + リサイズ + HiDPI + カーソル + 設定 + CLI。
**目標:** GUI ウィンドウで Kakoune がキーボード操作可能になる。

### Phase G2 — ✓ 完了

マウス + クリップボード + VSync スムーズスクロール。
**目標:** TUI と同等の操作性を達成する。

### Phase G3 — ✓ 完了

ボーダー・シャドウの GPU 描画。
**目標:** GUI 固有の視覚品質を確保する。

## 各フェーズの詳細タスクリスト

### Phase G1: MVP

| タスク | 内容 | 対象ファイル |
|--------|------|-------------|
| G1-1 | kasane-gui クレート作成。Cargo.toml、feature flag 設定 | `kasane-gui/Cargo.toml`, `kasane/Cargo.toml`, `Cargo.toml` |
| G1-2 | wgpu Device/Queue/Surface 初期化、winit ApplicationHandler スケルトン | `kasane-gui/src/lib.rs`, `kasane-gui/src/backend.rs`, `kasane-gui/src/gpu/mod.rs` |
| G1-3 | glyphon FontSystem + セルメトリクス計算 (`font_family`, `font_size`, `scale_factor`) | `kasane-gui/src/gpu/cell_renderer.rs` |
| G1-4 | セルグリッド描画 (背景クワッド + グリフアトラステクスチャクワッド、CellDiff 差分更新) | `kasane-gui/src/gpu/cell_renderer.rs` |
| G1-5 | winit `KeyboardInput` → `InputEvent` 変換 | `kasane-gui/src/input.rs` |
| G1-6 | リサイズ処理 (Resized → グリッドサイズ再計算 → Kakoune に Resize 送信)。即時処理、デバウンスなし | `kasane-gui/src/backend.rs` |
| G1-7 | CLI `--ui gui` フラグ追加、`run_tui`/`run_gui` 分岐。Config 拡張 (`[window]`, `[font]`, `[colors]`) | `kasane/src/main.rs`, `kasane-core/src/config.rs` |

**G1-2 コードスケッチ (wgpu 初期化):**

```rust
// kasane-gui/src/gpu/mod.rs
pub struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl GpuState {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("no suitable GPU adapter"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await?;
        // ...
        Ok(GpuState { surface, device, queue, config })
    }
}
```

**G1-3 コードスケッチ (セルメトリクス):**

```rust
// kasane-gui/src/gpu/cell_renderer.rs
use glyphon::{FontSystem, Metrics};

pub struct CellMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    pub baseline: f32,
}

impl CellMetrics {
    pub fn from_font(font_system: &mut FontSystem, font_size: f32, scale: f64) -> Self {
        let scaled_size = font_size * scale as f32;
        let metrics = Metrics::new(scaled_size, scaled_size * 1.2);
        // "M" の advance width でセル幅を算出
        // ascent + descent + line_gap でセル高さを算出
        CellMetrics {
            cell_width: metrics.font_size * 0.6,  // 概算、実測値で置換
            cell_height: metrics.line_height,
            baseline: metrics.font_size * 0.8,
        }
    }
}
```

**G1-5 入力マッピング表:**

| winit WindowEvent | kasane InputEvent |
|-------------------|-------------------|
| `KeyboardInput` + `logical_key` | `Key(KeyEvent)` |
| `Resized(PhysicalSize)` | `Resize(cols, rows)` — セル数に変換 |
| `Focused(true)` | `FocusGained` |
| `Focused(false)` | `FocusLost` |
| `CloseRequested` | → `Command::Quit` |

### Phase G2: 入力拡張

| タスク | 内容 | 対象ファイル |
|--------|------|-------------|
| G2-1 | マウス入力: `CursorMoved` ピクセル→グリッド座標変換、`MouseInput` プレス/リリース、`MouseWheel` スクロール | `kasane-gui/src/input.rs` |
| G2-2 | クリップボード: arboard (workspace 既存依存) で `clipboard_get`/`clipboard_set` 実装 | `kasane-gui/src/backend.rs` |
| G2-3 | IME: `Ime::Commit` → `KeyEvent` 変換 (プリエディット表示は将来) | `kasane-gui/src/input.rs` |
| G2-4 | VSync 駆動スムーズスクロール (`about_to_wait` で 16ms チェック → `request_redraw`) | `kasane-gui/src/backend.rs` |

**G2-1 座標変換:**

```rust
fn pixel_to_grid(px: f64, py: f64, cell_width: f32, cell_height: f32) -> (u16, u16) {
    let col = (px as f32 / cell_width).floor() as u16;
    let row = (py as f32 / cell_height).floor() as u16;
    (col, row)
}
```

**G2-3 IME 処理:**

```rust
WindowEvent::Ime(ime) => match ime {
    Ime::Commit(text) => {
        // 各文字を個別の KeyEvent に変換して Kakoune に送信
        for ch in text.chars() {
            let key = InputEvent::Key(KeyEvent::Char(ch));
            // → update() → Command::SendToKakoune
        }
    }
    Ime::Preedit(_, _) => {
        // Phase G2 では無視。将来: インライン表示
    }
    _ => {}
}
```

**G2-4 VSync スムーズスクロール:**

アニメーション中は `about_to_wait()` で毎フレーム `request_redraw()` を呼ぶ。`RedrawRequested` で描画するため、モニターのリフレッシュレートに自動同期される。TUI の `recv_timeout(16ms)` と同等の効果をより自然に実現する。

### Phase G3: 視覚品質 + 堅牢性

| タスク | 内容 | 対象ファイル |
|--------|------|-------------|
| G3-1 | GPU アルファシャドウ: `RenderBackend::draw_overlay()` 追加、`place()` 結果からフローティングウィンドウ矩形を抽出、アルファブレンド影矩形 | `kasane-core/src/render/mod.rs`, `kasane-gui/src/backend.rs`, `kasane-gui/src/gpu/cell_renderer.rs` |
| G3-2 | CPU フォールバック: softbuffer + tiny-skia レンダラー。wgpu アダプター取得失敗時に自動切り替え | `kasane-gui/src/cpu/mod.rs`, `kasane-gui/src/cpu/cell_renderer.rs` |
| G3-3 | ファイル D&D: `WindowEvent::DroppedFile(path)` → `:edit {path}` コマンドを Kakoune に送信 | `kasane-gui/src/input.rs` |

**G3-1 シャドウ描画:**

```rust
// GUI の draw_overlay 実装
fn draw_overlay(&mut self, overlays: &[OverlayRect]) -> anyhow::Result<()> {
    for overlay in overlays {
        match &overlay.kind {
            OverlayKind::Shadow { alpha } => {
                // アルファブレンド矩形をレンダーパスに追加
                self.gpu.draw_alpha_rect(
                    overlay.x, overlay.y,
                    overlay.width, overlay.height,
                    [0.0, 0.0, 0.0, *alpha],
                );
            }
            OverlayKind::Cursor { style } => {
                // カーソル矩形を描画
            }
        }
    }
    Ok(())
}
```

**G3-2 CPU フォールバックアーキテクチャ:**

```rust
// kasane-gui/src/backend.rs
enum Renderer {
    Gpu(GpuRenderer),
    Cpu(CpuRenderer),
}

impl GuiBackend {
    pub fn new(window: Arc<Window>, config: &Config) -> anyhow::Result<Self> {
        let renderer = match GpuRenderer::new(window.clone(), config) {
            Ok(gpu) => Renderer::Gpu(gpu),
            Err(e) => {
                tracing::warn!("GPU init failed ({e}), falling back to CPU renderer");
                Renderer::Cpu(CpuRenderer::new(window, config)?)
            }
        };
        Ok(GuiBackend { renderer, ... })
    }
}
```

**CPU フォールバックが必要な環境:**

- SSH リモート表示 (GPU なし)
- VM/コンテナ (Vulkan/Metal なし)
- CI/CD 自動テスト (ヘッドレス)
- wgpu アダプター取得失敗時の自動フォールバック

## テスト戦略

### ユニットテスト

| テスト対象 | 内容 |
|-----------|------|
| 入力変換 | winit `KeyboardInput` → kasane `InputEvent` のマッピング正確性 |
| 座標変換 | ピクセル座標→グリッド座標の計算 (境界値、HiDPI 倍率含む) |
| 設定パース | `[window]`, `[font]`, `[colors]` の TOML デシリアライゼーション |
| カラーパレット解決 | `Color::Default` → `ColorsConfig` の RGB 変換 |
| セルメトリクス | フォントサイズ + scale_factor → cell_width/cell_height |

### 統合テスト

- kasane-core の既存テスト (305件) が `--features gui` ビルドでも全パスすることを確認
- `Config` 拡張が既存設定ファイルとの後方互換性を維持することを確認

### 手動テストチェックリスト

- [ ] セルグリッド描画: ASCII、CJK、絵文字、box-drawing 文字
- [ ] カーソルスタイル: Block, Bar, Underline, Outline の各表示
- [ ] リサイズ: ドラッグ中の連続リサイズ
- [ ] HiDPI: 異なる scale_factor でのフォント品質
- [ ] マウス座標: クリック位置が正しいグリッドセルに対応
- [ ] IME: 日本語入力 (fcitx5/ibus) でのコミット
- [ ] スムーズスクロール: VSync 同期の滑らかさ
- [ ] シャドウ: フローティングウィンドウのアルファシャドウ
- [ ] CPU フォールバック: `LIBGL_ALWAYS_SOFTWARE=1` 等での動作
- [ ] ファイル D&D: ファイルマネージャからのドロップで `:edit`

## Nix flake / ビルド設定の変更

### flake.nix

```nix
# devShell の buildInputs に追加
buildInputs = [
  # 既存 ...
  # GUI 依存 (--features gui 用)
  vulkan-loader
  vulkan-validation-layers
  wayland
  wayland-protocols
  libxkbcommon
  xorg.libX11
  xorg.libXcursor
  xorg.libXrandr
  xorg.libXi
  fontconfig
  freetype
];

# LD_LIBRARY_PATH に追加
LD_LIBRARY_PATH = lib.makeLibraryPath [
  vulkan-loader
  wayland
  libxkbcommon
];
```

### CI (GitHub Actions)

```yaml
# 既存の cargo test ステップに加えて
- name: Build with GUI feature
  run: cargo build --features gui
  # GPU ランタイムテストは不可。コンパイル確認のみ。
```

## 未来の拡張パス

以下は Phase G1-G3 の範囲外だが、アーキテクチャ設計時に考慮する拡張項目。

| 拡張 | 概要 |
|------|------|
| GPU ボーダー | CellGrid のボーダー文字を検出し、GPU 矩形/円弧でオーバードロー (サブピクセル品質) |
| プリエディット表示 | IME の合成文字列をインライン表示 (`Ime::Preedit`) |
| マルチウィンドウ | Kakoune `:new` で新しい kasane-gui インスタンスを起動 (custom `windowing_module`) |
| アンダーラインバリエーション | 波線、点線、破線アンダーラインを GPU で描画 |
| 領域別フォントサイズ | UI 領域ごとに異なるフォントサイズ |
| カーソルブリンク | タイマー駆動のカーソル表示/非表示トグル |
| リッチフローティングウィンドウ装飾 | 角丸、グラデーション、ブラーエフェクト |
