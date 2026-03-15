# Kasane - 要件定義書

本ドキュメントは、Kasane が満たすべき要件本文の正本である。
実装状態、Phase、上流依存の追跡は [requirements-traceability.md](./requirements-traceability.md) を参照。

## 1. プロジェクト概要

**プロジェクト名:** Kasane (重ね)

**目的:** Kakoune テキストエディタの JSON UI プロトコルを介して、拡張可能で高性能なフロントエンド UI 基盤を提供する。Kasane は機能そのものの提供より、拡張性・設定可能性・意味的一貫性を重視し、標準ターミナル UI では実現しにくい表示と操作を可能にする。

**設計方針:**
- **拡張可能性:** プラグインが UI への寄与、装飾、重畳、変換、独自領域の提供を通じてフロントエンドを拡張できる
- **設定可能性:** テーマ、レイアウト、キーバインド、表示ポリシーをユーザーが設定で変更できる
- **高性能:** 高頻度更新下でも実用的な応答性と滑らかな描画を維持する
- **意味的一貫性:** バックエンドが異なっても同じ状態に対して同じ意味の UI を表示する
- **導入容易性:** 既存の Kakoune 利用者が大きな設定変更や運用変更なしに採用できることを重視する
- **互換性優先:** `kakrc`、autoload、既存プラグイン、既存セッション運用と整合的に動作することを標準動作として重視する
- **保守的デフォルト:** デフォルト状態では Kakoune 既存の利用感を不必要に逸脱しない
- **拡張の任意性:** Kasane 独自の高度機能やプラグインは追加価値であり、通常利用の前提条件とはしない
- **Kakoune 専用:** Kakoune の JSON UI プロトコルに特化した設計。不要な抽象化を行わない
- JSON UI (JSON-RPC 2.0) プロトコルによる Kakoune との通信
- 純粋な JSON UI フロントエンド (特定プラグインに依存しない)

**補助ドキュメント:**
- [要件トレーサビリティ](./requirements-traceability.md) — 解決層、状態、Phase、上流依存
- [現行意味論](./semantics.md) — 状態、レンダリング、再描画ポリシー、拡張性の規範
- [実装ロードマップ](./roadmap.md) — 実装順序と今後の段階
- [上流依存項目](./upstream-dependencies.md) — 上流ブロッカーと再統合条件

---

## 2. コア機能要件

