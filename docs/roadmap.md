# 実装ロードマップ

本ドキュメントは、Kasane の **現在 open な実装 workstream** を追跡する tracker である。
詳細な設計理由や現行意味論ではなく、「今どこが未完了で、次に何を出すか」だけを記録する。

## 1. 文書の責務

この文書で扱うのは次の 3 点だけに限定する。

- 現在 open / active な workstream
- 次に出す deliverable
- backlog / 上流依存への委譲先

次はこの文書の責務ではない。

- 現行意味論の説明
- 共有 Plugin API の詳細仕様
- native escape hatch の長い設計説明
- 完了済みフェーズの詳細な履歴

詳細な設計理由は [decisions.md](./decisions.md)、現行意味論は [semantics.md](./semantics.md)、
shared API と native escape hatch の責務分担は
[layer-responsibilities.md](./layer-responsibilities.md)、plugin から見た現行仕様は
[plugin-api.md](./plugin-api.md)、性能の数値と実装状況は
[performance-benchmarks.md](./performance-benchmarks.md) を参照。

## 2. 現在の優先順

### 2.1 Now

| Workstream | 状態 | 次の deliverable | 完了条件 |
|---|---|---|---|
| Session / Surface parity | Active | session-bound surface 自動生成 | active / inactive session ごとの surface 群が自動生成され、session 切替で surface 側も一貫して追従する |
| Multi-session UI parity | Active | session switcher か session list の最小 UI | 複数 session を user-visible に切り替えられ、active session 以外の存在が UI から分かる |
| GUI fidelity | Active | R-053 の忠実描画 | 下線種別、下線色、取り消し線が GUI で protocol 相当の見た目になる |
| Display transformation / display unit model | Active | P-030〜P-043 の最初の slice | display transformation / navigation policy の最小実装と proof が入る |

### 2.2 Next

| Workstream | 状態 | 次の deliverable |
|---|---|---|
| WASM runtime operations | Open | plugin manifest, plugin settings API, precompiled component cache の順で運用機能を追加 |
| Native escape hatch redesign | Open | `PaintHook` の高レベル化、`Pane` / `Workspace` parity model の定義 |
| Core event / degraded behavior | Open | D-001 の最小キューイング、P-023 `DropEvent` 導入 |

### 2.3 Backlog

| Workstream | 状態 | 注記 |
|---|---|---|
| External plugin candidates | Open | インデントガイド、クリッカブルリンク、ビルトインスプリット、フローティングパネル、コード折りたたみ、表示行ナビゲーション、URL 検出、領域別 text policy などを候補として維持 |

## 3. Open Workstreams

### 3.1 Session / Surface parity

現在地:

- `SessionManager` 基盤、primary session 連携、runtime の `spawn-session` / `close-session` 配線は導入済み
- inactive session の Kakoune event は off-screen snapshot へ継続反映される
- hosted surface の `render-surface` / `handle-surface-event` / `handle-surface-state-changed` は導入済み

残件:

- session-bound surface の自動生成
- session 切替時に session ごとの surface 群を一貫して attach / detach する仕組み
- surface registry / workspace 側で session identity を first-class に扱う整理

次の deliverable:

- active session ごとに buffer/status/supplemental surface を自動生成する最小実装
- session 切替で surface 構成が deterministic に入れ替わる proof

proof / 完了条件:

- session を 2 つ以上持つ状態で surface 構成が壊れない
- session 切替で stale surface が残らない
- active / inactive session の snapshot と surface の対応が自動テストで固定される

### 3.2 Multi-session UI parity

現在地:

- runtime は複数 session を保持できる
- inactive session の state snapshot は保持される
- 描画対象はまだ active session 1 つだけ

残件:

- session list / session switcher の最小 UI
- user-visible な active session 表示
- session close / promote の UI feedback

次の deliverable:

- session の一覧と active 状態を見せる最小 UI
- UI から session を切り替える command path

proof / 完了条件:

