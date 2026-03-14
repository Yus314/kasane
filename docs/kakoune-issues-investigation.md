# Kakoune Issue/PR 調査報告書 — Kasane で解決可能な課題

## 調査概要

Kakoune (mawww/kakoune) の GitHub Issue/PR を調査し、Kasane (カスタム JSON UI フロントエンド) で解決・改善可能な課題を特定した。約100件以上の Issue/PR を分析し、以下にカテゴリ別に整理する。

> **注:** 本ドキュメントで特定された課題の多くは、Kasane の現行 UI 基盤と拡張モデルを通じて、プラグインまたはコア実装で解決される。[plugin-development.md](./plugin-development.md)、[plugin-api.md](./plugin-api.md)、[semantics.md](./semantics.md) を参照。

---

## 1. フローティングウィンドウ・ポップアップ (最重要カテゴリ)

Kasane のコア機能であるフローティングウィンドウで直接解決できる課題群。

### 1.1 情報ポップアップの制限

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#1516](https://github.com/mawww/kakoune/issues/1516) | 複数 info ボックスの同時表示 | OPEN | info ボックスは同時に1つしか表示できない。lint エラーと LSP hover が互いに上書きし合う |
| [#4043](https://github.com/mawww/kakoune/issues/4043) | スクロール可能な info ボックス | OPEN | LSP hover ドキュメントが長い場合に切り捨てられる。スクロール手段がない |
| [#5398](https://github.com/mawww/kakoune/issues/5398) | ポップアップが選択範囲を遮る | OPEN | info ポップアップが選択範囲を覆い隠し、何が選択されているか見えない |
| [#5294](https://github.com/mawww/kakoune/issues/5294) | 起動時に info を表示できない | OPEN | kakrc / KakBegin フックで `info -style modal` が無視される |
| [#3944](https://github.com/mawww/kakoune/issues/3944) | info ウィンドウのボーダーが配色と衝突 | OPEN | ボーダー色の変更や無効化ができない |
| [#2676](https://github.com/mawww/kakoune/issues/2676) | メニューと info の視覚的不統一 | OPEN | ユーザーモードのキーヒントとコマンドモードメニューの見た目が異なる |

**Kasane での解決策 (宣言的 UI):**
- Element の `Stack` + `Overlay` で複数フローティングウィンドウを同時描画し、Z軸レイヤー管理
- `Scrollable` Element でスクロール可能なポップアップを実現
- `OverlayAnchor::AnchorPoint` の衝突回避ロジック (avoid) で選択範囲との重なりを防止
- TEA の `update()` で起動時 info メッセージをキューイング
- `Container` Element の border/shadow プロパティ + セマンティックスタイルトークンで統一デザイン
- プラグインは `Replacement(InfoPrompt)` で情報ポップアップの表示を完全カスタマイズ可能

### 1.2 補完メニューの制限

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#3938](https://github.com/mawww/kakoune/issues/3938) | 補完メニューの表示位置変更 | OPEN | カーソル下のコードを覆い隠す。別の場所に表示したい |
| [#4396](https://github.com/mawww/kakoune/issues/4396) | `:menu` 候補のフィルタリング | OPEN | コードアクション等の多数の候補をファジー検索で絞り込めない |
| [#5068](https://github.com/mawww/kakoune/issues/5068) | 補完プレビュー (バッファ未書き込み) | OPEN | 補完候補を選択するとバッファに書き込まれてしまう。プレビューのみしたい |
| [#5277](https://github.com/mawww/kakoune/issues/5277) | 補完リスト最初の項目の自動選択 | OPEN | 最初の候補を自動選択するオプションがない |
| [#5410](https://github.com/mawww/kakoune/issues/5410) | プロンプト補完を無視する方法 | OPEN | プロンプト入力時に補完を無視できない |
| [#1491](https://github.com/mawww/kakoune/issues/1491) | マクロ実行時の補完メニューフラッシュ | OPEN | マクロ再生時にメニューが一瞬表示される |
| [#2170](https://github.com/mawww/kakoune/issues/2170) | 検索補完をドロップダウン表示 | OPEN | 検索候補がプロンプト行に横並びで分かりにくい |
| [#1531](https://github.com/mawww/kakoune/issues/1531) | 補完を画面下部に水平表示 | OPEN | コマンドライン補完のように水平表示したい |

**Kasane での解決策 (宣言的 UI):**
- `OverlayAnchor` の設定で補完メニューの表示位置を自由に変更可能
- プラグインが `Replacement(MenuPrompt/MenuInline/MenuSearch)` でメニューを fzf 風等に完全差替可能
- `Slot::Overlay` にゴーストテキスト Element を挿入して補完プレビュー表示
- TEA の `update()` 内で設定を参照し、自動選択・補完無視モードを制御
- イベントバッチング (try_recv) でマクロ再生中のポップアップフラッシュを抑制
- `Grid` Element でドロップダウン/水平切り替え可能な補完表示

---

## 2. レンダリング・表示品質

ターミナル依存に起因する描画問題で、独自レンダリングにより根本解決できるもの。

### 2.1 ちらつき・再描画問題

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#3429](https://github.com/mawww/kakoune/issues/3429) | 画面のちらつき | CLOSED | 相対行番号使用時、差分スクロール最適化で中間状態が見える |
| [#4320](https://github.com/mawww/kakoune/issues/4320) | ちらつき (行の位置ずれ) | CLOSED | 同期出力の検出バグによる誤作動 |
| [#4317](https://github.com/mawww/kakoune/issues/4317) | Linux コンソールでの視覚的不具合 | CLOSED | Linux コンソールは中間レンダリング状態をすべて表示する |
| [#3185](https://github.com/mawww/kakoune/issues/3185) | st ターミナルでの不整合な再描画 | OPEN | terminfo データベースの不一致による描画問題 |
| [#4689](https://github.com/mawww/kakoune/issues/4689) | aerc 内での再描画問題 | OPEN | 別アプリ内埋め込み時のターミナル互換性問題 |

**Kasane での解決策:**
- ダブルバッファリングによるアトミックなフレーム描画。中間状態は一切表示されない
- ターミナルエスケープシーケンスへの依存を完全に排除
- terminfo / 同期出力プロトコルが不要

### 2.2 色・カラースキーム問題

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#3554](https://github.com/mawww/kakoune/issues/3554) | デフォルトテーマのコントラストが悪い | OPEN | ターミナルパレットによって見た目が大きく変わる |
| [#2842](https://github.com/mawww/kakoune/issues/2842) | Solarized でテキストマークアップが壊れる | OPEN | bold-as-bright の動作により色の不一致が発生 |
| [#4193](https://github.com/mawww/kakoune/issues/4193) | tmux + solarized で空白画面 | OPEN | tmux のカラー設定に依存 |
| [#3763](https://github.com/mawww/kakoune/issues/3763) | 色の誤算出 | CLOSED | True Color 非対応時の 256 色近似 |

**Kasane での解決策:**
- ネイティブ 24bit RGB カラーレンダリング。パレット近似不要
- bold と色を完全に独立して処理 (bold-as-bright 問題は発生しない)
- tmux 等のマルチプレクサに依存しない直接描画
- 一貫したカラーレンダリングを保証するデフォルトテーマを同梱

### 2.3 Unicode / CJK / 絵文字の表示問題

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#3598](https://github.com/mawww/kakoune/issues/3598) | CJK 文字の補完候補表示崩れ | OPEN | ダブル幅文字とポップアップの重なりで描画崩壊 |
| [#4257](https://github.com/mawww/kakoune/issues/4257) | macOS で絵文字がほぼ動作しない | OPEN | `iswprint()` の Unicode データベースが古い |
| [#3059](https://github.com/mawww/kakoune/issues/3059) | 絵文字サポート | CLOSED | 同上。Kakoune はシステムの libc に依存 |
| [#1941](https://github.com/mawww/kakoune/issues/1941) | CJK 幅でスクロールバーと info 領域が崩れる | OPEN | 文字幅計算のずれによるレイアウト崩壊 |
| [#3570](https://github.com/mawww/kakoune/issues/3570) | ゼロ幅文字が不可視 | OPEN | U+200B 等が見えないがカーソル移動に影響 |
| [#2936](https://github.com/mawww/kakoune/issues/2936) | 制御文字を ^A, ^M として表示 | OPEN | 制御文字が不可視で識別困難 |
| [#3364](https://github.com/mawww/kakoune/issues/3364) | UTF-8 レンダリング破損 | OPEN | ターミナルエンコーディング問題による文字化け |

**Kasane での解決策:**
- 独自の Unicode テキストレイアウトライブラリで正確な文字幅計算
- システムフォントフォールバックチェーンによる絵文字の正常表示
- libc の `iswprint()` / `wcwidth()` に依存しない
- ゼロ幅文字・制御文字の可視化表示 (プレースホルダグリフ)
- JSON (UTF-8) 通信によるエンドツーエンドの文字データ完全性

### 2.4 カーソルレンダリング

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#3652](https://github.com/mawww/kakoune/issues/3652) | 非アクティブ時のカーソル変化なし | OPEN | フォーカス喪失時のカーソルスタイル変更がない |
| [#5377](https://github.com/mawww/kakoune/issues/5377) | Kitty マルチカーソルプロトコル | OPEN | ネイティブカーソルが UI ウィジェットに重なる問題 |
| [#1524](https://github.com/mawww/kakoune/issues/1524) | カーソルのちらつき | CLOSED | 描画更新中にハードウェアカーソルがランダム位置に表示 |
| [#2727](https://github.com/mawww/kakoune/issues/2727) | 画面外カーソルの表示 | CLOSED | 画面外の選択範囲を忘れてファイルを破壊 |

**Kasane での解決策:**
- ソフトウェアカーソル描画 (ブロック/バー/アンダーライン/アウトライン)
- フォーカス追跡によるアクティブ/非アクティブカーソルの自動切り替え
- 複数カーソルのネイティブレンダリング (ターミナルプロトコル不要)
- ビューポート端に画面外選択のインジケータを表示

---

## 3. ターミナル互換性問題

Kasane が独自レンダリングを行うことで、カテゴリ丸ごと解消される問題群。

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#4079](https://github.com/mawww/kakoune/issues/4079) | Terminal.app が同期出力を無視 | CLOSED | DCS シーケンスがテキストとして表示される |
| [#3705](https://github.com/mawww/kakoune/issues/3705) | PuTTY での画面破損 | CLOSED | DCS 非対応 |
| [#4260](https://github.com/mawww/kakoune/issues/4260) | tmux がイタリックを異なる解釈 | CLOSED | エスケープシーケンスの解釈差 |
| [#4616](https://github.com/mawww/kakoune/issues/4616) | Xterm で Backspace が認識されない | OPEN | キーコード差異 |
| [#4834](https://github.com/mawww/kakoune/issues/4834) | WezTerm で Shift-Tab が動作しない | OPEN | キーコード差異 |
| [#1307](https://github.com/mawww/kakoune/issues/1307) | iTerm2 で Kakoune が遅い | OPEN | ターミナルエミュレータのオーバーヘッド |
| [#5333](https://github.com/mawww/kakoune/issues/5333) | GNU Screen でのレンダリング | CLOSED | Screen 5.0 未満は True Color 非対応 |

**Kasane での解決策:**
- ターミナルエスケープシーケンスを一切使用しないため、全問題が自動的に解消
- キーボード入力をウィンドウシステムから直接取得 (ターミナルキーコード変換不要)
- ターミナルエミュレータのレンダリングオーバーヘッドを排除

---

## 4. ウィンドウ管理・レイアウト

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#1363](https://github.com/mawww/kakoune/issues/1363) | ポータブルな水平/垂直分割 | OPEN | tmux/WM に依存しない分割コマンド。29コメントの高需要 |
| [#3878](https://github.com/mawww/kakoune/issues/3878) | tmux ポップアップのサポート | OPEN | fzf 等のためのフローティングターミナル |
| [#3942](https://github.com/mawww/kakoune/issues/3942) | tmux でのフォーカス/非フォーカスの区別なし | CLOSED | 複数クライアント時にどれがアクティブか分からない |

**Kasane での解決策 (宣言的 UI):**
- `Flex` Element でビルトインのスプリット/ペインシステムを構築 (ドラッグ可能な `Interactive` 境界)
- `Slot::Overlay` にフローティングパネルプラグイン (ファイルピッカー, ターミナル) を配置
- セマンティックスタイルトークンでフォーカス/非フォーカスの視覚的区別

---

## 5. 仮想テキスト・オーバーレイ

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#1813](https://github.com/mawww/kakoune/issues/1813) | ウィンドウ内の仮想テキスト | OPEN | LSP コードレンズ, インレイヒント, インライン診断。11コメント |
| [#5382](https://github.com/mawww/kakoune/issues/5382) | replace-ranges で仮想改行を挿入 | OPEN | 行の下にインライン診断を表示できない |
| [#4387](https://github.com/mawww/kakoune/issues/4387) | コードアクションインジケータ (電球) | OPEN | matklad (rust-analyzer 開発者) による提案。10コメント |
| [#2323](https://github.com/mawww/kakoune/issues/2323) | インデントガイド | CLOSED | ターミナルでは薄い縦線が描画不可能。21コメント |
| [#3937](https://github.com/mawww/kakoune/issues/3937) | インデントガイドライン | OPEN | コミュニティ調査からの要望 |
| [#4316](https://github.com/mawww/kakoune/issues/4316) | クリッカブルリンク (OSC 8) | OPEN | info ボックスやドキュメント内の URL をクリック可能にしたい |
| [#1820](https://github.com/mawww/kakoune/issues/1820) | ウィンドウ相対のハイライト | OPEN | easymotion 等のオーバーレイ機能の実装に必要 |
| [#1909](https://github.com/mawww/kakoune/issues/1909) | 選択範囲を行末まで拡張表示 | OPEN | 改行文字を含む選択が見づらい |

**Kasane での解決策 (宣言的 UI):**
- `Slot::Overlay` + `Stack` Element で仮想テキストをバッファ上に重畳描画
- プラグインが `Decorator(Buffer)` でコードレンズ・インレイ型注釈レイヤーを追加
- `Slot::BufferLeft` にガターアイコンプラグイン (電球, エラー/警告, git diff) を挿入
- GUI バックエンドでサブピクセルのインデントガイドライン描画
- `Interactive` Element でクリッカブルハイパーリンク (InteractiveId によるヒットテスト)
- `OverlayAnchor::Absolute` でビューポート相対のオーバーレイ (easymotion 等)
- `Decorator(BufferLine)` で選択範囲のウィンドウ幅拡張表示

---

## 6. スクロール動作

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#4028](https://github.com/mawww/kakoune/issues/4028) | 高 scrolloff でマウススクロールが不均一 | OPEN | スクロール量が不整合 |
| [#4027](https://github.com/mawww/kakoune/issues/4027) | 高 scrolloff でカーソルが先頭行に到達不可 | OPEN | 境界条件バグ |
| [#4030](https://github.com/mawww/kakoune/issues/4030) | 高 scrolloff + マウスクリックで行ずれ | OPEN | クリック座標がずれる |
| [#4155](https://github.com/mawww/kakoune/issues/4155) | マウス無効時にスクロールが Up/Down キーに | OPEN | イベントの誤変換 |
| [#3951](https://github.com/mawww/kakoune/issues/3951) | 対象行が表示中なのにスクロール | OPEN | 不要なスクロールが発生 |
| [#1517](https://github.com/mawww/kakoune/issues/1517) | 折り返し行で PageUp が機能しない | OPEN | 表示行を考慮しないスクロール量計算 |

**Kasane での解決策:**
- ビューポートスクロールとカーソル移動を独立制御
- ピクセル単位のスムーズスクロール / 慣性スクロール
- 正確なマウス座標→バッファ位置マッピング
- 表示行を正確に考慮したページスクロール計算

---

## 7. マウス操作

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#2051](https://github.com/mawww/kakoune/issues/2051) | テキスト選択中にスクロール不可 | OPEN | スクロールすると選択が壊れる |
| [#5339](https://github.com/mawww/kakoune/issues/5339) | 右クリックドラッグで選択拡張 | OPEN | 右クリックダウンは機能するがドラッグは無反応 |
| [#4135](https://github.com/mawww/kakoune/issues/4135) | 空白表示が URL クリック検出を妨害 | OPEN | `·` 文字がターミナルの URL 検出を破壊 |
| [#3928](https://github.com/mawww/kakoune/issues/3928) | ドラッグ＆ドロップサポート | OPEN | ファイルマネージャからのファイルドロップ |

**Kasane での解決策:**
- ドラッグ中のスクロールで選択範囲を正しく拡張
- 右クリックドラッグによる選択拡張の完全実装
- 独自の URL 検出 (空白表示に影響されない)
- ネイティブのドラッグ＆ドロップ対応

---

## 8. クリップボード統合

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#3935](https://github.com/mawww/kakoune/issues/3935) | ビルトインクリップボード統合 | OPEN | xclip/xsel への依存を排除したい |
| [#4620](https://github.com/mawww/kakoune/issues/4620) | OSC 52 ネイティブサポート | OPEN | 貼り付けが機能しない |
| [#4497](https://github.com/mawww/kakoune/issues/4497) | クリップボードの改行・特殊文字 | OPEN | シェルコマンド経由のエスケープ問題 |
| [#1743](https://github.com/mawww/kakoune/issues/1743) | X11 クリップボードからの貼り付けが遅い | OPEN | 外部プロセス起動のオーバーヘッド |

**Kasane での解決策:**
- システムクリップボード API への直接アクセス
- 外部プロセス起動なしの即時コピー/ペースト
- Unicode/バイナリデータの正確なクリップボード処理

---

## 9. ステータスライン・モードライン

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#5428](https://github.com/mawww/kakoune/issues/5428) | JSON UI でステータスラインのコンテキスト区別不可 | OPEN | コマンド/検索/情報メッセージの区別ができない |
| [#4445](https://github.com/mawww/kakoune/issues/4445) | ステータスラインのカスタマイズ | OPEN | 個別コンポーネント (モード, 選択数等) へのアクセスが限定的 |
| [#4507](https://github.com/mawww/kakoune/issues/4507) | モードラインでマークアップが解析されない | OPEN | `{green}text` がリテラル表示される |
| [#5425](https://github.com/mawww/kakoune/issues/5425) | カーソル数インジケータ | CLOSED | 複数カーソル状態の可視化 |
| [#235](https://github.com/mawww/kakoune/issues/235) | ステータスラインを上部に配置 | CLOSED | 位置がハードコード |

**Kasane での解決策 (宣言的 UI):**
- `Replacement(StatusBar)` でステータスバーを完全カスタマイズ可能 (位置、レイアウト、ウィジェット)
- `Slot::StatusLeft` / `Slot::StatusRight` にプラグインがウィジェットを挿入
- `Decorator(StatusBar)` でマークアップのパース・レンダリングを追加
- `Slot::AboveStatus` にコマンドパレット / 通知エリアを分離配置

---

## 10. ソフトラップ・表示行ナビゲーション

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#5163](https://github.com/mawww/kakoune/issues/5163) | ソフトラップテキストの上下ナビゲーション | OPEN | vim の `gj`/`gk` 相当がない。21コメント |
| [#1425](https://github.com/mawww/kakoune/issues/1425) | 表示行単位の移動 | OPEN | mawww が設計課題を指摘 (画面外の複数選択) |
| [#3649](https://github.com/mawww/kakoune/issues/3649) | ソフトラップテキストのカーソルナビゲーション | OPEN | 散文編集で必要 |
| [#5328](https://github.com/mawww/kakoune/issues/5328) | buffer_display_width / split_line の公開 | OPEN | 表示行ナビゲーションをスクリプトで実装するため |

**Kasane での解決策:**
- Kasane は正確なビジュアルレイアウトを把握しているため、表示行座標をバッファ座標に変換して `gj`/`gk` を実装可能
- ただし画面外の複数選択に対するラッピング情報は Kakoune 側との連携が必要

---

## 11. コード折りたたみ・スクロールバー・ミニマップ

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#453](https://github.com/mawww/kakoune/issues/453) | コード折りたたみ | OPEN | 2016年からの要望。27コメント |
| [#165](https://github.com/mawww/kakoune/issues/165) | スクロールバーの追加 | CLOSED | テキストモードスクロールバーの要望 |
| [#4014](https://github.com/mawww/kakoune/issues/4014) | ディレクトリ探索機能 | OPEN | netrw のようなファイルブラウザ |

**Kasane での解決策 (宣言的 UI):**
- `Decorator(Buffer)` で表示レベルの行折りたたみプラグインを実装 (ガターの `Interactive` アイコン)
- `Slot::BufferRight` にスクロールバープラグイン (`Scrollable` + アノテーションマーカー)
- `Slot::BufferRight` にミニマッププラグインを配置
- `Slot::BufferLeft` または `Slot::Overlay` にファイルツリー / ファジーファインダープラグイン

---

## 12. フォントレンダリング・テキストサイズ

| Issue | タイトル | 状態 | 概要 |
|-------|---------|------|------|
| [#5295](https://github.com/mawww/kakoune/issues/5295) | Kitty text-sizing プロトコル | OPEN | 領域ごとにフォントサイズを変更したい |
| [#4138](https://github.com/mawww/kakoune/issues/4138) | アンダーラインバリエーション | CLOSED | 波線/点線/二重線。ターミナルの対応が不安定 |
| [#3946](https://github.com/mawww/kakoune/issues/3946) | Right-to-Left テキストサポート | OPEN | RTL テキスト表示 |

**Kasane での解決策:**
- 領域別フォントサイズ (見出し大きく、インレイヒント小さく)
- 全アンダーラインスタイルの一貫した描画 (ターミナル対応不要)
- BiDi テキストレンダリング (将来的な拡張)

---

## 13. JSON UI プロトコルの拡張提案 (上流への貢献)

Kasane の実装と並行して、上流の Kakoune に提案すべきプロトコル改善。各制約の詳細な影響分析は [Kakoune プロトコル制約分析](./kakoune-protocol-constraints.md) を参照。

| PR/Issue | タイトル | 状態 | 概要 |
|----------|---------|------|------|
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | JSON UI に Face 名を追加 | OPEN | 意味的な Face 名 (PrimaryCursor 等) をフロントエンドに送信 |
| [PR #4737](https://github.com/mawww/kakoune/pull/4737) | draw メッセージに DisplaySetup コンテキスト追加 | OPEN | バッファ座標、カーソル位置、ウィジェット列数 |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | インクリメンタル draw 通知 | OPEN | 変更行のみの差分送信 |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | コードと仮想テキスト/行番号の区別 | OPEN | Atom の種類を区別可能にする |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | ステータスラインコンテキストの追加 | OPEN | `status_style` パラメータの追加 |
| [#2019](https://github.com/mawww/kakoune/issues/2019) | JSON UI の制限事項まとめ | OPEN | クリップボード, 文字幅, コマンド実行等 |

**Kasane の戦略:**
- まずは現行プロトコルで動作する実装を完成させる
- ヒューリスティックな回避策で制限に対応
- 上流 PR のレビュー・フィードバックに参加
- 必要に応じて新しい PR を提出

---

## 優先度ランキング

ユーザー需要 (コメント数、再要望頻度) と Kasane での実現容易性を総合評価。

### Tier 1 — Kasane のコア価値 (直接的な差別化要因)

| 順位 | カテゴリ | 代表 Issue | コメント数 |
|------|---------|-----------|-----------|
| 1 | フローティングウィンドウ (メニュー/info) | #1516, #4043, #3938, #5398 | 多数 |
| 2 | ちらつき/再描画の根絶 | #3429, #4320, #3185 | — |
| 3 | Unicode/CJK/絵文字の正常表示 | #3598, #4257, #3059 | 多数 |
| 4 | True Color の一貫した表示 | #3554, #2842 | 16+ |
| 5 | ターミナル互換性問題の全面解消 | #4079, #3705, #4616 等 | — |

### Tier 2 — 高需要の機能拡張

| 順位 | カテゴリ | 代表 Issue | コメント数 |
|------|---------|-----------|-----------|
| 6 | ビルトイン分割管理 | #1363 | 29 |
| 7 | コード折りたたみ | #453 | 27 |
| 8 | 表示行ナビゲーション | #5163, #1425 | 21 |
| 9 | インデントガイド | #2323 | 21 |
| 10 | クリップボード統合 | #3935, #4620, #1743 | 多数 |

### Tier 3 — UX 向上

| 順位 | カテゴリ | 代表 Issue | コメント数 |
|------|---------|-----------|-----------|
| 11 | 仮想テキスト/コードレンズ | #1813, #4387 | 11, 10 |
| 12 | ステータスラインカスタマイズ | #4445, #5428 | 7 |
| 13 | スクロール動作改善 | #4028, #4027, #1517 | — |
| 14 | マウス操作改善 | #2051, #5339, #3928 | — |
| 15 | スクロールバー/ミニマップ | #165, PR #5304 | — |
| 16 | カーソルレンダリング強化 | #3652, #5377, #2727 | — |
| 17 | フォントサイズ/アンダーライン | #5295, #4138 | — |

---

## 既存の代替フロントエンドプロジェクト

| プロジェクト | 技術 | 状態 | 特徴 |
|-------------|------|------|------|
| [kakoune-gtk](https://gitlab.com/Screwtapello/kakoune-gtk) | GTK | PoC | #2019 の Issue を生み出した先駆者 |
| [kakoune-electron](https://github.com/Delapouite/kakoune-electron) | Electron/Canvas | 実験的 | Canvas レンダリング |
| [Kakoune Qt](https://discuss.kakoune.com/t/announcing-kakoune-qt/2522) | Qt | アクティブ (2024) | 分割、ボーダー、マルチフォントサイズ |
| [kakoune-arcan](https://github.com/cipharius/kakoune-arcan) | Arcan/Zig | 実験的 | Arcan ディスプレイサーバーフロントエンド |
| [kak-ui](https://docs.rs/kak-ui/latest/kak_ui/) | Rust crate | 公開済 | JSON-RPC プロトコルの Rust ラッパー |
