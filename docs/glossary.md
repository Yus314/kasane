# 用語集

## プロトコル・描画

| 用語 | 説明 |
|------|------|
| JSON UI | Kakoune の JSON-RPC 2.0 ベースの外部 UI プロトコル |
| Face | テキストの装飾情報 (前景色, 背景色, 下線色, 属性) |
| Atom | Face と文字列のペア。描画の最小単位 |
| Line | Atom の配列。表示行の1行に対応 |
| Coord | 行番号と列番号のペア。画面上の位置を表す |
| Anchor | フローティングウィンドウの表示基準座標。Kakoune プロトコル由来。Element ツリーでは OverlayAnchor の基盤 |
| Inline スタイル | バッファ内のアンカー位置に追従するフローティング表示 |
| Prompt スタイル | ステータスバー領域に固定表示 |
| ガター | エディタ左端の行番号・アイコン表示領域。Slot::BufferLeft で拡張可能 |
| ダブルバッファリング | オフスクリーンバッファに描画してから一括転送する手法。ちらつきを防止 |
| CellGrid | セルの二次元配列。ダブルバッファリングで差分描画を実現 |

## 宣言的 UI

| 用語 | 説明 |
|------|------|
| Element | UI の宣言的記述の最小単位。Text, StyledLine, Flex, Grid, Stack, Scrollable, Container, Interactive, Empty, BufferRef のバリアントを持つ enum。view() が返すツリーの構成要素 |
| Element ツリー | Element のネスト構造。view(&State) の戻り値。フレームワークがレイアウト計算と CellGrid 描画に使用 |
| view() | State を受け取り Element ツリーを返す純粋関数。TEA の中核 |
| paint() | Element ツリーとレイアウト結果を受け取り、CellGrid に描画する処理 |
| Overlay | Element ツリーの Stack コンテナ内で他の要素の上に重ねて配置される子要素。メニュー・情報ポップアップ等に使用 |
| OverlayAnchor | Overlay の位置指定。Absolute (絶対座標)、Relative (相対位置)、AnchorPoint (Kakoune 互換の anchor ベース配置) |
| InteractiveId | Element に付与するマウスヒットテスト用の識別子。レイアウト結果と照合してクリック対象を特定 |
| 所有型 Element | Element がライフタイムパラメータを持たず全データを所有するメモリモデル (ADR-009-3)。プラグイン作者の認知負荷を最小化。clone コストは BufferRef パターンで軽減 |
| BufferRef | パフォーマンス最適化パターン。バッファ行を clone せず、paint 時に State から直接描画 |

## TEA (The Elm Architecture)

| 用語 | 説明 |
|------|------|
| TEA | The Elm Architecture。State → view() → Element、Event → Msg → update() → State の単方向データフロー |
| State | アプリケーション全体の状態。CoreState (Kakoune 由来) + プラグイン状態を保持 |
| Msg | 状態変更を引き起こすメッセージ。Kakoune メッセージ、入力イベント、プラグインメッセージ等 |
| update() | State と Msg を受け取り、State を更新して Command を返す関数。副作用は Command として明示化 |
| Command | update() が返す副作用の記述。SendToKakoune, Paste, Quit, RequestRedraw, ScheduleTimer, PluginMessage, SetConfig |
| DirtyFlags | AppState の変更箇所を示すビットフラグ (u16)。BUFFER, STATUS, MENU_STRUCTURE, MENU_SELECTION, INFO, OPTIONS の 6 種。on_state_changed() や PluginSlotCache の無効化判定に使用 |
| CoreState | Kakoune プロトコル由来の状態 (バッファ行、カーソル、メニュー、ステータス等)。プラグインからは読み取り専用 |

## プラグインシステム

