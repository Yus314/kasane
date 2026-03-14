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

## 3. 機能要件トレーサビリティ

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

### 3.2 フローティングウィンドウ - 補完メニュー (R-010〜R-016)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-010 | 基盤 | `Stack` + `Overlay` で構築 | ✓ Phase 1 |
| R-011 | 基盤 | `OverlayAnchor` によるスタイル別配置 | ✓ Phase 1 |
| R-012 | 基盤 | `MenuState` の `selected` 反映 | ✓ Phase 1 |
| R-013 | 基盤 | `MenuState` クリアで即時非表示 | ✓ Phase 1 |
| R-014 | 設定 + 基盤 | `MenuPlacement` (Auto/Above/Below) | ✓ Phase 2 |
| R-015 | 基盤 | `build_menu_search_dropdown` で垂直ドロップダウン化 | ✓ Phase 2 |
| R-016 | レンダラ | イベントバッチング (`recv + try_recv`, 安全弁付き) | ✓ Phase 2 |

### 3.3 フローティングウィンドウ - 情報ポップアップ (R-020〜R-028)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-020 | 基盤 | `Stack` + `Overlay` で構築 | ✓ Phase 1 |
| R-021 | 基盤 | `OverlayAnchor` + `InfoStyle` 切り替え | ✓ Phase 1 |
| R-022 | 基盤 | `InfoState` クリアで即時非表示 | ✓ Phase 1 |
| R-023 | 基盤 | `infos: Vec<InfoState>` + `InfoIdentity` で同時管理 | ✓ Phase 2 |
| R-024 | 基盤 | `scroll_offset` + `InteractiveId` + マウスホイール | ✓ Phase 2 |
| R-025 | 基盤 | `compute_pos` の `&[Rect]` 汎化 + カーソル avoid | ✓ Phase 2 |
| R-026 | 設定 + 基盤 | `BorderConfig` (5 スタイル) + `StyleToken::Border` | ✓ Phase 2 |
| R-027 | レンダラ | TEA `update()` でキューイング | - 先送り |
| R-028 | 設定 + 基盤 | `StyleToken` + `Theme` + `ThemeConfig` | ✓ Phase 2 |

### 3.4 フローティングウィンドウ - 共通 (R-030〜R-033)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-030 | 基盤 | `OverlayAnchor::AnchorPoint` | ✓ Phase 1 |
| R-031 | 基盤 | `compute_pos` のクランプロジック | ✓ Phase 1 |
| R-032 | 基盤 | `Stack` の描画順序 | ✓ Phase 1 |
| R-033 | 設定 + 基盤 | `Container` の `shadow` プロパティ | ✓ Phase 1 |

### 3.5 入力処理 (R-040〜R-047)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-040〜R-045 | レンダラ | `crossterm` イベント変換 | ✓ Phase 1 |
| R-046 | レンダラ | 選択中スクロールの座標計算 | ✓ Phase 3 |
| R-047 | レンダラ | 右クリックドラッグイベント処理 | ✓ Phase 3 |

### 3.6 カーソルとテキスト装飾レンダリング (R-050〜R-053)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-050 | レンダラ | ソフトウェアレンダリング | ✓ Phase 4a |
| R-051 | レンダラ | フォーカス追跡 | ✓ Phase 4a |
| R-052 | 基盤 | Slot または Decorator でインジケータ表示 | [上流依存](./upstream-dependencies.md) |
| R-053 | レンダラ | プロトコルパーサーと TUI バックエンドは一部対応済み。GUI バックエンドは要追随 | ○ 部分対応 |

### 3.7 ステータスバー (R-060〜R-064)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-060 | 基盤 | Element ツリーで構築 | ✓ Phase 1 |
| R-061 | 設定 | `status_at_top` による `Column` 配置順序変更 | ✓ Phase 2 |
| R-062 | レンダラ | ヒューリスティック推定 | - 先送り |
| R-063 | 基盤 | `markup.rs` で `{face_spec}text{default}` パース | ✓ Phase 2 |
| R-064 | 基盤 | `cursor_count` バッジ (`FINAL_FG+REVERSE` 検出) | ✓ Phase 2 |

