# パフォーマンス分析

本ドキュメントでは、kasane の宣言的 UI アーキテクチャにおけるパフォーマンス特性を分析する。
現在の命令的パイプラインと新アーキテクチャの比較、ボトルネックの特定、対策を記述する。

## 現在のフレーム実行フロー

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
render_frame(&state, &mut grid)
  ├── grid.clear()          ── O(w×h)
  ├── render_buffer()       ── O(rows × avg_line_width)
  ├── render_status()       ── O(width)
  ├── render_menu()         ── O(visible_items)
  └── render_info()         ── O(visible_info_lines)
  │
  ▼
grid.diff()                 ── O(w×h)
  │
  ▼
backend.draw(&diffs)        ── O(changed_cells) ← 端末 I/O (最も遅い)
backend.flush()
  │
  ▼
grid.swap()                 ── O(w×h)
```

## 現在のフレームあたりコスト (80×24 ターミナル)

| 処理 | 計算量 | 実測目安 | 備考 |
|---|---|---|---|
| `grid.clear()` | O(w×h) = 1,920 セル | ~5 μs | 全セルを Cell::default() で初期化 |
| `render_buffer()` | O(rows × line_width) | ~10-30 μs | Atom→セル変換、unicode-width 計算 |
| `render_status()` | O(width) | ~1 μs | 1 行分 |
| `render_menu()` | O(visible_items) | ~5-15 μs | スタイルにより異なる |
| `render_info()` | O(visible_lines) | ~5-20 μs | word_wrap がある場合は高い |
| `grid.diff()` | O(w×h) = 1,920 セル比較 | ~5 μs | Cell 同士の等価比較 |
| `grid.swap()` + clear | O(w×h) | ~5 μs | swap は O(1)、clear が O(w×h) |
| `backend.draw()` | O(changed_cells) | **100-3,000 μs** | エスケープシーケンス生成 + I/O |
| `backend.flush()` | O(1) | **50-500 μs** | stdout への write |
| **合計** | | **~200-3,600 μs** | |

**支配的コスト: 端末 I/O (`backend.draw()` + `backend.flush()`)。**
グリッド操作の合計は 30-80 μs で全体の 5-20%。残りは端末への書き込み。

## 既存の最適化

| 最適化 | 対象 | 効果 |
|---|---|---|
| CompactString | Cell.grapheme | 短い文字列 (24B 以下) のヒープアロケーション回避 |
| bitflags Attributes | Face.attributes | Vec\<Attribute\> → u16。Copy 型化によるアロケーション排除 |
| ダブルバッファリング | CellGrid | `std::mem::swap()` でポインタ交換。O(1) |
| 差分描画 | CellGrid.diff() | 変更セルのみ端末に送信。I/O 量を最小化 |
| イベントバッチング | main.rs | `try_recv()` で pending イベントを全て消化してから 1 回描画 |
| SIMD JSON | protocol.rs | simd_json による高速 JSON パース |

## 新アーキテクチャが追加するオーバーヘッド

### フレーム実行フローの変化

```
現在:     State ──→ CellGrid (直接描画)

新:       State ──→ view() ──→ Element ──→ layout() ──→ paint() ──→ CellGrid
                    ~1 μs       ~0.5 KB     ~1 μs        ~30 μs
                    ^^^^^^^^    ^^^^^^^^^    ^^^^^^^^^    ^^^^^^^^^^
                    新規コスト   中間表現     新規コスト    既存と同等
```

### 1. Element ツリー構築: view()

**コアUI のノード数見積もり:**

| コンポーネント | ノード数 |
|---|---|
| ルート Flex (Column) | 1 |
| バッファ領域 (BufferRef) | 1 |
| ステータスバー Flex (Row) | 3 |
| メニュー (表示時) | 5-10 |
| 情報ポップアップ (表示時) | 5-10 |
| Slot の構造ノード | 3-5 |
| **合計** | **20-30** |

**構築コスト:**
- Enum 構築 + Vec アロケーション: ノードあたり 20-50 ns
- 20-30 ノード × ~30 ns = **約 1 μs**

**プラグイン追加時:**
- プラグインごとに 5-20 ノード追加
- 10 プラグイン × 10 ノード = 100 ノード追加
- 合計 130 ノード × ~30 ns = **約 4 μs**

### 2. レイアウト計算: layout()

**Flex レイアウトの計算手順:**

```
measure() 下→上:
  各 Flex ノードで子を走査、固定/可変を分類、残り空間を比率分配
  ノードあたり O(子の数) の加算・比較

place() 上→下:
  各 Flex ノードで子の位置を順番に計算
  ノードあたり O(子の数) の加算
