# Kasane - 要件定義書

## 1. プロジェクト概要

**プロジェクト名:** Kasane (重ね)

**目的:** プラグイン作成者のための拡張可能な UI 基盤を提供する。Kakoune テキストエディタの JSON UI プロトコルを介し、宣言的 UI (Element ツリー + TEA) により、ターミナル UI では解決できない多数の課題を根本から解消する。機能そのものの提供より、拡張性・設定可能性を重視する。

**設計方針:**
- **拡張性:** Slot / Decorator / Replacement の三段階でプラグインが UI を拡張できる
- **設定可能性:** テーマ・レイアウト・キーバインドをユーザーが設定で変更できる
- **宣言的 UI:** プラグインは「何を表示したいか」を記述し、フレームワークが描画を担当する
- **パフォーマンス:** 差分描画・BufferRef パターンにより 60fps を維持
- **Kakoune 専用:** Kakoune の JSON UI プロトコルに特化した設計。不要な抽象化を行わない
- JSON UI (JSON-RPC 2.0) プロトコルによる Kakoune との通信
- 純粋な JSON UI フロントエンド (特定プラグインに依存しない)

**関連ドキュメント:**
- [技術的意思決定記録](./decisions.md)
- [アーキテクチャ設計書](./architecture.md)
- [宣言的 UI アーキテクチャ設計書](./declarative-ui.md)
- [パフォーマンス分析](./performance.md)
- [実装ロードマップ](./roadmap.md)
- [JSON UI プロトコル仕様](./json-ui-protocol.md)
- [Kakoune Issue 調査報告書](./kakoune-issues-investigation.md)
- [用語集](./glossary.md)

---

## 2. 解決層の分類

各要件は、Kasane のどの層で解決されるかに応じて分類される。この分類により、基盤として設計すべきものとそうでないものの境界を明確にする。

| 解決層 | 説明 | 基盤設計への影響 |
|--------|------|-----------------|
| **レンダラ** | 描画エンジン・入力処理の基本実装で自動的に解決される | 基盤メカニズム不要。正しく実装するだけで解決 |
| **設定** | config.toml / ui_options による設定で解決される | 基盤メカニズムの上に設定インターフェースを構築 |
| **基盤** | Slot / Decorator / Replacement / Element で解決される | プラグイン作者も同じメカニズムを利用可能 |
| **プロトコル制約** | Kakoune プロトコルの制限により完全解決不可 | ヒューリスティック回避。上流への貢献を追跡 |

### 要件と解決層のマッピング

#### 基本レンダリング (R-001〜R-009)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-001 | 基盤 | Element ツリー (BufferRef) で構築 | ✓ Phase 1 |
| R-002 | 基盤 | Element ツリーで構築 | ✓ Phase 1 |
| R-003 | レンダラ | ソフトウェアカーソル描画 | ✓ Phase 1 |
| R-004 | レンダラ | padding_face による描画 | ✓ Phase 1 |
| R-005 | レンダラ | リサイズ検知と resize メッセージ送信 | ✓ Phase 1 |
| R-006 | レンダラ | 24bit RGB 直接描画 | ✓ Phase 1 |
| R-007 | レンダラ | ダブルバッファリング (CellGrid) | ✓ Phase 1 |
| R-008 | レンダラ | unicode-width ベースの幅計算 | ✓ Phase 1 |
| R-009 | レンダラ | プレースホルダグリフの描画 | ✓ Phase 1 |

#### フローティングウィンドウ — 補完メニュー (R-010〜R-016)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-010 | 基盤 | Stack + Overlay で構築 | ✓ Phase 1 |
| R-011 | 基盤 | OverlayAnchor によるスタイル別配置 | ✓ Phase 1 |
| R-012 | 基盤 | MenuState の selected 反映 | ✓ Phase 1 |
| R-013 | 基盤 | MenuState クリアで即時非表示 | ✓ Phase 1 |
| R-014 | 設定 + 基盤 | MenuPlacement (Auto/Above/Below) | ✓ Phase 2 |
| R-015 | 基盤 | build_menu_search_dropdown で垂直ドロップダウン化 | ✓ Phase 2 |
| R-016 | レンダラ | イベントバッチング (recv + try_recv, 安全弁付き) | ✓ Phase 2 |