| 用語 | 説明 |
|------|------|
| Plugin | kasane の拡張単位。独自の State, Msg, update(), view() を持つ Rust クレート |
| PluginId | プラグインの一意な識別子 |
| PluginRegistry | 登録された全プラグインを管理し、Slot 収集・Decorator 適用・Replacement 解決を行う |
| Slot | フレームワークが定義する拡張ポイント。プラグインは Slot に Element を挿入して UI を拡張 |
| Decorator | 既存の Element を受け取りラップして返す拡張パターン。行番号追加、ボーダー変更等 |
| Replacement | 既存コンポーネントを完全に差し替える拡張パターン。メニューの fzf 風差替等 |
| DecorateTarget | Decorator の適用対象 (Buffer, StatusBar, Menu, Info, BufferLine) |
| ReplaceTarget | Replacement の適用対象 (MenuPrompt, MenuInline, InfoPrompt, StatusBar 等) |
| proc macro | `#[kasane::plugin]`, `#[kasane::component]` 等の手続きマクロ。ボイラープレート自動生成・コンパイル時検証 |
| LineDecoration | プラグインがバッファの各行に提供する装飾。left_gutter (左ガター Element)、right_gutter (右ガター Element)、background (行背景 Face) の 3 つのオプショナル要素で構成 |
| contribute_overlay | Plugin トレイトのメソッド。プラグインが Overlay (位置指定付き浮動 Element) を一つ提供する拡張ポイント。Slot::Overlay とは独立 |
| contribute_line | Plugin トレイトのメソッド。指定行の LineDecoration を返す。ガターアイコンや行背景の実装に使用 |
| on_state_changed | Plugin トレイトのライフサイクルメソッド。AppState 更新時に DirtyFlags 付きで呼ばれる。プラグイン内部状態の同期に使用 |
| observe_key / observe_mouse | Plugin トレイトの入力観測メソッド。全プラグインに通知されるが消費不可。内部状態の追跡に使用 |
| state_hash | Plugin トレイトのメソッド。内部状態の u64 ハッシュを返す。PluginSlotCache の L1 キャッシュ層で差分判定に使用 |
| slot_deps | Plugin トレイトのメソッド。指定 Slot の contribute() が依存する DirtyFlags を返す。PluginSlotCache の L3 キャッシュ層で使用 |
| PluginSlotCache | PluginRegistry のメモリ内キャッシュ。L1 (state_hash) と L3 (slot_deps) の 2 階層で contribute() 結果をキャッシュし、不要な再計算を回避 |
| transform_menu_item | Plugin トレイトのメソッド。メニューアイテム (Atom 配列) の描画前変換。アイコン追加等に使用 |
| CursorLinePlugin | ビルトインプラグイン。カーソル行の背景色をハイライト。contribute_line() の実用例 |
| ColorPreviewPlugin | ビルトインプラグイン。バッファ内の色コード (#RRGGBB, #RGB, rgb:RRGGBB) を検出し、ガタースウォッチとインタラクティブカラーピッカーを提供。contribute_line() + contribute_overlay() + handle_mouse() の実用例 |

## レイヤー責務

| 用語 | 説明 |
|------|------|
| 四層レイヤー責務モデル | 機能の責務境界を上流 (Kakoune) / コア (kasane-core) / 組み込みプラグイン / 外部プラグインの四層で分類するモデル。判断フローチャートで機能の所属レイヤーを決定する。詳細は [layer-responsibilities.md](./layer-responsibilities.md) |
| API パリティ | 組み込みプラグインと外部プラグインが同一の Plugin trait API を使用する原則。組み込みプラグインが `pub(crate)` を使用しないことで保証される |
| API 実証 | 未実証の Plugin trait extension point を実プラグインで検証すること。組み込みプラグインの主要な役割の一つ |
| フロントエンドネイティブ | OS やウィンドウシステムに固有の能力 (フォーカス検知、D&D、クリップボード等)。コアレイヤーに属する機能の一カテゴリ |

## レイアウト

| 用語 | 説明 |
|------|------|
| Flex | Flexbox 簡略版のレイアウトモデル。Direction (Row/Column) + flex-grow + min/max で子要素を配置 |
| Constraints | レイアウト計算時の制約。min/max の幅と高さ |
| measure() | レイアウト計算の第1段階 (下→上)。各要素が制約内でのサイズを報告 |
| place() | レイアウト計算の第2段階 (上→下)。親が子の具体的な位置を決定 |
| LayoutResult | レイアウト計算の結果。各要素の画面上の矩形 (Rect) |
