# Kasane - 要件トレーサビリティ

本ドキュメントは、要件ごとの解決層、状態、Phase、上流依存を追跡する tracker である。
要件本文そのものは [requirements.md](./requirements.md) を正本とする。

## 1. 文書の責務

本ドキュメントは、[requirements.md](./requirements.md) に定義された要件が
どの仕組みで解決されるか、どの段階まで実装・実証されているか、どの項目が上流制約に
よってブロックされているかを追跡する。

要件本文そのものは [requirements.md](./requirements.md) を正本とする。
現行意味論は [semantics.md](./semantics.md)、実装順序は [roadmap.md](./roadmap.md)、
上流ブロッカーは [upstream-dependencies.md](./upstream-dependencies.md) を参照。

## 2. 解決層の分類

各要件は、Kasane のどの仕組みで解決されるかに応じて分類される。
この分類は「どのレイヤーが責任を持つか」を定義するものではない。
責務境界は [layer-responsibilities.md](./layer-responsibilities.md) を参照。

| 解決層 | 説明 | 基盤設計への影響 |
|--------|------|-----------------|
| **レンダラ** | 描画エンジン・入力処理の基本実装で自動的に解決される | 基盤メカニズム不要。正しく実装するだけで解決 |
| **設定** | `config.toml` / `ui_options` による設定で解決される | 基盤メカニズムの上に設定インターフェースを構築 |
| **基盤** | Kasane の UI 基盤や拡張機構で解決される | プラグイン作者も同じメカニズムを利用可能 |
| **プロトコル制約** | Kakoune プロトコルの制限により完全解決不可 | ヒューリスティック回避。上流への貢献を追跡 |

> **補完モデル:** 解決層は「どの仕組みで解決するか」(HOW) の分類。これを補完する
> 「どのレイヤーが責任を持つか」(WHERE) の分類は
> [三層レイヤー責務モデル](./layer-responsibilities.md) を参照。

## 3. コア機能要件トレーサビリティ

### 3.1 基本レンダリング (R-001, R-003〜R-009)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-001 | 基盤 | Element ツリー (BufferRef) で構築 | ✓ Phase 1 |
| R-003 | レンダラ | ソフトウェアカーソル描画 | ✓ Phase 1 |
| R-004 | レンダラ | padding_face による描画 | ✓ Phase 1 |
| R-005 | レンダラ | リサイズ検知と `resize` メッセージ送信 | ✓ Phase 1 |
| R-006 | レンダラ | 24bit RGB 直接描画 | ✓ Phase 1 |
| R-007 | レンダラ | ダブルバッファリング (`CellGrid`) | ✓ Phase 1 |
| R-008 | レンダラ | `unicode-width` ベースの幅計算 | ✓ Phase 1 |
| R-009 | レンダラ | プレースホルダグリフの描画 | ✓ Phase 1 |

### 3.2 標準フローティング UI (R-010〜R-032)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-010 | 基盤 | `Stack` + `Overlay` で構築 | ✓ Phase 1 |
| R-011 | 基盤 | `OverlayAnchor` によるスタイル別配置 | ✓ Phase 1 |
| R-012 | 基盤 | `MenuState` の `selected` 反映 | ✓ Phase 1 |
| R-013 | 基盤 | `MenuState` クリアで即時非表示 | ✓ Phase 1 |
| R-014 | 設定 + 基盤 | `MenuPlacement` / `InfoPlacement` による配置 policy | ✓ Phase 2 |
| R-016 | レンダラ | イベントバッチング (`recv + try_recv`, 安全弁付き) | ✓ Phase 2 |
| R-020 | 基盤 | `Stack` + `Overlay` で構築 | ✓ Phase 1 |
| R-021 | 基盤 | `OverlayAnchor` + `InfoStyle` 切り替え | ✓ Phase 1 |
| R-022 | 基盤 | `InfoState` クリアで即時非表示 | ✓ Phase 1 |
| R-023 | 基盤 | `infos: Vec<InfoState>` + `InfoIdentity` で同時管理 | ✓ Phase 2 |
| R-024 | 基盤 | `scroll_offset` + `InteractiveId` + マウスホイール | ✓ Phase 2 |
| R-025 | 基盤 | `compute_pos` の `&[Rect]` 汎化 + avoid rect | ✓ Phase 2 |
| R-030 | 基盤 | `OverlayAnchor::AnchorPoint` | ✓ Phase 1 |
| R-031 | 基盤 | `compute_pos` のクランプロジック | ✓ Phase 1 |
| R-032 | 基盤 | `Stack` の描画順序 | ✓ Phase 1 |

### 3.3 標準ステータス / プロンプト UI (R-060, R-061, R-063, R-064)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-060 | 基盤 | Element ツリーで構築 | ✓ Phase 1 |
| R-061 | 設定 | `status_at_top` による `Column` 配置順序変更 | ✓ Phase 2 |
| R-063 | 基盤 | `markup.rs` で `{face_spec}text{default}` パース | ✓ Phase 2 |
| R-064 | 基盤 | `cursor_count` バッジ (`FINAL_FG+REVERSE` 検出) | ✓ Phase 2 |

### 3.4 入力処理 (R-040〜R-047)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-040〜R-045 | レンダラ | `crossterm` イベント変換 | ✓ Phase 1 |
| R-046 | レンダラ | 選択中スクロールの座標計算 | ✓ Phase 3 |
| R-047 | レンダラ | 右クリックドラッグイベント処理 | ✓ Phase 3 |

