# 実装ロードマップ

本ドキュメントは、Kasane の実装フェーズと未完了項目を追跡する tracker である。
詳細な設計理由や現行意味論ではなく、「今どこまで進んでいるか」を記録する。

## 1. 文書の責務

本ドキュメントは、Kasane の実装フェーズと未完了項目を追跡するための tracker である。

この文書で扱うのは次の 3 点だけに限定する。
- フェーズごとの状態
- まだ残っている作業
- 他の tracker へ委譲した項目

詳細な設計理由は [decisions.md](./decisions.md)、現行意味論は [semantics.md](./semantics.md)、
共有 Plugin API の妥当性検証状況と native escape hatch の扱いは
[layer-responsibilities.md](./layer-responsibilities.md)、性能の数値と実装状況は
[performance-benchmarks.md](./performance-benchmarks.md) を参照。

## 2. フェーズ一覧

| Phase | 主目的 | 状態 | 注記 |
|------|--------|------|------|
| Phase 0 | 開発環境・CI 基盤 | ✓ 完了 | project bootstrap |
| Phase 1 | MVP (TUI コア機能 + 宣言的 UI 基盤) | ✓ 完了 | Element + TEA + 基本スロット |
| Phase 2 | 強化フローティングウィンドウ + プラグイン基盤 | ✓ 完了 | 一部項目は上流依存または後続フェーズへ移動 |
| Phase 3 | 入力・クリップボード・スクロール強化 | ✓ 完了 | R-046〜R-047, R-080〜R-082, R-090〜R-093 |
| Phase G | GUI バックエンド | ✓ 完了 | winit + wgpu + glyphon |
| Phase W | WASM プラグインランタイム基盤 | ✓ 基盤完了 | 残課題は別表で継続追跡 |
| Phase 4 | 共有 Plugin API 妥当性検証 | Open | WASM 到達可能な公開 API を対象とする |
| Phase 5 | Surface / Workspace / 表示再構成基盤 | Open | 5a 完了、5b/5c が未完了 |
| Phase P | プラグイン I/O 基盤 | Open | Phase 4 に依存。Phase 5 とは独立に進行可能 |

## 3. 現在の未完了項目

### 3.1 Phase 4 - 共有 Plugin API 妥当性検証

Phase 4 は **WASM から到達可能な共有 Plugin API** の妥当性検証だけを扱う。
`PaintHook`、`Surface`、`Pane` などの native escape hatch は本 Phase の完了条件に含めない。

Phase 4 の完了条件:
- 公開 extension point ごとに少なくとも 1 つの proof artifact がある
- proof artifact は自動テストで検証されている
- proof artifact の形式は `examples/`、WASM fixture、統合テスト内 plugin のいずれでもよい

**既達成:**
- 4a のプラグイン実証: `cursor_line`, `color_preview`
- 先送り項目のうち R-050, R-051 は完了
- ADR-010 の Stage 1-4 は完了
- P-010 / P-011 の部分実証: `line_numbers` (`BUFFER_LEFT`), `color_preview` (左ガター + overlay), `cursor_line` (行背景 annotation)
- `transform_menu_item()` は統合テスト内 plugin で proof artifact あり
- `transform(TransformTarget::Buffer)` と `cursor_style_override()` は統合テスト内 plugin で proof artifact あり

**未完了:**

| 項目 | 種別 | 状態 | 次の作業 |
|------|------|------|----------|
| `OverlayAnchor::Absolute` | Shared Plugin API validation | Open | WASM fixture または統合テスト内 plugin で公開経路の proof artifact を追加する |
| `SlotId::ABOVE_BUFFER / BELOW_BUFFER / Named(...)` | Shared Plugin API validation | Open | sample ではなく最小 proof artifact でよいので 1 件ずつ通す |

### 3.2 Phase G - GUI 描画追随項目

GUI バックエンドの基盤自体は完了しているが、忠実描画の追随項目は残っている。

| 項目 | 種別 | 状態 | 次の作業 |
|------|------|------|----------|
| R-053 | GUI 描画 | Open | 下線種別、下線色、取り消し線の表現を GUI バックエンドで忠実描画に揃える |

### 3.3 Phase 5 - Surface / Workspace / 表示再構成

**既達成:**
- 5a の基盤: `Surface`, `SurfaceRegistry`, `Workspace`, `WorkspaceCommand` の基本導入
- コアサーフェス実装とエフェメラル surface の同期

**未完了:**

| 項目 | 種別 | 状態 | 次の作業 |
|------|------|------|----------|
| SessionManager | コア基盤 | Open | 複数 `kak -ui json` セッション管理と surface 自動生成を実装する |
| `spawn-session` / `close-session` | WIT / Command | Open | WASM から管理対象セッションを生成・終了できるようにする |
| WorkspaceCommand 完成 | コア基盤 | Open | `FocusDirection`, `Resize`, `Swap`, `Float`, `Unfloat` の dispatch を詰める |
| Split divider UI | UI | Open | divider 描画と drag による `Resize` 発行を実装する |
| P-030〜P-043 | 表示再構成基盤 | Open | display transformation、display unit model、navigation policy を段階的に導入する |
| 5c 外部プラグイン候補 | Backlog | Open | インデントガイド、クリッカブルリンク、ビルトインスプリット、フローティングパネル、コード折りたたみ、表示行ナビゲーション、URL 検出、領域別 text policy などを候補として維持 |

