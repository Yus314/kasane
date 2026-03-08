# パフォーマンス分析

本ドキュメントでは、kasane の宣言的 UI アーキテクチャにおけるパフォーマンス特性を分析する。
ボトルネックの特定、計測結果、対策を記述する。

## フレーム実行フロー

```
イベント受信
  │
  ▼
イベントバッチ処理 (try_recv で pending を全て消化)
  │
  ▼
state.apply() ── O(メッセージサイズ)
  │
  ▼
view(&state, &registry)     ── Element ツリー構築
  │
  ▼
flex::place(&element, area)  ── レイアウト計算
  │
  ▼
grid.clear() + paint()       ── CellGrid への描画
  │
  ▼
grid.diff()                  ── O(w×h) 差分検出
  │
  ▼
backend.draw(&diffs)         ── O(changed_cells) ← 端末 I/O (最も遅い)
backend.flush()
  │
  ▼
grid.swap()                  ── O(w×h) バッファ交換
```

## フレームあたりコスト (80×24 ターミナル)

`cargo bench --bench rendering_pipeline` による criterion 計測結果。

| 処理 | 計算量 | 計測値 | 備考 |
|---|---|---|---|
| `view()` | O(ノード数) | 0.26 μs (0 plugins) / 2.35 μs (10 plugins) | Element ツリー構築 |
| `flex::place()` | O(ノード数) | 0.38 μs | レイアウト計算 |
| `grid.clear()` + `paint()` | O(w×h) | 20.3 μs | Atom→セル変換、unicode-width 計算 |
| `grid.diff()` (incremental) | O(w×h) = 1,920 セル比較 | 11.0 μs | Cell 同士の等価比較 |
| `grid.diff()` (full redraw) | O(w×h) | 24.1 μs | 初回フレームのみ。全セルを CellDiff に構築 |
| `grid.swap()` | O(w×h) | ~5 μs | swap は O(1)、clear が O(w×h) |
| **CPU パイプライン合計** | | **~40 μs** | `full_frame` ベンチマーク実測値 |
| `backend.draw()` | O(changed_cells) | **100-3,000 μs** | エスケープシーケンス生成 + I/O |
| `backend.flush()` | O(1) | **50-500 μs** | stdout への write |

**支配的コスト: 端末 I/O (`backend.draw()` + `backend.flush()`)。**
CPU パイプラインの合計は ~40 μs で、16 ms フレームバジェットの **0.25%**。

## 既存の最適化

| 最適化 | 対象 | 効果 |
|---|---|---|
| CompactString | Cell.grapheme | 短い文字列 (24B 以下) のヒープアロケーション回避 |
| bitflags Attributes | Face.attributes | Vec\<Attribute\> → u16。Copy 型化によるアロケーション排除 |
| ダブルバッファリング | CellGrid | `std::mem::swap()` でポインタ交換。O(1) |
| 差分描画 | CellGrid.diff() | 変更セルのみ端末に送信。I/O 量を最小化 |
| イベントバッチング | main.rs | `try_recv()` で pending イベントを全て消化してから 1 回描画 |
| SIMD JSON | protocol.rs | simd_json による高速 JSON パース |

## 宣言的パイプラインのオーバーヘッド

旧命令的パイプライン (`render_frame()` が直接 CellGrid に描画) と比較した、宣言的パイプライン (`view() → layout() → paint()`) の追加コスト。

### フレーム時間内訳 (full_frame ≈ 40 μs)

```
view()  構築:     0.26 μs ━          (0.6%)
flex    layout:   0.38 μs ━          (1.0%)
paint   80x24:   20.3  μs ━━━━━━━━━━━━━━━━━━━━━━━━━━  (50.7%)  ← 支配的
grid    diff:   ~13    μs ━━━━━━━━━━━━━━━━━  (加重平均, ~33%)
grid    swap:    ~5    μs ━━━━━━━  (~13%)
plugin他:        ~1    μs ━
─────────────────────────
合計             ~40   μs
```

### 各フェーズの計測値

#### 1. Element ツリー構築: view()

| 条件 | 計測値 | 備考 |
|---|---|---|
| プラグイン 0 個 | **0.26 μs** | コア UI (~20-30 ノード) |
| プラグイン 10 個 | **2.35 μs** | 各プラグインが StatusRight に contribute |

プラグイン 0 個のコア UI 構築コストは 260 ns で、事前見積もり (~1 μs) を大幅に下回る。プラグイン追加時のスケーリングはほぼ線形。

#### 2. レイアウト計算: layout()

| 条件 | 計測値 |
|---|---|
| 標準 80x24 (プラグイン 0) | **0.38 μs** |

事前見積もり (~1 μs) の約 1/3。Flex レイアウトの measure/place 2 パスは極めて軽量。

#### 3. 描画: paint()

