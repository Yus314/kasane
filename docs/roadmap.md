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
Plugin API の実証状況は [plugin-api.md](./plugin-api.md)、性能の数値と実装状況は
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
| Phase 4 | 拡張機能実証 | Open | 4a は大半完了、4b に未完了項目あり |
| Phase 5 | Surface / Workspace 拡張性基盤 | Open | 5a 完了、5b/5c が未完了 |

## 3. 現在の未完了項目

### 3.1 Phase 4 - 拡張機能実証

**既達成:**
- 4a のプラグイン実証: `cursor_line`, `color_preview`
- 先送り項目のうち R-050, R-051 は完了
- ADR-010 の Stage 1-4 は完了

**未完了:**

| 項目 | 種別 | 状態 | 次の作業 |
|------|------|------|----------|
| R-027 | コア | Open | 上流挙動を確認したうえで `update()` ベースの最小キューイングを入れる |
| E-005 | Plugin API 実証 | Open | `OverlayAnchor::Absolute` を外部または WASM ゲストで実証する |
| E-006 | Plugin API 実証 | Open | 選択範囲ハイライト系プラグインで `contribute_line()` 拡張を実証する |
| E-030 | GUI 機能 | Open | `DroppedFile` 系イベントを `:edit` へ接続する |

### 3.2 Phase G - GUI 描画追随項目

GUI バックエンドの基盤自体は完了しているが、忠実描画の追随項目は残っている。

| 項目 | 種別 | 状態 | 次の作業 |
|------|------|------|----------|
| R-053 | GUI 描画 | Open | 下線種別、下線色、取り消し線の表現を GUI バックエンドで忠実描画に揃える |

### 3.3 Phase 5 - Surface / Workspace

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
| 5c 外部プラグイン候補 | Backlog | Open | E-003, E-004, E-010, E-011, E-012, E-022, E-023, E-031, E-040, E-041 を候補として維持 |

### 3.4 Phase W 残課題

Phase W の基盤自体は完了しているが、運用面の残課題は残っている。

| 項目 | 状態 | 次の作業 |
|------|------|----------|
| プラグインマニフェスト | Open | 名前、バージョン、依存、使用 extension point を定義する |
| プラグイン設定 API | Open | `config.toml` との接続方針を固める |
| コンパイル済み component キャッシュ | Open | `Engine::precompile_component` ベースで起動コストを削減する |

## 4. 上流依存に分離した項目

次の項目は本ロードマップでは追跡せず、[upstream-dependencies.md](./upstream-dependencies.md) を正本とする。

- R-052: 画面外カーソルインジケータ
- R-062: ステータスラインコンテキスト推定
- E-001: オーバーレイレイヤー (完全版)
- E-002: ガターアイコン (完全版)
- E-020: スクロールバー
- E-021: スクロールバーアノテーション

## 5. 更新ルール

次の場合にこの文書を更新する。
- phase の状態が変わったとき
- 未完了項目が完了・分離・棚上げになったとき
- tracker の正本を別文書へ移したとき

## 6. 関連文書

- [requirements-traceability.md](./requirements-traceability.md) — 要件ごとの状態
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流ブロッカー
- [plugin-api.md](./plugin-api.md) — extension point の実証状況
- [performance-benchmarks.md](./performance-benchmarks.md) — 性能実装の進捗