#### フローティングウィンドウ — 情報ポップアップ (R-020〜R-028)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-020 | 基盤 | Stack + Overlay で構築 | ✓ Phase 1 |
| R-021 | 基盤 | OverlayAnchor + InfoStyle 切り替え | ✓ Phase 1 |
| R-022 | 基盤 | InfoState クリアで即時非表示 | ✓ Phase 1 |
| R-023 | 基盤 | infos: Vec\<InfoState\> + InfoIdentity で同時管理 | ✓ Phase 2 |
| R-024 | 基盤 | scroll_offset + InteractiveId + マウスホイール | ✓ Phase 2 |
| R-025 | 基盤 | compute_pos の &[Rect] 汎化 + カーソル avoid | ✓ Phase 2 |
| R-026 | 設定 + 基盤 | BorderConfig (5 スタイル) + StyleToken::Border | ✓ Phase 2 |
| R-027 | レンダラ | TEA update() でキューイング | — 先送り |
| R-028 | 設定 + 基盤 | StyleToken + Theme + ThemeConfig | ✓ Phase 2 |

#### フローティングウィンドウ — 共通 (R-030〜R-033)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-030 | 基盤 | OverlayAnchor::AnchorPoint | ✓ Phase 1 |
| R-031 | 基盤 | compute_pos のクランプロジック | ✓ Phase 1 |
| R-032 | 基盤 | Stack の描画順序 | ✓ Phase 1 |
| R-033 | 設定 + 基盤 | Container の shadow プロパティ | ✓ Phase 1 |

#### 入力処理 (R-040〜R-047)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-040〜R-045 | レンダラ | crossterm イベント変換 | ✓ Phase 1 |
| R-046 | レンダラ | 選択中スクロールの座標計算 | ✓ Phase 3 |
| R-047 | レンダラ | 右クリックドラッグイベント処理 | ✓ Phase 3 |

#### カーソルレンダリング (R-050〜R-052)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-050 | レンダラ | ソフトウェアレンダリング | — 先送り |
| R-051 | レンダラ | フォーカス追跡 | — 先送り |
| R-052 | 基盤 | Slot または Decorator でインジケータ表示 | — 先送り |

#### ステータスバー (R-060〜R-064)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-060 | 基盤 | Element ツリーで構築 | ✓ Phase 1 |
| R-061 | 設定 | status_at_top による Column 配置順序変更 | ✓ Phase 2 |
| R-062 | レンダラ | ヒューリスティック推定 | — 先送り |
| R-063 | 基盤 | markup.rs で {face_spec}text{default} パース | ✓ Phase 2 |
| R-064 | 基盤 | cursor_count バッジ (FINAL_FG+REVERSE 検出) | ✓ Phase 2 |

#### UIオプション・リフレッシュ (R-070〜R-071), クリップボード (R-080〜R-082), スクロール (R-090〜R-093)

| ID | 解決層 | 備考 | 状態 |
|----|--------|------|------|
| R-070, R-071 | レンダラ | 状態反映と再描画 | ✓ Phase 1 |
| R-080〜R-082 | レンダラ | システムクリップボード API 直接アクセス (arboard) | ✓ Phase 3 |
| R-090〜R-093 | レンダラ | スクロール計算の独自実装 (スムーズスクロール + PageUp/PageDown) | ✓ Phase 3 |

#### 拡張機能 (E-001〜E-041)