```

- 30 ノード × 2 パス × ~20 ns = **約 1 μs**
- Overlay の配置は既存 `compute_pos` と同等 (O(1))

### 3. 描画: paint()

Element ツリーを走査して CellGrid に描画する。**描画自体のコストは現在の `render_*()` 関数群と同等。** 追加コストは再帰走査の match 分岐のみ:

- 30 ノードの再帰走査 × ~10 ns (match + 関数呼び出し) = **約 0.3 μs**

### 4. Plugin dispatch

| 処理 | コスト | 頻度 |
|---|---|---|
| HashMap lookup (PluginId) | ~30 ns | イベントごとに 1 回 |
| `Box<dyn Any>` downcast | ~5 ns (TypeId 比較) | イベントごとに 1 回 |
| Slot 収集 (全プラグイン × 全 Slot) | ~80 ns × (plugins × slots) | フレームごとに 1 回 |
| vtable 経由の Plugin メソッド呼び出し | ~5 ns | 各呼び出しごと |

10 プラグイン × 8 Slot = 80 回の仮想関数呼び出し: **約 1 μs**

### 5. Decorator チェーン

構造レベルの Decorator (Buffer, StatusBar 等):
- Decorator あたり: Element 構築 1-3 ノード + Flex ラップ = ~100 ns
- 5 個の Decorator: **約 0.5 μs**

### 新アーキテクチャの追加コスト合計

| 処理 | コスト |
|---|---|
| Element 構築 (view) | ~1-4 μs |
| レイアウト計算 (layout) | ~1 μs |
| 再帰走査 (paint overhead) | ~0.3 μs |
| Plugin dispatch | ~1 μs |
| Decorator チェーン | ~0.5 μs |
| **合計** | **~4-7 μs** |

**既存のグリッド操作 (30-80 μs) と比較して 10-20% の追加。**
**端末 I/O (200-3,600 μs) と比較して 0.2-3% の追加。**
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
- **段階 2 (静的レイアウトキャッシュ)**: プラグイン追加により Element ノード数が 100 を超え、layout() コストが計測で 5 μs を超えた場合
- **段階 3 (細粒度更新コード生成)**: paint() コストが計測で端末 I/O の 10% (15-360 μs) を超えた場合

上記のコスト削減見積もりはいずれも端末 I/O (200-3,600 μs) に対して小さい。Phase 2 では段階 1 を必ず実装し、段階 2・3 は計測結果に基づいて判断する。

## 注意が必要なボトルネック

### 深刻度: 高

#### バッファ行の clone

Element が所有型 (Owned) のため、バッファの全行をツリーに含めると毎フレーム clone が発生する。

```
50 行 × 5 Atom/行 = 250 Atom
各 Atom: Face(16B) + CompactString(24B) = 40B
合計: ~10 KB + 250 回の CompactString clone
推定コスト: 10-30 μs (現在のフレーム時間の 15-40%)
```

**対策: BufferRef パターン**

```rust
enum Element {
    /// バッファ専用: データを clone せず、paint 時に State から直接描画
    BufferRef { line_range: Range<usize> },
    // ...
}

fn paint(element: &Element, area: Rect, grid: &mut CellGrid, state: &AppState) {
    match element {
        Element::BufferRef { line_range } => {
            // state.core.lines[line_range.clone()] を直接 grid に描画
            // clone なし。現在の render_buffer() と同等のパス
        }
    }
}
```

`BufferRef` により clone コストはゼロになる。paint 時に `&AppState` を渡す必要があるが、TEA の `view()` → `paint()` の間で State は不変であるため安全。

同じパターンは `StyledLineRef` としてメニューアイテムにも適用できる:

```rust
enum Element {
    /// 既存の Line を参照 (メニューアイテム等)
    StyledLineRef { index: usize, source: DataSource },
}

enum DataSource {
    BufferLine,
    MenuItem,
    StatusLine,
}
```

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

## ベンチマーク計画

パフォーマンスの回帰を検出するため、以下のベンチマークを整備する:

### マイクロベンチマーク (criterion)

| ベンチマーク | 測定対象 | 目標 |
|---|---|---|
| `bench_element_construct` | view() の Element ツリー構築時間 | < 10 μs (100 ノード) |
| `bench_flex_layout` | Flex レイアウト計算時間 | < 5 μs (100 ノード) |
| `bench_paint` | Element → CellGrid 描画時間 | 現在の render_frame() と同等 |
| `bench_grid_diff` | CellGrid の差分計算時間 | < 10 μs (80×24) |
| `bench_decorator_chain` | N 段 Decorator の適用時間 | < 1 μs (10 段) |
| `bench_plugin_dispatch` | Slot 収集 + Decorator 適用 | < 5 μs (10 プラグイン) |

### 統合ベンチマーク

| ベンチマーク | 測定対象 | 目標 |
|---|---|---|
| `bench_full_frame` | イベント → 描画完了の全フレーム時間 | < 16 ms (60 FPS) |
| `bench_draw_message` | Kakoune Draw メッセージの処理時間 | < 5 ms |
| `bench_menu_show` | メニュー表示の初回描画時間 | < 5 ms |

### 測定方法

```rust
// criterion ベンチマーク例
fn bench_element_construct(c: &mut Criterion) {
    let state = test_state_with_plugins(10);
    c.bench_function("view_100_nodes", |b| {
        b.iter(|| {
            let element = view(black_box(&state));
            black_box(element);
        });
    });
}
```

## パフォーマンス原則

新アーキテクチャ開発において遵守するパフォーマンス原則:

1. **端末 I/O が支配的:** グリッド操作の最適化より、diff の精度向上 (変更セル数の最小化) が効果的
2. **アロケーションを避ける:** ホットパス (paint, layout) でのヒープアロケーションを最小化する。BufferRef パターンで大きなデータの clone を回避
3. **計測してから最適化:** 推測でなく criterion ベンチマークで計測し、ボトルネックを特定してから対処する
4. **プラグインのコストを制限:** フレームワーク側で Element ノード数の上限警告、Decorator 適用回数の監視を行う
5. **キャッシュは必要になってから:** レイアウトキャッシュ、VirtualList 等は問題が顕在化してから導入する。早すぎる最適化は複雑さを増すだけ