本章は、Kasane が JSON UI フロントエンドとして直接提供し、標準動作として保証すべき機能を定義する。ここでいうコア機能要件には、描画、入力、標準 UI、状態反映、標準スタイル体系など、Kasane 本体が実装責任を持つ能力のみを含む。外部プラグインで実現可能な具体機能や、Kasane の基盤が可能にする応用例は本章には含めず、[3. 拡張基盤要件](#3-拡張基盤要件) および [4. 実証対象・代表ユースケース](#4-実証対象代表ユースケース) で扱う。上流情報不足により完全保証できない項目や、ヒューリスティックに依存する縮退動作は [6. 上流依存・縮退動作](#6-上流依存縮退動作) で扱う。

### 2.1 基本レンダリング

Kasane は、Kakoune から観測される描画事実を、正確かつ安定して画面へ反映するための基本描画能力を提供する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-001 | バッファ描画 | `draw` メッセージに基づくメインバッファの描画。Face (fg, bg, underline, attributes) を正確に反映する | — |
| R-003 | カーソル表示 | ソフトウェアカーソル描画 (ブロック/バー/アンダーライン)。バッファカーソルとプロンプトカーソルの優先制御 | [#1524](https://github.com/mawww/kakoune/issues/1524) |
| R-004 | パディング表示 | バッファ末尾以降の行を `padding_face` で描画 | — |
| R-005 | リサイズ対応 | ウィンドウサイズ変更を検知し、`resize` メッセージを Kakoune に送信。再描画を適切に処理 | — |
| R-006 | True Color 描画 | 24bit RGB カラーを直接描画。ターミナルパレット近似なし | [#3554](https://github.com/mawww/kakoune/issues/3554), [#2842](https://github.com/mawww/kakoune/issues/2842), [#3763](https://github.com/mawww/kakoune/issues/3763) |
| R-007 | ダブルバッファリング | フレーム描画をアトミックに行い、ちらつきを完全に排除 | [#3429](https://github.com/mawww/kakoune/issues/3429), [#4320](https://github.com/mawww/kakoune/issues/4320), [#4317](https://github.com/mawww/kakoune/issues/4317), [#3185](https://github.com/mawww/kakoune/issues/3185) |
| R-008 | Unicode 文字幅計算 | 独自の Unicode テキストレイアウトで CJK/絵文字/ゼロ幅文字の正確な幅計算。libc の `wcwidth()` に依存しない | [#3598](https://github.com/mawww/kakoune/issues/3598), [#4257](https://github.com/mawww/kakoune/issues/4257), [#3059](https://github.com/mawww/kakoune/issues/3059), [#1941](https://github.com/mawww/kakoune/issues/1941) |
| R-009 | 特殊文字の可視化 | ゼロ幅文字 (U+200B 等) と制御文字 (^A, ^M) をプレースホルダグリフで可視表示 | [#3570](https://github.com/mawww/kakoune/issues/3570), [#2936](https://github.com/mawww/kakoune/issues/2936) |

### 2.2 標準フローティング UI

Kasane は、標準提供する menu / info などのフローティング UI について、表示、配置、閲覧性、視覚安定性を保証する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-010 | 標準メニュー表示 | `menu_show` メッセージに基づく補完メニューの標準フローティング表示 | — |
| R-011 | 標準メニュースタイル | `inline`, `prompt`, `search` の各スタイルに応じて標準 UI を切り替える | — |
| R-012 | メニュー選択表示 | `menu_select` メッセージに基づき、標準メニュー UI で選択項目をハイライト表示する | — |
| R-013 | メニュー非表示 | `menu_hide` メッセージに基づき、標準メニュー UI を即座に非表示化する | — |
| R-014 | 標準フローティング UI の配置 policy | 標準 menu / info UI はアンカー選択、配置、衝突回避の policy を設定可能である | [#3938](https://github.com/mawww/kakoune/issues/3938), [#2170](https://github.com/mawww/kakoune/issues/2170), [#1531](https://github.com/mawww/kakoune/issues/1531) |
| R-016 | 高頻度更新下での中間状態抑制 | 高頻度 UI 更新下でも、標準フローティング UI は不要な中間状態表示や一時的フラッシュを抑制する | [#1491](https://github.com/mawww/kakoune/issues/1491) |
| R-020 | 標準情報表示 | `info_show` メッセージに基づくドキュメント・ヘルプ情報の標準フローティング表示 | — |
| R-021 | 標準情報スタイル | `prompt`, `inline`, `inlineAbove`, `inlineBelow`, `menuDoc`, `modal` の各スタイルに対応する | — |
| R-022 | 情報非表示 | `info_hide` メッセージに基づき、標準情報 UI を即座に非表示化する | — |
| R-023 | 複数情報要素の同時表示 | 標準フローティング UI は複数の独立した情報要素を同時に保持・表示できる | [#1516](https://github.com/mawww/kakoune/issues/1516) |
| R-024 | 長文内容への閲覧手段 | 標準情報 UI は表示領域を超える内容に対してスクロール等の閲覧手段を提供する | [#4043](https://github.com/mawww/kakoune/issues/4043) |
| R-025 | 重要観測対象の遮蔽抑制 | 標準フローティング UI は cursor、selection、anchor 周辺などの重要観測対象を不必要に遮蔽しないよう配置を調整できる | [#5398](https://github.com/mawww/kakoune/issues/5398) |
| R-030 | アンカー追従 | `inline` スタイルのフローティング UI は `anchor` 座標に追従して表示する | — |
| R-031 | 画面境界制御 | フローティング UI が画面境界を超える場合、表示位置を自動調整する | — |
| R-032 | Z軸レイヤー管理 | メニュー、情報ポップアップ、メインバッファの描画順序 (Z-order) を適切に管理する | — |

### 2.3 標準ステータス / プロンプト UI

Kasane は、標準提供する status / prompt 系 UI について、表示、文脈反映、標準的な可読性を保証する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-060 | ステータスバー描画 | `draw_status` に基づくプロンプト、コンテンツ、モードラインの描画 | — |
| R-061 | ステータスバー位置 | ステータスバーの表示位置を上部/下部で設定可能 | [#235](https://github.com/mawww/kakoune/issues/235) |
| R-063 | マークアップレンダリング | ステータスライン内の `{Face}` マークアップ構文をパースしてレンダリング | [#4507](https://github.com/mawww/kakoune/issues/4507) |
| R-064 | カーソル数バッジ | 複数カーソル/選択時にカーソル数をステータスバーに表示 | [#5425](https://github.com/mawww/kakoune/issues/5425) |

### 2.4 互換性と採用容易性

Kasane は、高度な拡張能力を提供しつつ、既存の Kakoune 利用者が通常の `kak` の代替として採用できることを標準動作として重視する。本節で定義する要件は、Kasane 独自の高度機能ではなく、Kasane を標準フロントエンド候補として成立させるための互換性・保守性の要求である。

| ID | 要件 | 説明 |
|----|------|------|
| R-100 | 既存設定互換 | Kasane は Kakoune の通常の設定読み込み経路と整合し、既存の `kakrc` / autoload 構成を前提とした運用を阻害しない |
| R-101 | 既存プラグイン互換 | Kasane は Kasane 専用 API を要求せず、Kakoune 標準機構のみを用いる既存プラグインが動作可能であることを重視する |
| R-102 | セッション運用互換 | Kasane は Kakoune の既存の起動・接続・セッション利用ワークフローと整合的に動作する |
| R-103 | 保守的デフォルト | Kasane のデフォルト UI は、既存の Kakoune 利用者にとって予期しない大規模な再構成や常設 UI 追加を行わない |
| R-104 | 高度機能の任意性 | Kasane 独自のプラグイン、拡張 UI、追加 capability は通常利用の必須前提とならない |
| R-105 | 段階的強化 | Kasane は互換的な標準動作を基盤とし、その上に opt-in の高度機能を積み上げられる |
| R-106 | 代表的ワークフロー維持 | Kasane は既存の plugin manager、LSP クライアント、tmux / SSH 等の代表的ワークフローを採用阻害要因なく扱えることを重視する |

### 2.5 入力処理

Kasane は、キーボード、マウス、スクロールなどの入力を正確に受け取り、Kakoune と標準 UI へ整合的に配送する。

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

### 2.6 カーソルとテキスト装飾

Kasane は、カーソル、選択、下線、取り消し線など、観測された編集状態と装飾を忠実に可視化する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-050 | 複数カーソル描画 | 全カーソル (プライマリ/セカンダリ) をソフトウェアレンダリングで描画 | [#5377](https://github.com/mawww/kakoune/issues/5377) |
| R-051 | フォーカス連動カーソル | ウィンドウフォーカス喪失時にカーソルをアウトラインスタイルに切り替え | [#3652](https://github.com/mawww/kakoune/issues/3652) |
| R-053 | テキスト装飾の忠実描画 | Kakoune が送る下線種別、下線色、取り消し線等のテキスト装飾を、バックエンドが許す範囲で忠実に描画する | [#4138](https://github.com/mawww/kakoune/issues/4138) |

### 2.7 UI オプションとリフレッシュ

Kasane は、Kakoune からの UI オプション変更や再描画要求を受け取り、標準 UI と描画状態へ反映する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-070 | UIオプション受信 | `set_ui_options` メッセージを受信し、レンダリングに反映 | — |
| R-071 | リフレッシュ | `refresh` メッセージに基づく画面再描画 (通常/強制) | — |

### 2.8 クリップボード統合

Kasane は、システムクリップボードと直接連携し、低遅延かつ正確なコピー / ペーストを提供する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-080 | システムクリップボード連携 | システムクリップボード API に直接アクセスし、外部プロセス (xclip/xsel) 不要のコピー/ペースト | [#3935](https://github.com/mawww/kakoune/issues/3935), [#4620](https://github.com/mawww/kakoune/issues/4620) |
| R-081 | 高速ペースト | 外部プロセス起動なしの即時ペースト。大量テキストでも遅延なし | [#1743](https://github.com/mawww/kakoune/issues/1743) |
| R-082 | 特殊文字の正確な処理 | クリップボード内の改行・特殊文字をシェルエスケープの問題なく処理 | [#4497](https://github.com/mawww/kakoune/issues/4497) |

### 2.9 スクロール

Kasane は、ビューポート移動、ページ移動、scrolloff、標準スクロール挙動の正しさと操作感を保証する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-090 | スムーズスクロール | ピクセル単位のスムーズスクロール / 慣性スクロール (オプション) | [#4028](https://github.com/mawww/kakoune/issues/4028) |
| R-091 | scrolloff の正確な処理 | 高 scrolloff 値での境界条件を正しく処理し、カーソルが先頭/末尾行に到達可能 | [#4027](https://github.com/mawww/kakoune/issues/4027) |
| R-092 | 表示行考慮のページスクロール | ソフトラップされた表示行を正確に考慮した PageUp/PageDown 計算 | [#1517](https://github.com/mawww/kakoune/issues/1517) |
| R-093 | 不要スクロールの抑制 | 対象行がビューポート内にある場合の不要なスクロールを抑制 | [#3951](https://github.com/mawww/kakoune/issues/3951) |

### 2.10 標準 UI のスタイル体系

Kasane は、標準 UI 群を一貫した theme / style token / container style の上で構成する。

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| R-028 | 共通 style system | メニュー、info、キーヒント等の標準 UI は共通の style token / theme system を共有する | [#2676](https://github.com/mawww/kakoune/issues/2676), [#3944](https://github.com/mawww/kakoune/issues/3944) |

---

## 3. 拡張基盤要件

本章は、Kasane が外部プラグインおよび将来の標準 UI 拡張を成立させるために、本体が保証すべき拡張基盤能力を定義する。本章の要件は、特定の具体機能を Kasane 本体が標準提供することを意味しない。本章で定義するのは、UI 合成、補助領域、対話性、表示変形、表示単位、複数 surface、拡張スタイリングなど、具体機能を実装可能にするための能力である。これらの基盤の上で実現される代表的な機能や需要は [4. 実証対象・代表ユースケース](#4-実証対象代表ユースケース) で扱う。

### 3.1 UI 合成とレイヤー

Kasane は、標準 UI と plugin UI を含む複数の視覚要素を、共通の合成モデルとレイヤー規則の上で重畳・共存させる基盤を提供する。

| ID | 要件 | 説明 |
|----|------|------|
| P-001 | オーバーレイ合成 | バッファ上に独立した描画レイヤーを重畳し、標準 UI と plugin UI が同じ合成モデルに参加できる |
| P-002 | ビューポート相対配置 | ビューポート座標に対する overlay / marker / plugin UI の配置を表現できる |
| P-003 | レイヤー順序と可視性 | 各 UI 要素は独立した Z 順序、可視性、クリッピング規則を持てる |

### 3.2 補助領域と拡張スロット

Kasane は、本文以外の補助領域や周辺領域を表現し、plugin がそれらへ意味を持って寄与できる拡張スロットを提供する。

| ID | 要件 | 説明 |
|----|------|------|
| P-010 | 補助領域モデル | 本文以外の補助領域 (ガター、右側領域、周辺領域等) を表現できる |
| P-011 | 領域への寄与 | plugin は補助領域へ独自の表示要素を寄与できる |
| P-012 | 領域と source / viewport の対応 | 補助領域上の要素は source、viewport、文書全体位置と対応付けられうる |

### 3.3 対話性とイベント配送

Kasane は、plugin 定義要素を含む任意の UI 要素が hit test、focus、click、drag、wheel、drop などの対象となりうる対話基盤を提供する。

| ID | 要件 | 説明 |
|----|------|------|
| P-020 | interactive 要素 | 任意の UI 要素を hit test、hover、click、drag、wheel、focus の対象として表現できる |
| P-021 | event routing | ネイティブ入力イベントを適切な target へ配送できる |
| P-022 | semantic recognizer と binding | plugin は独自の semantic region と event binding を定義できる |
| P-023 | native drop event | OS 由来の drop event を UI 要素または plugin へ配送できる |

### 3.4 表示変形と再構成

Kasane は、Observed State を改竄せず表示 policy として扱うことを前提に、省略、代理表示、追加表示、再構成を定義できる基盤を提供する。

| ID | 要件 | 説明 |
|----|------|------|
| P-030 | 表示変形の定義 | Observed State に対する表示上の変形を定義・適用できる |
| P-031 | 変形の種別 | 表示変形は、省略、代理表示、追加表示、再構成を含みうる |
| P-032 | 事実と表示 policy の分離 | 表示変形は Observed State の改竄ではなく、表示 policy として扱われる |
| P-033 | plugin 定義変形 | plugin は独自の表示変形を定義できる |
| P-034 | 限定操作の明示 | source への完全な逆写像を持たない表示は、限定操作または読み取り専用として表現できる |

### 3.5 表示単位モデルとナビゲーション

Kasane は、再構成後 UI を操作可能な表示単位として表現し、移動、選択、hit test、source mapping を支える基盤を提供する。

| ID | 要件 | 説明 |
|----|------|------|
| P-040 | 表示単位モデル | 再構成後 UI の表示単位を表現するモデルを提供する |
| P-041 | 幾何情報と source mapping | 表示単位は geometry、semantic role、source mapping、interaction policy を持ちうる |
| P-042 | 表示単位への操作 | 表示単位に対する移動、選択、hit test、focus 管理を支える |
| P-043 | plugin 定義ナビゲーション | plugin は独自の表示単位と navigation policy を定義できる |

### 3.6 複数 surface / workspace / pane 抽象

Kasane は、複数の surface や pane を保持・配置し、plugin が独自の workspace 管理モデルを構築できる抽象を提供する。

| ID | 要件 | 説明 |
|----|------|------|
| P-050 | 複数 surface の保持 | 複数の surface / pane を同時に保持・配置できる |
| P-051 | focus と input routing | surface 間の focus、可視性、入力配送を扱える |
| P-052 | layout 抽象 | plugin は独自の pane / workspace / tab 管理モデルを構築できる |

### 3.7 拡張スタイリングとテキスト表現

Kasane は、Kakoune プロトコルの直接表現を超えて、標準 UI と plugin UI が共通に利用できる richer styling と領域別テキスト表現の基盤を提供する。

| ID | 要件 | 説明 |
|----|------|------|
| P-060 | richer styling | プロトコル由来を超える装飾や視覚表現を持てる |
| P-061 | semantic style token | style は role、focus、context 等に応じた semantic token として扱える |
| P-062 | 領域別テキスト描画 policy | 領域ごとに異なる text rendering policy を適用できる |

---

## 4. 実証対象・代表ユースケース

本章は、[3. 拡張基盤要件](#3-拡張基盤要件) により実現可能であることを示す代表的なユースケースを示す。これらは Kasane 本体が標準提供すべき機能一覧ではなく、基盤の表現力と適用可能性を検証・説明するための対象である。各ユースケースは、標準提供される場合もあれば、外部プラグインとして実装される場合もある。関連 Issue は、Kasane が対処しうる現実の需要を示す証拠として参照される。

### 4.1 バッファ上の補助表示

| ユースケース | 概要 | 依存する基盤能力 | 提供形態 | 関連 Issue |
|-------------|------|------------------|----------|-----------|
| ガターアイコン | 行番号ガターにコードアクション、エラー、git diff 等のアイコンを描画する | P-010, P-011, P-020 | 外部プラグイン想定 | [#4387](https://github.com/mawww/kakoune/issues/4387) |
| インデントガイド | サブピクセルの薄い縦線でインデントレベルや現在スコープを表示する | P-001, P-060 | 外部プラグイン想定 | [#2323](https://github.com/mawww/kakoune/issues/2323), [#3937](https://github.com/mawww/kakoune/issues/3937) |
| クリッカブルリンク | info ボックスやバッファ内の URL をクリック可能にし、ホバー効果を与える | P-020, P-021, P-022 | 外部プラグイン想定 | [#4316](https://github.com/mawww/kakoune/issues/4316) |
| 選択範囲の拡張表示 | 改行を含む選択範囲をウィンドウ幅いっぱいまでハイライトする | P-030, P-060 | 外部プラグイン想定 | [#1909](https://github.com/mawww/kakoune/issues/1909) |

### 4.2 ナビゲーション補助 UI

| ユースケース | 概要 | 依存する基盤能力 | 提供形態 | 関連 Issue |
|-------------|------|------------------|----------|-----------|
| スクロールバー | プロポーショナルハンドル付きスクロールバーを表示し、クリック / ドラッグ操作を提供する | P-010, P-012, P-020 | 外部プラグイン想定 | [#165](https://github.com/mawww/kakoune/issues/165), [PR #5304](https://github.com/mawww/kakoune/pull/5304) |
| スクロールバーアノテーション | 検索結果、エラー、選択範囲の位置をスクロールバー上へマーカー表示する | P-010, P-012, P-020 | 外部プラグイン想定 | [#2727](https://github.com/mawww/kakoune/issues/2727) |
| コード折りたたみ | 表示レベルでの行折りたたみ、ガターの開閉 UI、クリック展開を提供する | P-030, P-040, P-020, P-010 | 外部プラグイン想定 | [#453](https://github.com/mawww/kakoune/issues/453) |
| 表示行ナビゲーション | ソフトラップや再構成後の表示単位に対する `gj/gk` 相当の移動を提供する | P-040, P-042, P-043 | 外部プラグイン想定 | [#5163](https://github.com/mawww/kakoune/issues/5163), [#1425](https://github.com/mawww/kakoune/issues/1425), [#3649](https://github.com/mawww/kakoune/issues/3649) |

### 4.3 複数 view / workspace UI

| ユースケース | 概要 | 依存する基盤能力 | 提供形態 | 関連 Issue |
|-------------|------|------------------|----------|-----------|
| ビルトインスプリット | tmux / WM に依存しない水平 / 垂直分割と任意レイアウトを構築する | P-050, P-051, P-052 | 外部プラグイン想定 | [#1363](https://github.com/mawww/kakoune/issues/1363) |
| フローティングパネル | ファイルピッカーやターミナル等の独立 surface を浮動表示する | P-001, P-050, P-051 | 外部プラグイン想定 | [#3878](https://github.com/mawww/kakoune/issues/3878) |
| フォーカス視覚フィードバック | フォーカス / 非フォーカス surface の視覚差を提供する | P-051, P-061 | 標準提供または外部プラグイン | [#3942](https://github.com/mawww/kakoune/issues/3942), [#3652](https://github.com/mawww/kakoune/issues/3652) |

### 4.4 外部入力とインタラクション

| ユースケース | 概要 | 依存する基盤能力 | 提供形態 | 関連 Issue |
|-------------|------|------------------|----------|-----------|
| ファイルドラッグ＆ドロップ | GUI ファイルマネージャからのファイルドロップでバッファを開く | P-023, P-021 | 標準提供 + 将来拡張 | [#3928](https://github.com/mawww/kakoune/issues/3928) |
| URL 検出 | バッファ内の URL を独自に検出し、interactive region として扱う | P-022, P-020 | 外部プラグイン想定 | [#4135](https://github.com/mawww/kakoune/issues/4135) |

### 4.5 高度なテキスト表現

| ユースケース | 概要 | 依存する基盤能力 | 提供形態 | 関連 Issue |
|-------------|------|------------------|----------|-----------|
| Kasane 独自 decoration を用いる plugin UI | プロトコルに依存しない richer decoration を plugin UI へ適用する | P-060, P-061 | 外部プラグイン想定 | [#4138](https://github.com/mawww/kakoune/issues/4138) |
| 領域別フォントサイズ | インレイヒントを小さく、見出しを大きく等の領域別 text policy を適用する | P-062, P-060 | 外部プラグイン想定 | [#5295](https://github.com/mawww/kakoune/issues/5295) |

---

## 5. 非機能要件

### 5.1 パフォーマンス

| ID | 要件 | 目標値 | 関連 Issue |
|----|------|--------|-----------|
| NF-001 | 描画レイテンシ | Kakoune からの描画命令受信から画面反映まで 16ms 以下 (60fps 相当) | [#1307](https://github.com/mawww/kakoune/issues/1307) |
| NF-002 | 入力レイテンシ | キー入力から Kakoune への送信まで 1ms 以下 | — |
| NF-003 | メモリ使用量 | 通常使用時のメモリ消費を最小限に抑制 | — |
| NF-004 | 局所的再描画 | 変更のあった領域に応じて再描画範囲を抑制する | — |
| NF-005 | 非同期I/O | Kakoune との通信をノンブロッキングで処理 | — |
| NF-006 | 高頻度更新下での視覚安定性 | 高頻度の連続更新 (マクロ再生等) に対しても、視覚的フラッシュや不要な中間状態表示を抑制する | [#1491](https://github.com/mawww/kakoune/issues/1491) |

### 5.2 UI/UX

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| NF-012 | ちらつき排除 | ダブルバッファリングにより一切のちらつきなし | [#3429](https://github.com/mawww/kakoune/issues/3429) |
| NF-013 | Unicode対応 | Unicode 文字幅 (全角/半角/絵文字) を正確に計算し、位置合わせを行う | [#3598](https://github.com/mawww/kakoune/issues/3598) |
| NF-014 | True Color | 24bit True Color (RGB) 対応。ターミナルパレット非依存 | [#3554](https://github.com/mawww/kakoune/issues/3554) |
| NF-015 | Kakoune互換性 | 標準ターミナル UI と同等の操作感を維持 | — |
| NF-016 | ターミナル非依存 | ターミナルエスケープシーケンス・terminfo に一切依存しない | [#4079](https://github.com/mawww/kakoune/issues/4079), [#3705](https://github.com/mawww/kakoune/issues/3705), [#4260](https://github.com/mawww/kakoune/issues/4260) |

### 5.3 正しさ・拡張性

| ID | 要件 | 説明 | 関連 Issue |
|----|------|------|-----------|
| NF-020 | バックエンド間意味的一貫性 | 同一の状態に対し、TUI と GUI は同じ意味の UI を表示する | — |
| NF-021 | 最適化された描画経路の観測等価性 | 増分描画やキャッシュを用いる高速経路は、文書化された再描画ポリシーの下で参照描画と観測上等価である | — |
| NF-022 | 拡張境界の保全 | プラグインは UI と操作を拡張できるが、プロトコルが与える事実とコアの状態遷移を破壊しない | — |
| NF-023 | 縮退動作の明示 | 上流プロトコルが必要情報を与えない場合、Kasane は推定や制限付き表示を行えてよいが、その結果をプロトコル事実と同格には扱わない | — |

---

## 6. 上流依存・縮退動作

本章は、Kasane が目指す能力のうち、現時点では上流プロトコルの情報不足や挙動制約により完全には保証できず、限定的実装またはヒューリスティックな縮退動作として扱う項目を示す。本章の項目は非目標ではないが、現行プロトコルの下ではコア機能要件として厳密に約束できない。Kasane はこれらの項目について、可能な範囲で有用な fallback を提供してよいが、その結果を上流が与えた事実と同格には扱わない。上流改善により必要条件が満たされた場合、これらの項目はコア要件または拡張基盤要件へ再統合されうる。

| ID | 項目 | 現状の扱い | 関連 Issue |
|----|------|------------|-----------|
| D-001 | 起動時 info の保持 | 起動時に受信した info の保持・再表示は有用だが、実現方式は Kakoune 側の起動時挙動に依存する | [#5294](https://github.com/mawww/kakoune/issues/5294) |
| D-002 | 画面外カーソル / 選択範囲の補助表示 | 視野外情報の完全性は上流プロトコルが提供する情報に依存するため、完全保証ではなく限定的 fallback として扱う | [#2727](https://github.com/mawww/kakoune/issues/2727), [#5425](https://github.com/mawww/kakoune/issues/5425) |
| D-003 | ステータスライン文脈推定 | `draw_status` のみからのコマンド / 検索 / 情報メッセージの区別は heuristic fallback として扱う | [#5428](https://github.com/mawww/kakoune/issues/5428) |
| D-004 | 右側ナビゲーション UI の完全性 | スクロールバーや文書全体位置 UI は現状のプロトコルが持つ情報で完全な精度を保証できない場合がある | [#165](https://github.com/mawww/kakoune/issues/165), [PR #5304](https://github.com/mawww/kakoune/pull/5304), [#2727](https://github.com/mawww/kakoune/issues/2727) |

---

## 7. 既知の制約事項

> 各制約の詳細な分析（実装の歪みとプロトコル上の限界）は [Kakoune プロトコル制約分析](./kakoune-protocol-constraints.md) を参照。

| ID | 制約 | 影響 | 回避策 |
|----|------|------|--------|
| C-001 | クリップボード統合なし | プロトコルにクリップボードイベントが存在しない | フロントエンド側でシステムクリップボード API に直接アクセス (R-080) |
| C-002 | 文字幅情報なし | Atom に表示幅情報が含まれない | フロントエンド側で独自の Unicode 幅計算を実装 (R-008) |
| C-003 | オプション変更通知なし | Kakoune 側のオプション変更がリアルタイムで通知されない | `set_ui_options` の定期的なポーリングまたは `refresh` 契機での再取得 |
| C-004 | マウス修飾キー非対応 | マウスイベントに Ctrl/Alt 修飾キーを付与できない | Ctrl+クリック等はフロントエンド側で独自処理 |
| C-005 | 位置パラメータのみ | JSON-RPC は位置パラメータのみ対応 | パーサーで位置パラメータを正確にハンドリング |
| C-006 | ステータスラインコンテキスト不明 | コマンド/検索/メッセージの区別不可 | ヒューリスティック推定 (D-003)。上流 [#5428](https://github.com/mawww/kakoune/issues/5428) の解決を追跡 |
| C-007 | インクリメンタル draw なし | 毎回全表示行が送信される | フロントエンド側で差分検出 (NF-004)。上流 [#4686](https://github.com/mawww/kakoune/issues/4686) の解決を追跡 |
| C-008 | Atom の種別不明 | 行番号/仮想テキスト/コードを区別できない | Face 名ベースのヒューリスティック。上流 [#4687](https://github.com/mawww/kakoune/issues/4687), [PR #4707](https://github.com/mawww/kakoune/pull/4707) を追跡 |

## 8. 関連文書

- [requirements-traceability.md](./requirements-traceability.md) — 状態、Phase、上流依存の追跡
- [semantics.md](./semantics.md) — 現行意味論
- [roadmap.md](./roadmap.md) — 実装順序と未完了項目
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流ブロッカー