### 3.4 Phase W 残課題

Phase W の基盤自体は完了しているが、運用面の残課題は残っている。

| 項目 | 状態 | 次の作業 |
|------|------|----------|
| プラグインマニフェスト | Open | 名前、バージョン、依存、使用 extension point を定義する |
| プラグイン設定 API | Open | `config.toml` との接続方針を固める |
| コンパイル済み component キャッシュ | Open | `Engine::precompile_component` ベースで起動コストを削減する |

### 3.5 Phase P - プラグイン I/O 基盤

Phase 4 のプラグイン API 実証が前提。Phase 5 (Surface / Workspace) とは独立に進行可能。
設計根拠は [ADR-019](./decisions.md#adr-019-プラグイン-io-基盤--ハイブリッドモデル) を参照。

| サブフェーズ | 項目 | 種別 | 状態 | 次の作業 |
|---|---|---|---|---|
| P-1 | WASI ケイパビリティ基盤 | WASM ランタイム | Open | プラグインマニフェストにケイパビリティ宣言を追加し、`WasiCtxBuilder` にプラグイン別の `preopened_dir` / `env` / `inherit_monotonic_clock` を設定する仕組みを実装する |
| P-2 | プロセス実行基盤 | コア基盤 + プラグイン API | Open | `IoEvent` / `ProcessEvent` 型、`Plugin::on_io_event()`、`Command::SpawnProcess`、`ProcessManager`、イベントループへの `Event::ProcessOutput` 追加、16ms バッチ配送、ジョブ ID / キャンセルを実装する。WIT に `io-event` 型と `on-io-event` 関数を追加する |
| P-3 | 実証・安定化 | 実証 | Open | ファジーファインダー参照実装 (WASM ゲスト) を作成し、ランタイムフレームタイム計測、バックプレッシャー調整を行う |

**設計方針 (ADR-019):**
- **ハイブリッドモデル**: 同期 I/O (ファイルシステム、環境変数、時計) は WASI 直接、非同期 I/O (プロセス実行、将来のネットワーク) はホスト媒介 (`Command` + `IoEvent`)
- **IoEvent 統一型**: `Plugin::on_io_event(IoEvent)` 1 メソッドで全 I/O イベントを配送。将来の I/O 種別追加は `IoEvent` variant 追加のみ
- **wasmtime async 化は行わない**: `add_to_linker_sync` を維持

**解放されるユースケース:** ファジーファインダー、ファイルブラウザ、外部リンター連携、ストリーミング検索結果、長時間タスクの進捗表示

### 3.6 別トラック: コアイベント / 縮退追随

Phase 4 に含めないが、コア側で継続して追う項目。

| 項目 | 種別 | 状態 | 次の作業 |
|------|------|------|----------|
| D-001 | 縮退動作 | Open | 上流挙動を確認したうえで `update()` ベースの最小キューイングを入れる |
| P-023 汎用 drop routing | コアイベントモデル拡張 | Open | `DropEvent` を `InputEvent` / plugin API / WIT に導入し、UI 要素または plugin へ配送できるようにする |

### 3.7 別トラック: Native Escape Hatches と WASM parity

native-only API は Phase 4 の完了条件から分離し、ここで追跡する。

| 項目 | 現在位置づけ | 状態 | 次の作業 |
|------|--------------|------|----------|
| `PaintHook` | 暫定 escape hatch | Active | `CellGrid` 直操作に依存しない高レベル render hook へ再設計し、将来の WASM parity を目指す |
| `Surface` / `SURFACE_PROVIDER` | native-only だが parity target | Active | `Box<dyn Surface>` をそのまま公開せず、hosted surface model として WASM から扱える形を設計する |
| `Pane` / `Workspace` 高度 API | native-only だが parity target | Active | 生ポインタ的な API ではなく command / observer ベースで parity model を定義する |

## 4. 上流依存に分離した項目

次の項目は本ロードマップでは追跡せず、[upstream-dependencies.md](./upstream-dependencies.md) を正本とする。

- D-002: 画面外カーソル / 選択範囲の補助表示
- D-003: ステータスラインコンテキスト推定
- P-001: オーバーレイ合成 (完全版)
- P-010 / P-011: 補助領域寄与 (完全版)
- D-004: 右側ナビゲーション UI の完全性

## 5. 更新ルール

次の場合にこの文書を更新する。
- phase の状態が変わったとき
- 未完了項目が完了・分離・棚上げになったとき
- tracker の正本を別文書へ移したとき

## 6. 関連文書

- [requirements-traceability.md](./requirements-traceability.md) — 要件ごとの状態
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流ブロッカー
- [layer-responsibilities.md](./layer-responsibilities.md) — shared API validation と native escape hatch の整理
- [performance-benchmarks.md](./performance-benchmarks.md) — 性能実装の進捗