### 3.5 カーソルとテキスト装飾 (R-050, R-051, R-053)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-050 | レンダラ | ソフトウェアレンダリング | ✓ Phase 4a |
| R-051 | レンダラ | フォーカス追跡 | ✓ Phase 4a |
| R-053 | レンダラ | プロトコルパーサーと TUI バックエンドは一部対応済み。GUI バックエンドは要追随 | ○ 部分対応 |

### 3.6 UI オプション / クリップボード / スクロール / 標準 UI スタイル

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-070, R-071 | レンダラ | 状態反映と再描画 | ✓ Phase 1 |
| R-080〜R-082 | レンダラ | システムクリップボード API 直接アクセス (`arboard`) | ✓ Phase 3 |
| R-090〜R-093 | レンダラ | スクロール計算の独自実装 (スムーズスクロール + `PageUp` / `PageDown`) | ✓ Phase 3 |
| R-028 | 設定 + 基盤 | `StyleToken` + `Theme` + `ThemeConfig` | ✓ Phase 2 |

## 4. 拡張基盤要件トレーサビリティ

P 系は本体が保証する拡張基盤能力を追跡する。具体ユースケースは
[requirements.md](./requirements.md#4-実証対象代表ユースケース) を参照。

| ID | 解決層 | 備考 | 状態 | Phase |
|----|--------|------|------|-------|
| P-001 | 基盤 | `Slot::Overlay` + `Decorator(Buffer)` | ○ 部分実証 (`color_preview`) | 4b |
| P-002 | 基盤 | `OverlayAnchor::Absolute` | ○ インフラ実装済み (プラグイン実証は未) | 4b |
| P-003 | 基盤 | `Stack` の描画順序と clip | ✓ Phase 1 | 1 |
| P-010 | 基盤 + プロトコル制約 | `Slot::BufferLeft/Right`。一部は semantic type や scroll 情報に依存 | ○ 部分実証 | 4b |
| P-011 | 基盤 | 補助領域への寄与 API | ○ 部分実証 (`color_preview`) | 4b |
| P-012 | 基盤 + プロトコル制約 | 文書全体位置との完全対応は一部上流依存 | - 進行中 | [上流依存](./upstream-dependencies.md) |
| P-020 | 基盤 | `Interactive Element` でヒットテスト | ○ 部分実証 (`color_preview`) | 4b |
| P-021 | 基盤 | event routing と target 解決 | ○ 部分実装 | 4b |
| P-022 | 基盤 | semantic recognizer / binding | - 候補 | 5c |
| P-023 | レンダラ | GUI バックエンド (`winit`) での native event | - 候補 | 4b |
| P-030 | 基盤 | display transformation hook | - 候補 | 5c |
| P-031 | 基盤 | 省略・代理表示・追加表示の合成規則 | - 候補 | 5c |
| P-032 | 基盤 + 意味論 | Observed / policy 分離。正本は [semantics.md](./semantics.md) | ○ 理論整理済み | 5c |
| P-033 | 基盤 | plugin 定義変形 API | - 候補 | 5c |
| P-034 | 基盤 + 意味論 | 読み取り専用 / 制限付き interaction policy | - 候補 | 5c |
| P-040 | 基盤 | display unit model | - 候補 | 5c |
| P-041 | 基盤 | geometry / source mapping / role | - 候補 | 5c |
| P-042 | 基盤 + レンダラ | visual navigation / hit test | - 候補 | 5c |
| P-043 | 基盤 | plugin 定義 navigation policy | - 候補 | 5c |
| P-050 | 基盤 | `Flex` による分割レイアウト | - 候補 | 5a |
| P-051 | 基盤 | focus / input routing across surfaces | - 候補 | 5a |
| P-052 | 基盤 | workspace / tab / pane manager 抽象 | - 候補 | 5a |
| P-060 | レンダラ + 基盤 | Kasane 独自 decoration 能力 | - 候補 | 5c |
| P-061 | 設定 + 基盤 | セマンティックスタイルトークン | ○ 基礎実装あり | 5a |
| P-062 | レンダラ | GUI バックエンド (`glyphon`) 前提の text policy | - 候補 | 5c |

## 5. 上流依存・縮退動作トレーサビリティ

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| D-001 | プロトコル制約 | 起動時 `info` は Kakoune 側挙動の切り分け待ち | [上流依存](./upstream-dependencies.md) |
| D-002 | プロトコル制約 | 視野外情報の完全性が不足 | [上流依存](./upstream-dependencies.md) |
| D-003 | プロトコル制約 | `draw_status` context 不足。heuristic fallback のみ | [上流依存](./upstream-dependencies.md) |
| D-004 | プロトコル制約 | scroll 情報不足により完全な右側ナビゲーション UI は不可 | [上流依存](./upstream-dependencies.md) |

## 6. 追跡上の注記

- 解決層は「どの仕組みで解決するか」を示す。責務境界は
  [layer-responsibilities.md](./layer-responsibilities.md) が正本。
- Phase の意味と実装順序は [roadmap.md](./roadmap.md) を参照。
- 上流に依存する項目の再統合条件は
  [upstream-dependencies.md](./upstream-dependencies.md) を参照。
- 非機能要件、とくに性能要件の実測と実装状況は
  [performance.md](./performance.md) と
  [performance-benchmarks.md](./performance-benchmarks.md) を参照。

## 7. 関連文書

- [requirements.md](./requirements.md) — 要件本文の正本
- [roadmap.md](./roadmap.md) — Phase と未完了項目
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流依存の追跡
- [layer-responsibilities.md](./layer-responsibilities.md) — 責務境界の判断基準