| ID | 解決層 | 備考 |
|----|--------|------|
| E-001 | 基盤 | Slot::Overlay + Decorator(Buffer) |
| E-002 | 基盤 + プロトコル制約 | Slot::BufferLeft。行番号は PR #4737 依存 |
| E-003 | 基盤 | Decorator(Buffer)。GUI バックエンドでサブピクセル描画 |
| E-004 | 基盤 | Interactive Element でヒットテスト |
| E-005 | 基盤 | OverlayAnchor::Absolute |
| E-006 | 基盤 | Decorator(BufferLine) |
| E-010 | 基盤 | Flex による分割レイアウト |
| E-011 | 基盤 | Slot::Overlay |
| E-012 | 設定 + 基盤 | セマンティックスタイルトークン |
| E-020 | 基盤 + プロトコル制約 | Slot::BufferRight。スクロール位置はプロトコル外 |
| E-021 | 基盤 + プロトコル制約 | E-020 に依存 |
| E-022 | 基盤 | Decorator(Buffer) + Interactive |
| E-023 | レンダラ + プロトコル制約 | ビジュアルレイアウト計算。画面外情報は上流依存 |
| E-030 | レンダラ | GUI バックエンド (winit) |
| E-031 | レンダラ | 独自 URL 検出 |
| E-040 | レンダラ | アンダーラインスタイル描画 |
| E-041 | レンダラ | GUI バックエンド (cosmic-text) |

---

## 3. 機能要件