| 条件 | 計測値 | セル数 | per-cell |
|---|---|---|---|
| 80×24 | **20.3 μs** | 1,920 | 10.6 ns |
| 200×60 | **85.8 μs** | 12,000 | 7.1 ns |

面積に対してほぼ線形にスケール。per-cell コストが大画面で下がるのはキャッシュ効率の改善による。paint が CPU パイプラインの約 50% を占めるが、これは旧 `render_buffer()` + `render_status()` と同等のコスト (Atom→Cell 変換 + unicode-width 計算)。

#### 4. Plugin dispatch

| プラグイン数 | collect_slot (8 Slot) + apply_decorator | 計測値 |
|---|---|---|
| 1 | 8 回の vtable 呼び出し + sort + fold | **0.21 μs** |
| 5 | 40 回 + sort + fold | **0.86 μs** |
| 10 | 80 回 + sort + fold | **1.85 μs** |

スケーリングはほぼ線形 (~185 ns/plugin)。

#### 5. Decorator チェーン

| プラグイン数 | 計測値 | 備考 |
|---|---|---|
| 1 | **26 ns** | sort + 1 回の fold |
| 5 | **66 ns** | sort + 5 回の fold |
| 10 | **117 ns** | sort + 10 回の fold |

事前見積もり (~500 ns for 5 plugins) を大幅に下回る。no-op decorator のため実際のプラグインでは若干増えるが、μs 未満に収まる。

### 宣言的パイプラインの純追加コスト

| 処理 | 事前見積もり | 計測値 |
|---|---|---|
| Element 構築 (view) | ~1-4 μs | 0.26-2.35 μs |
| レイアウト計算 (layout) | ~1 μs | 0.38 μs |
| 再帰走査 (paint overhead) | ~0.3 μs | 計測不可 (paint 内に含まれる) |
| Plugin dispatch | ~1 μs | 1.85 μs (10 plugins) |
| Decorator チェーン | ~0.5 μs | 0.12 μs (10 plugins) |
| **合計** | **~4-7 μs** | **~3 μs** (0 plugins) / **~5 μs** (10 plugins) |

**CPU パイプライン全体 (40 μs) の 7-12%。端末 I/O (200-3,600 μs) の 0.1-2.5%。**
**実用上の影響はない。**

## コンパイラ駆動最適化 (Phase 2)