- 複数 session が UI 上で識別できる
- UI から active session を切り替えられる
- close 時の creation-order promotion が UI でも観測できる

### 3.3 GUI fidelity

残件:

- R-053: 下線種別、下線色、取り消し線の忠実描画

次の deliverable:

- 現行 glyph / scene path に必要な style 情報を通す
- GUI 側の proof / screenshot test またはレンダリングテストを追加する

### 3.4 Display transformation / display unit model

残件:

- P-030〜P-043
- display transformation
- display unit model
- navigation policy

次の deliverable:

- 上記のうち最小の 1 slice を選び、proof artifact 付きで導入する

### 3.5 WASM runtime operations

残件:

- plugin manifest
- plugin settings API
- コンパイル済み component cache

次の deliverable:

- manifest か settings API のどちらかを first implementation として確定する

### 3.6 Native escape hatch redesign

残件:

- `PaintHook` を `CellGrid` 直操作に依存しない高レベル render hook へ再設計
- `Pane` / `Workspace` parity model の定義

次の deliverable:

- `PaintHook` の再設計方針を固め、移行先 API の最小骨格を置く

### 3.7 Core event / degraded behavior

残件:

- D-001: `update()` ベースの最小キューイング
- P-023: `DropEvent` を `InputEvent` / plugin API / WIT に導入

次の deliverable:

- D-001 か P-023 のどちらかを first slice として選び、コア path に載せる

## 4. フェーズ状態サマリ

| Phase | 主目的 | 状態 | 注記 |
|---|---|---|---|
| Phase 0 | 開発環境・CI 基盤 | ✓ 完了 | project bootstrap |
| Phase 1 | MVP (TUI コア機能 + 宣言的 UI 基盤) | ✓ 完了 | Element + TEA + 基本スロット |
| Phase 2 | 強化フローティングウィンドウ + プラグイン基盤 | ✓ 完了 | 一部項目は後続 workstream に移動済み |
| Phase 3 | 入力・クリップボード・スクロール強化 | ✓ 完了 | TUI 側の基礎入力機能は完了 |
| Phase G | GUI バックエンド | ✓ 完了 | 基盤は完了。忠実描画の追随は workstream 化 |
| Phase W | WASM プラグインランタイム基盤 | ✓ 基盤完了 | 運用面の残課題は `WASM runtime operations` へ集約 |
| Phase 4 | 共有 Plugin API 妥当性検証 | ✓ 完了 | 公開 extension point の proof artifact は充足済み |
| Phase 5 | Surface / Workspace / 表示再構成基盤 | Open | session/surface parity と display transformation が継続中 |
| Phase P | プラグイン I/O 基盤 | ✓ 完了 | P-1 / P-2 / P-3 完了 |

## 5. 上流依存に分離した項目

次の項目は本ロードマップでは追跡せず、[upstream-dependencies.md](./upstream-dependencies.md) を正本とする。

- D-002: 画面外カーソル / 選択範囲の補助表示
- D-003: ステータスラインコンテキスト推定
- P-001: オーバーレイ合成 (完全版)
- P-010 / P-011: 補助領域寄与 (完全版)
- D-004: 右側ナビゲーション UI の完全性

## 6. 更新ルール

次の場合にこの文書を更新する。

- `Now` / `Next` / `Backlog` の優先順位が変わったとき
- open workstream の deliverable または完了条件が変わったとき
- phase 状態が変わったとき
- tracker の正本を別文書へ移したとき

## 7. 関連文書

- [requirements-traceability.md](./requirements-traceability.md) — 要件ごとの状態
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流ブロッカー
- [layer-responsibilities.md](./layer-responsibilities.md) — shared API validation と native escape hatch の整理
- [plugin-api.md](./plugin-api.md) — plugin から見た現行 API
- [plugin-development.md](./plugin-development.md) — plugin authoring の実務ガイド
- [performance-benchmarks.md](./performance-benchmarks.md) — 性能実装の進捗