### 3.1 基本レンダリング

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-001 | バッファ描画 | `draw` メッセージに基づくメインバッファの描画。Face (fg, bg, underline, attributes) を正確に反映する | — |
| R-002 | ステータスバー描画 | `draw_status` メッセージに基づくプロンプト、コンテンツ、モードラインの描画 | — |
| R-003 | カーソル表示 | ソフトウェアカーソル描画 (ブロック/バー/アンダーライン)。バッファカーソルとプロンプトカーソルの優先制御 | [#1524](https://github.com/mawww/kakoune/issues/1524) |
| R-004 | パディング表示 | バッファ末尾以降の行を `padding_face` で描画 | — |
| R-005 | リサイズ対応 | ウィンドウサイズ変更を検知し、`resize` メッセージを Kakoune に送信。再描画を適切に処理 | — |
| R-006 | True Color 描画 | 24bit RGB カラーを直接描画。ターミナルパレット近似なし | [#3554](https://github.com/mawww/kakoune/issues/3554), [#2842](https://github.com/mawww/kakoune/issues/2842), [#3763](https://github.com/mawww/kakoune/issues/3763) |
| R-007 | ダブルバッファリング | フレーム描画をアトミックに行い、ちらつきを完全に排除 | [#3429](https://github.com/mawww/kakoune/issues/3429), [#4320](https://github.com/mawww/kakoune/issues/4320), [#4317](https://github.com/mawww/kakoune/issues/4317), [#3185](https://github.com/mawww/kakoune/issues/3185) |
| R-008 | Unicode 文字幅計算 | 独自の Unicode テキストレイアウトで CJK/絵文字/ゼロ幅文字の正確な幅計算。libc の `wcwidth()` に依存しない | [#3598](https://github.com/mawww/kakoune/issues/3598), [#4257](https://github.com/mawww/kakoune/issues/4257), [#3059](https://github.com/mawww/kakoune/issues/3059), [#1941](https://github.com/mawww/kakoune/issues/1941) |
| R-009 | 特殊文字の可視化 | ゼロ幅文字 (U+200B 等) と制御文字 (^A, ^M) をプレースホルダグリフで可視表示 | [#3570](https://github.com/mawww/kakoune/issues/3570), [#2936](https://github.com/mawww/kakoune/issues/2936) |

### 3.2 フローティングウィンドウ — 補完メニュー

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-010 | メニュー表示 | `menu_show` メッセージに基づく補完メニューのフローティングウィンドウ表示 | — |
| R-011 | メニュースタイル | `inline`, `prompt`, `search` の各スタイルに応じた表示位置の切り替え | — |
| R-012 | メニュー選択 | `menu_select` メッセージに基づく選択項目のハイライト表示 | — |
| R-013 | メニュー非表示 | `menu_hide` メッセージに基づくメニューの即座の非表示化 | — |
| R-014 | メニュー配置のカスタマイズ | 補完メニューの表示位置を設定可能 (カーソル上/下/サイドバー等)。コード遮蔽を回避 | [#3938](https://github.com/mawww/kakoune/issues/3938) |
| R-015 | 検索補完ドロップダウン | 検索候補をプロンプト行の横並びではなく、垂直ドロップダウンとして表示 | [#2170](https://github.com/mawww/kakoune/issues/2170), [#1531](https://github.com/mawww/kakoune/issues/1531) |
| R-016 | マクロ再生時のフラッシュ抑制 | 高速な UI 更新をバッチ処理し、マクロ再生時の一時的なメニューフラッシュを抑制 | [#1491](https://github.com/mawww/kakoune/issues/1491) |

### 3.3 フローティングウィンドウ — 情報ポップアップ

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-020 | 情報ポップアップ | `info_show` メッセージに基づくドキュメント・ヘルプ情報のフローティング表示 | — |
| R-021 | 情報スタイル | `prompt`, `inline`, `inlineAbove`, `inlineBelow`, `menuDoc`, `modal` の各スタイル対応 | — |
| R-022 | 情報非表示 | `info_hide` メッセージに基づく情報ポップアップの即座の非表示化 | — |
| R-023 | 複数ポップアップ同時表示 | 複数の info ボックスを同時に表示可能。lint エラーと LSP hover 等が互いに上書きしない | [#1516](https://github.com/mawww/kakoune/issues/1516) |
| R-024 | スクロール可能なポップアップ | 長い内容を持つ info ポップアップ内でスクロール (マウスホイール/キーボード) 可能 | [#4043](https://github.com/mawww/kakoune/issues/4043) |
| R-025 | 選択範囲衝突回避 | ポップアップの表示位置がカーソルや選択範囲を遮らないよう自動調整 | [#5398](https://github.com/mawww/kakoune/issues/5398) |
| R-026 | カスタマイズ可能なボーダー | ポップアップのボーダースタイル (色、太さ、角丸、無効化) をカラースキームに連動して設定可能 | [#3944](https://github.com/mawww/kakoune/issues/3944) |
| R-027 | 起動時 info キューイング | 起動時に受信した info メッセージをキューイングし、UI 準備完了後に表示 | [#5294](https://github.com/mawww/kakoune/issues/5294) |
| R-028 | 統一デザインシステム | メニュー・info・キーヒント等の全ポップアップ要素で一貫した視覚デザイン | [#2676](https://github.com/mawww/kakoune/issues/2676) |

### 3.4 フローティングウィンドウ — 共通

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-030 | アンカー追従 | `inline` スタイルのフローティングウィンドウは `anchor` 座標に追従して表示 | — |
| R-031 | 画面境界制御 | フローティングウィンドウが画面境界を超える場合、表示位置を自動調整 | — |
| R-032 | Z軸レイヤー管理 | メニュー、情報ポップアップ、メインバッファの描画順序 (Z-order) を適切に管理 | — |
| R-033 | シャドウ効果 | フローティングウィンドウの下に影を表現し、浮遊感を演出 (オプション) | — |

### 3.5 入力処理

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-040 | キーボード入力 | すべてのキー入力を Kakoune のキーフォーマットに変換して送信 | [#4616](https://github.com/mawww/kakoune/issues/4616), [#4834](https://github.com/mawww/kakoune/issues/4834) |
| R-041 | 修飾キー | Control (`c-`), Alt (`a-`), Shift (`s-`) 修飾キーの正確なパース | — |
| R-042 | 特殊キー | `<ret>`, `<esc>`, `<tab>`, `<backspace>`, `<del>`, ファンクションキー等の対応 | — |
| R-043 | マウスクリック | `mouse_press` / `mouse_release` イベントの送信 (left, middle, right)。正確な座標マッピング | [#4030](https://github.com/mawww/kakoune/issues/4030) |
| R-044 | マウス移動 | `mouse_move` イベントの送信 | — |
| R-045 | スクロール | マウスホイールによるスクロールイベントの送信。スクロール速度の設定可能 | [#4155](https://github.com/mawww/kakoune/issues/4155) |
| R-046 | 選択中スクロール | テキスト選択中のマウスホイールスクロールで選択範囲を正しく拡張 | [#2051](https://github.com/mawww/kakoune/issues/2051) |
| R-047 | 右クリックドラッグ | 右クリックドラッグによる選択範囲の拡張 | [#5339](https://github.com/mawww/kakoune/issues/5339) |

### 3.6 カーソルレンダリング

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-050 | 複数カーソル描画 | 全カーソル (プライマリ/セカンダリ) をソフトウェアレンダリングで描画 | [#5377](https://github.com/mawww/kakoune/issues/5377) |
| R-051 | フォーカス連動カーソル | ウィンドウフォーカス喪失時にカーソルをアウトラインスタイルに切り替え | [#3652](https://github.com/mawww/kakoune/issues/3652) |
| R-052 | 画面外カーソルインジケータ | ビューポート外に存在するカーソル/選択範囲の方向と数をビューポート端に表示 | [#2727](https://github.com/mawww/kakoune/issues/2727), [#5425](https://github.com/mawww/kakoune/issues/5425) |

### 3.7 ステータスバー

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-060 | ステータスバー描画 | `draw_status` に基づくプロンプト、コンテンツ、モードラインの描画 | — |
| R-061 | ステータスバー位置 | ステータスバーの表示位置を上部/下部で設定可能 | [#235](https://github.com/mawww/kakoune/issues/235) |
| R-062 | コンテキスト推定 | `draw_status` の内容からコマンド/検索/情報メッセージをヒューリスティックに区別し、適切な表示形式を選択 | [#5428](https://github.com/mawww/kakoune/issues/5428) |
| R-063 | マークアップレンダリング | ステータスライン内の `{Face}` マークアップ構文をパースしてレンダリング | [#4507](https://github.com/mawww/kakoune/issues/4507) |
| R-064 | カーソル数バッジ | 複数カーソル/選択時にカーソル数をステータスバーに表示 | [#5425](https://github.com/mawww/kakoune/issues/5425) |

### 3.8 UIオプション・リフレッシュ

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-070 | UIオプション受信 | `set_ui_options` メッセージを受信し、レンダリングに反映 | — |
| R-071 | リフレッシュ | `refresh` メッセージに基づく画面再描画 (通常/強制) | — |

### 3.9 クリップボード統合

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-080 | システムクリップボード連携 | システムクリップボード API に直接アクセスし、外部プロセス (xclip/xsel) 不要のコピー/ペースト | [#3935](https://github.com/mawww/kakoune/issues/3935), [#4620](https://github.com/mawww/kakoune/issues/4620) |
| R-081 | 高速ペースト | 外部プロセス起動なしの即時ペースト。大量テキストでも遅延なし | [#1743](https://github.com/mawww/kakoune/issues/1743) |
| R-082 | 特殊文字の正確な処理 | クリップボード内の改行・特殊文字をシェルエスケープの問題なく処理 | [#4497](https://github.com/mawww/kakoune/issues/4497) |

### 3.10 スクロール

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-090 | スムーズスクロール | ピクセル単位のスムーズスクロール / 慣性スクロール (オプション) | [#4028](https://github.com/mawww/kakoune/issues/4028) |
| R-091 | scrolloff の正確な処理 | 高 scrolloff 値での境界条件を正しく処理し、カーソルが先頭/末尾行に到達可能 | [#4027](https://github.com/mawww/kakoune/issues/4027) |
| R-092 | 表示行考慮のページスクロール | ソフトラップされた表示行を正確に考慮した PageUp/PageDown 計算 | [#1517](https://github.com/mawww/kakoune/issues/1517) |
| R-093 | 不要スクロールの抑制 | 対象行がビューポート内にある場合の不要なスクロールを抑制 | [#3951](https://github.com/mawww/kakoune/issues/3951) |

---

## 4. 拡張機能要件

Kasane の宣言的 UI 基盤 (Element ツリー + Slot / Decorator / Replacement) 上で実現する拡張機能。多くはプラグインとして実装され、コアフレームワークの拡張性を実証する。設計の詳細は [declarative-ui.md](./declarative-ui.md) を参照。

### 4.1 仮想テキスト・オーバーレイ

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| E-001 | オーバーレイレイヤー | メインバッファ上に独立した描画レイヤーを重畳。仮想テキストをバッファ変更なしに表示 | [#1813](https://github.com/mawww/kakoune/issues/1813) |
| E-002 | ガターアイコン | 行番号ガターにコードアクション (電球)、エラー/警告、git diff 等のアイコンをネイティブ描画 | [#4387](https://github.com/mawww/kakoune/issues/4387) |
| E-003 | インデントガイド | サブピクセルの薄い縦線でインデントレベルを表示。現在のスコープをハイライト可能 | [#2323](https://github.com/mawww/kakoune/issues/2323), [#3937](https://github.com/mawww/kakoune/issues/3937) |
| E-004 | クリッカブルリンク | info ボックスやバッファ内の URL をクリック可能に。ホバー効果付き | [#4316](https://github.com/mawww/kakoune/issues/4316) |
| E-005 | ビューポート相対オーバーレイ | ビューポート座標に対するオーバーレイ描画 (easymotion ジャンプラベル等) | [#1820](https://github.com/mawww/kakoune/issues/1820) |
| E-006 | 選択範囲の拡張表示 | 改行を含む選択範囲をウィンドウ幅いっぱいまでハイライト | [#1909](https://github.com/mawww/kakoune/issues/1909) |

### 4.2 ウィンドウ管理

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| E-010 | ビルトインスプリット | tmux/WM に依存しない水平/垂直分割。ドラッグ可能な境界、任意レイアウト | [#1363](https://github.com/mawww/kakoune/issues/1363) |
| E-011 | フローティングパネル | fzf/ファイルピッカー等のためのフローティングターミナルパネル | [#3878](https://github.com/mawww/kakoune/issues/3878) |
| E-012 | フォーカス視覚フィードバック | フォーカス/非フォーカスペインの視覚的区別 (減色、ボーダー色変更) | [#3942](https://github.com/mawww/kakoune/issues/3942), [#3652](https://github.com/mawww/kakoune/issues/3652) |

### 4.3 スクロールバー・ミニマップ・ナビゲーション

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| E-020 | スクロールバー | プロポーショナルハンドル付きスクロールバー。クリック/ドラッグ対応 | [#165](https://github.com/mawww/kakoune/issues/165), [PR #5304](https://github.com/mawww/kakoune/pull/5304) |
| E-021 | スクロールバーアノテーション | 検索結果、エラー、選択範囲の位置をスクロールバー上にマーカー表示 | [#2727](https://github.com/mawww/kakoune/issues/2727) |
| E-022 | コード折りたたみ | 表示レベルでの行折りたたみ。ガターの折りたたみアイコン、クリック展開 | [#453](https://github.com/mawww/kakoune/issues/453) |
| E-023 | 表示行ナビゲーション | ソフトラップされた表示行単位でのカーソル移動 (gj/gk 相当) | [#5163](https://github.com/mawww/kakoune/issues/5163), [#1425](https://github.com/mawww/kakoune/issues/1425), [#3649](https://github.com/mawww/kakoune/issues/3649) |

### 4.4 ドラッグ＆ドロップ・URL

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| E-030 | ファイルドラッグ＆ドロップ | GUI ファイルマネージャからのファイルドロップでバッファを開く | [#3928](https://github.com/mawww/kakoune/issues/3928) |
| E-031 | URL 検出 | バッファ内の URL を独自に検出。空白文字表示に影響されない | [#4135](https://github.com/mawww/kakoune/issues/4135) |

### 4.5 フォント・テキストスタイル

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| E-040 | アンダーラインバリエーション | 波線/点線/二重線/色付きアンダーラインの一貫した描画 | [#4138](https://github.com/mawww/kakoune/issues/4138) |
| E-041 | 領域別フォントサイズ | インレイヒントを小さく、見出しを大きく等の領域別フォントサイズ (GUI バックエンド) | [#5295](https://github.com/mawww/kakoune/issues/5295) |

---

## 5. 非機能要件

### 5.1 パフォーマンス

| ID | 要件 | 目標値 | 関連 Issue |
|----|------|--------|-----------|
| NF-001 | 描画レイテンシ | Kakoune からの描画命令受信から画面反映まで 16ms 以下 (60fps 相当) | [#1307](https://github.com/mawww/kakoune/issues/1307) |
| NF-002 | 入力レイテンシ | キー入力から Kakoune への送信まで 1ms 以下 | — |
| NF-003 | メモリ使用量 | 通常使用時のメモリ消費を最小限に抑制 | — |
| NF-004 | 差分描画 | 変更のあったセルのみを再描画する差分レンダリング | — |
| NF-005 | 非同期I/O | Kakoune との通信をノンブロッキングで処理 | — |
| NF-006 | UI 更新バッチ処理 | 高頻度の連続更新 (マクロ再生等) をバッチ処理し、最終フレームのみ描画 | [#1491](https://github.com/mawww/kakoune/issues/1491) |

### 5.2 UI/UX

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| NF-010 | ボーダー描画 | フローティングウィンドウにカスタマイズ可能なボーダーを表示 | [#3944](https://github.com/mawww/kakoune/issues/3944) |
| NF-011 | シャドウ効果 | フローティングウィンドウの下に影を表現し、浮遊感を演出 (オプション) | — |
| NF-012 | ちらつき排除 | ダブルバッファリングにより一切のちらつきなし | [#3429](https://github.com/mawww/kakoune/issues/3429) |
| NF-013 | Unicode対応 | Unicode 文字幅 (全角/半角/絵文字) を正確に計算し、位置合わせを行う | [#3598](https://github.com/mawww/kakoune/issues/3598) |
| NF-014 | True Color | 24bit True Color (RGB) 対応。ターミナルパレット非依存 | [#3554](https://github.com/mawww/kakoune/issues/3554) |
| NF-015 | Kakoune互換性 | 標準ターミナル UI と同等の操作感を維持 | — |
| NF-016 | ターミナル非依存 | ターミナルエスケープシーケンス・terminfo に一切依存しない | [#4079](https://github.com/mawww/kakoune/issues/4079), [#3705](https://github.com/mawww/kakoune/issues/3705), [#4260](https://github.com/mawww/kakoune/issues/4260) |
| NF-017 | ポップアップ統一デザイン | 全ポップアップ要素 (メニュー/info/キーヒント) で統一されたデザイン言語 | [#2676](https://github.com/mawww/kakoune/issues/2676) |

---

## 6. 既知の制約事項

| ID | 制約 | 影響 | 回避策 |
|----|------|------|--------|
| C-001 | クリップボード統合なし | プロトコルにクリップボードイベントが存在しない | フロントエンド側でシステムクリップボード API に直接アクセス (R-080) |
| C-002 | 文字幅情報なし | Atom に表示幅情報が含まれない | フロントエンド側で独自の Unicode 幅計算を実装 (R-008) |
| C-003 | オプション変更通知なし | Kakoune 側のオプション変更がリアルタイムで通知されない | `set_ui_options` の定期的なポーリングまたは `refresh` 契機での再取得 |
| C-004 | マウス修飾キー非対応 | マウスイベントに Ctrl/Alt 修飾キーを付与できない | Ctrl+クリック等はフロントエンド側で独自処理 |
| C-005 | 位置パラメータのみ | JSON-RPC は位置パラメータのみ対応 | パーサーで位置パラメータを正確にハンドリング |
| C-006 | ステータスラインコンテキスト不明 | コマンド/検索/メッセージの区別不可 | ヒューリスティック推定 (R-062)。上流 [#5428](https://github.com/mawww/kakoune/issues/5428) の解決を追跡 |
| C-007 | インクリメンタル draw なし | 毎回全表示行が送信される | フロントエンド側で差分検出 (NF-004)。上流 [#4686](https://github.com/mawww/kakoune/issues/4686) の解決を追跡 |
| C-008 | Atom の種別不明 | 行番号/仮想テキスト/コードを区別できない | Face 名ベースのヒューリスティック。上流 [#4687](https://github.com/mawww/kakoune/issues/4687), [PR #4707](https://github.com/mawww/kakoune/pull/4707) を追跡 |