[ADR-010](./decisions.md#adr-010-コンパイラ駆動最適化--svelte-的二層レンダリング) の二層レンダリングモデルによる追加のコスト削減見積もり。

### 二層レンダリングによるコスト削減

`#[kasane::component]` のコンパイル済みパスにより、Element ツリー構築・レイアウト計算・再帰走査をスキップできる:

| 処理 | インタープリタパス | コンパイル済みパス | 削減 |
|---|---|---|---|
| Element 構築 (view) | ~1-4 μs | 0 μs (スキップ) | -1-4 μs |
| レイアウト計算 (layout) | ~1 μs | 0 μs (キャッシュ済み) | -1 μs |
| 再帰走査 (paint) | ~0.3 μs | 0 μs (直接更新) | -0.3 μs |
| CellGrid 更新 | ~30 μs (全ノード) | ~1-3 μs (変更セルのみ) | -27 μs |
| **合計** | **~35 μs** | **~1-3 μs** | **~32 μs** |

### 静的レイアウトキャッシュのコスト

静的構造のレイアウトキャッシュには以下のコストが伴う:

| コスト | 量 | 頻度 |
|---|---|---|
| キャッシュ保持メモリ | LayoutResult (~50B/ノード × 30 ノード = ~1.5 KB) | 常時 |
| キャッシュ無効化チェック | リサイズ判定 (u16 比較 × 2) | 毎フレーム |
| キャッシュ再構築 | 通常の layout() と同等 (~1 μs) | リサイズ時のみ |

メモリコストは無視できる水準。無効化チェックも定数時間。

### 段階的導入の条件

パフォーマンス原則「計測してから最適化」に従い、以下の条件を満たした場合に各段階を導入する:

- **段階 1 (入力メモ化)**: proc macro 基盤の構築と同時に導入。コスト低・効果確実
- **段階 2 (静的レイアウトキャッシュ)**: layout() が現在 0.38 μs のため、閾値 5 μs まで 13 倍の余裕がある。プラグイン追加により layout() が 5 μs を超えた場合に導入
- **段階 3 (細粒度更新コード生成)**: paint() が現在 20 μs (80x24)。200x60 で 86 μs。端末 I/O の 10% (15-360 μs) を超えるのは大画面時のみで、現時点では不要

上記のコスト削減見積もりはいずれも端末 I/O (200-3,600 μs) に対して小さい。Phase 2 では段階 1 を必ず実装し、段階 2・3 は計測結果に基づいて判断する。

## 注意が必要なボトルネック

### 深刻度: 高 → 対策済み

#### バッファ行の clone

Element が所有型 (Owned) のため、バッファの全行をツリーに含めると毎フレーム clone が発生する問題。

```
50 行 × 5 Atom/行 = 250 Atom
各 Atom: Face(16B) + CompactString(24B) = 40B
合計: ~10 KB + 250 回の CompactString clone
推定コスト: 10-30 μs
```

**対策: BufferRef パターン (実装済み)**

`Element::BufferRef { line_range }` により clone コストはゼロ。paint 時に `&AppState` から直接描画する。ベンチマークで view() が 0.26 μs (0 plugins) に収まっていることが、BufferRef の有効性を裏付ける。

### 深刻度: 中

#### BufferLine Decorator の乗算的コスト

`DecorateTarget::BufferLine` に複数の Decorator が登録された場合:

```
N 個の BufferLine Decorator × M 行 = N × M 回の関数呼び出し

例: 3 Decorator (行番号, git マーク, ブレークポイント) × 50 行
  = 150 回の Decorator 呼び出し
  = 150 ノード追加
  ≈ 5-10 μs 追加
```

単独では問題にならないが、Decorator 数と行数の積で増加する。

**対策:**

1. **Buffer 全体の Decorator を推奨:** 行ごとではなく列として追加する設計をガイドラインとして推奨

```rust
// 推奨: Buffer 全体に対する Decorator
#[decorate(DecorateTarget::Buffer)]
fn decorate(buffer: Element, state: &State, core: &CoreState) -> Element {
    flex(Row, [
        child(line_number_column(core), flex: 0.0),
        child(buffer, flex: 1.0),
    ])
}

// 非推奨: 行ごとの Decorator
#[decorate(DecorateTarget::BufferLine)]
fn decorate_line(line: Element, line_num: usize, ...) -> Element { ... }
```

2. **BufferLine Decorator の上限設定:** フレームワークが警告を出す閾値を設定

#### 大量ノードの Element ツリー

仮想化なしでプラグインが全データを Element 化する場合:

```
1,000 エントリのファイルツリー → 1,000 ノードの Element
構築: ~30 μs
レイアウト: ~20 μs
合計: ~50 μs (端末 I/O の 2-25%)
```

現実的にはまだ許容範囲だが、10,000 ノードになると数百マイクロ秒に達し、端末 I/O と同オーダーになる。

**対策: VirtualList (将来)**

```rust
enum Element {
    /// 仮想化リスト: 表示範囲のみ Element を生成
    VirtualList {
        item_count: usize,
        item_height: u16,           // 各アイテムの高さ (固定)
        scroll_offset: usize,
        render_item: Box<dyn Fn(usize) -> Element>,
    },
}
```

Phase 1 では不要。問題が顕在化してから導入する。

### 深刻度: 低

#### word_wrap の Vec アロケーション

`render_wrapped_line()` は呼び出しごとに 2 つの Vec を生成する:

```rust
fn collect_metrics(line: &[Atom]) -> Vec<(u16, bool)>  // グラフェムごとのメトリクス
fn word_wrap_segments(metrics: &[(u16, bool)], max_width: u16) -> Vec<WrapSegment>
```

info ポップアップの表示時に行数分呼ばれる。

**対策:** 再利用可能なバッファを `paint()` のコンテキストに持たせる:

```rust
struct PaintContext {
    metrics_buf: Vec<(u16, bool)>,
    segments_buf: Vec<WrapSegment>,
}
```

Phase 1 では不要。word_wrap は info 表示時のみ呼ばれ、頻度が低い。

## ベンチマーク結果

`kasane-core/benches/rendering_pipeline.rs` に 9 種の criterion ベンチマークを実装済み。
CI (`.github/workflows/bench.yml`) で自動回帰検出を行う (15% 超の劣化で PR にコメント)。

### 実行方法

```sh
cargo bench --bench rendering_pipeline           # 全ベンチマーク実行
cargo bench --bench rendering_pipeline -- "paint" # 特定ベンチのみ
```

HTML レポート: `target/criterion/*/report/index.html`

### マイクロベンチマーク (6 種)

| ベンチマーク | 測定対象 | 目標 | 計測値 | 判定 |
|---|---|---|---|---|
| `element_construct/plugins_0` | view() ツリー構築 (0 plugins) | < 10 μs | 0.26 μs | OK (38x 余裕) |
| `element_construct/plugins_10` | view() ツリー構築 (10 plugins) | < 10 μs | 2.35 μs | OK (4x 余裕) |
| `flex_layout` | place() レイアウト計算 | < 5 μs | 0.38 μs | OK (13x 余裕) |
| `paint/80x24` | clear() + paint() | — | 20.3 μs | 旧パイプライン同等 |
| `paint/200x60` | clear() + paint() (大画面) | — | 85.8 μs | 面積比で線形 |
| `grid_diff/full_redraw` | diff() 初回フレーム | < 10 μs | 24.1 μs | 超過 (注1) |
| `grid_diff/incremental` | diff() 差分なし | < 10 μs | 11.0 μs | 微超過 (注1) |
| `decorator_chain/plugins_10` | apply_decorator (10 段) | < 1 μs | 0.12 μs | OK (8.5x 余裕) |
| `plugin_dispatch/plugins_10` | 全 8 Slot collect + decorator | < 5 μs | 1.85 μs | OK (2.7x 余裕) |

**注1**: `grid_diff` は事前見積もり (~5 μs) の 2-5 倍。Cell の比較コスト (`CompactString(24B) + Face(16B) + u8`) が見積もりより高い。ただし full_redraw は初回フレームのみで、通常フレームは incremental パス。CPU パイプライン全体 (40 μs) の中では 28% を占めるが、16 ms バジェットに対しては無視可能。

### 統合ベンチマーク (3 種)

| ベンチマーク | 測定対象 | 目標 | 計測値 | 判定 |
|---|---|---|---|---|
| `full_frame` | view → layout → paint → diff → swap | < 16 ms | 40 μs | OK (400x 余裕) |
| `draw_message` | state.apply(Draw) + full frame | < 5 ms | 46 μs | OK (109x 余裕) |
| `menu_show/items_10` | menu 表示 + full frame | < 5 ms | 45 μs | OK |
| `menu_show/items_50` | menu 50 items + full frame | < 5 ms | 45 μs | OK |
| `menu_show/items_100` | menu 100 items + full frame | < 5 ms | 46 μs | OK |

`menu_show` がアイテム数にほぼ依存しないのは `menu_max_height=10` の制約で表示行数が一定のため。

### 拡張ベンチマーク (20 種)

| ベンチマーク | 測定対象 | 計測値 | 備考 |
|---|---|---|---|
| `parse_request/draw_lines/10` | JSON-RPC パース (10 行 draw) | — | 初回計測後に記入 |
| `parse_request/draw_lines/100` | JSON-RPC パース (100 行 draw) | — | |
| `parse_request/draw_lines/500` | JSON-RPC パース (500 行 draw) | — | |
| `parse_request/draw_status` | JSON-RPC パース (draw_status) | — | 小メッセージ、高頻度 |
| `parse_request/set_cursor` | JSON-RPC パース (set_cursor) | — | 最小メッセージ |
| `parse_request/menu_show_50` | JSON-RPC パース (menu_show 50件) | — | |
| `state_apply/draw_lines/23` | state.apply(Draw) 単体 | — | |
| `state_apply/draw_lines/100` | state.apply(Draw) 単体 | — | |
| `state_apply/draw_lines/500` | state.apply(Draw) 単体 | — | |
| `state_apply/draw_status` | state.apply(DrawStatus) | — | |
| `state_apply/set_cursor` | state.apply(SetCursor) | — | |
| `state_apply/menu_show_50` | state.apply(MenuShow) | — | |
| `scaling/full_frame/80x24` | full frame at 80x24 | — | ベースライン |
| `scaling/full_frame/200x60` | full frame at 200x60 | — | 大画面 |
| `scaling/full_frame/300x80` | full frame at 300x80 | — | 超大画面 |
| `scaling/parse_apply_draw/500` | パース + apply (500 行) | — | |
| `scaling/parse_apply_draw/1000` | パース + apply (1000 行) | — | |
| `scaling/diff_incremental/80x24` | diff() 差分なし 80x24 | — | |
| `scaling/diff_incremental/200x60` | diff() 差分なし 200x60 | — | |
| `scaling/diff_incremental/300x80` | diff() 差分なし 300x80 | — | |

## パフォーマンス原則

1. **端末 I/O が支配的:** CPU パイプライン (40 μs) は端末 I/O (200-3,600 μs) の 1-20%。グリッド操作の最適化より、diff の精度向上 (変更セル数の最小化) が効果的
2. **アロケーションを避ける:** ホットパス (paint, layout) でのヒープアロケーションを最小化する。BufferRef パターンで大きなデータの clone を回避
3. **計測してから最適化:** `cargo bench --bench rendering_pipeline` で計測し、ボトルネックを特定してから対処する。CI で 15% 超の回帰を自動検出
4. **プラグインのコストを制限:** 現在 10 plugins で view() 2.35 μs、dispatch 1.85 μs。線形スケーリングを監視し、合計が数十 μs に達したら対策を検討
5. **キャッシュは必要になってから:** layout() は 0.38 μs (閾値 5 μs まで 13x 余裕)。VirtualList 等は問題が顕在化してから導入する