### 3.8 UI オプション・リフレッシュ / クリップボード / スクロール

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-070, R-071 | レンダラ | 状態反映と再描画 | ✓ Phase 1 |
| R-080〜R-082 | レンダラ | システムクリップボード API 直接アクセス (`arboard`) | ✓ Phase 3 |
| R-090〜R-093 | レンダラ | スクロール計算の独自実装 (スムーズスクロール + `PageUp` / `PageDown`) | ✓ Phase 3 |

## 4. 拡張基盤の実証・エコシステム目標トレーサビリティ

> 上流にブロックされている項目の詳細は
> [upstream-dependencies.md](./upstream-dependencies.md) を参照。

E 系は本体の必須要求ではなく、Kasane の拡張基盤で実現可能であることを示す実証対象およびエコシステム目標を追跡する。

| ID | 解決層 | 備考 | 状態 | Phase |
|----|--------|------|------|-------|
| E-001 | 基盤 | `Slot::Overlay` + `Decorator(Buffer)` | ○ 部分実証 (`color_preview`) | [上流依存](./upstream-dependencies.md) (完全版) |
| E-002 | 基盤 + プロトコル制約 | `Slot::BufferLeft`。`widget_columns` は利用可能、semantic type は PR #4707 / #4687 依存 | ○ 部分実証 (`color_preview`) | [上流依存](./upstream-dependencies.md) |
| E-003 | 基盤 | `Decorator(Buffer)`。GUI バックエンドでサブピクセル描画 |  | 5c (外部プラグイン) |
| E-004 | 基盤 | `Interactive Element` でヒットテスト | ○ 部分実証 (`color_preview`) | 5c (外部プラグイン) |
| E-005 | 基盤 | `OverlayAnchor::Absolute` | ○ インフラ実装済み (プラグイン実証は未) | 4b (プラグイン実証) |
| E-006 | 基盤 | `Decorator(BufferLine)` | ○ 部分実証 (`cursor_line`) | 4b |
| E-010 | 基盤 | `Flex` による分割レイアウト |  | 5a |
| E-011 | 基盤 | `Slot::Overlay` |  | 5a |
| E-012 | 設定 + 基盤 | セマンティックスタイルトークン |  | 5a |
| E-020 | 基盤 + プロトコル制約 | `Slot::BufferRight`。スクロール位置はプロトコル外 |  | [上流依存](./upstream-dependencies.md) |
| E-021 | 基盤 + プロトコル制約 | E-020 に依存 |  | [上流依存](./upstream-dependencies.md) |
| E-022 | 基盤 | `Decorator(Buffer)` + `Interactive` |  | 5c (外部プラグイン) |
| E-023 | レンダラ + プロトコル制約 | ビジュアルレイアウト計算。画面外情報は上流依存 |  | 5c (外部プラグイン) |
| E-030 | レンダラ | GUI バックエンド (`winit`) |  | 4b |
| E-031 | レンダラ | 独自 URL 検出 |  | 5c (外部プラグイン) |
| E-040 | レンダラ + 基盤 | Kasane 独自のテキストデコレーション能力。プロトコル忠実描画は R-053 で追跡 | - 候補 | 5c (外部プラグイン候補) |
| E-041 | レンダラ | GUI バックエンド (`glyphon`) |  | 5c (外部プラグイン) |

## 5. 追跡上の注記

- 解決層は「どの仕組みで解決するか」を示す。責務境界は
  [layer-responsibilities.md](./layer-responsibilities.md) が正本。
- Phase の意味と実装順序は [roadmap.md](./roadmap.md) を参照。
- 上流に依存する項目の再統合条件は
  [upstream-dependencies.md](./upstream-dependencies.md) を参照。
- 非機能要件、とくに性能要件の実測と実装状況は
  [performance.md](./performance.md) と
  [performance-benchmarks.md](./performance-benchmarks.md) を参照。

## 6. 関連文書

- [requirements.md](./requirements.md) — 要件本文の正本
- [roadmap.md](./roadmap.md) — Phase と未完了項目
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流依存の追跡
- [layer-responsibilities.md](./layer-responsibilities.md) — 責務境界の判断基準
