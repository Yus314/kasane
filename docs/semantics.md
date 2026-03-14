# Kasane Semantics

本ドキュメントは、Kasane の現行意味論と正しさ条件の正本である。
ここで定義するのは「Kasane が何を意味するか」であり、ベンチマーク値、実装進捗、上流 Issue の追跡、API シグネチャ一覧は対象外とする。

## 1. 文書の責務

### 1.1 この文書が定義するもの

- Kasane のシステム境界
- 状態、更新、描画、invalidation の意味
- プラグイン合成と Surface/Workspace の意味
- 最適化パスに対して要求される観測等価性
- 現在わかっている理論的ギャップ

### 1.2 この文書が定義しないもの

- ベンチマーク値や性能実測の一覧
- いつ何が実装されたかという履歴
- 利用者向け設定の詳細
- プラグイン API の完全なリファレンス
- 将来提案の詳細設計

### 1.3 関連文書

- [requirements.md](./requirements.md): 要件の正本
- [architecture.md](./architecture.md): システム構成と責務境界の要約
- [plugin-development.md](./plugin-development.md): プラグイン作者向けガイド
- [performance.md](./performance.md): 性能原則と測定結果
- [decisions.md](./decisions.md): 設計判断の履歴
- [layer-responsibilities.md](./layer-responsibilities.md): 上流/コア/プラグインの責務境界
- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md): 上流プロトコル制約の分析

## 2. 基本モデル

### 2.1 システム境界

Kasane は Kakoune の JSON UI フロントエンドである。Kakoune は JSON-RPC メッセージとして描画命令と UI 状態を送り、Kasane はそれを `AppState` に反映し、宣言的 UI と backend を通じて描画する。

Kasane は汎用 UI フレームワークではない。Kakoune の JSON UI プロトコルに密結合なまま設計される。

### 2.2 Kakoune と Kasane の責務分担

Kakoune が管理するのは、編集モデル、バッファ内容、選択、メニューや info の発火、プロトコル上の真実である。
Kasane が管理するのは、それをどのような宣言的 UI と backend 実装で表示するか、どのようにプラグイン合成を行うか、そしてプロトコルが表現しない frontend ネイティブ能力をどう扱うかである。

Kasane のコアは「何を、どこに表示するか」を担当し、backend は「どう描画するか」を担当する。

### 2.3 解決層 (HOW) と責務層 (WHERE)

Kasane では 2 つの軸を使って機能を分類する。

- 解決層 (HOW)
  - レンダラ
  - 設定
  - 基盤
  - プロトコル制約
- 責務層 (WHERE)
  - 上流 (Kakoune)
  - コア (`kasane-core`)
  - プラグイン

解決層は「どの仕組みで解決するか」を表し、責務層は「どのレイヤーが責任を持つか」を表す。両者は独立であり、混同しない。

## 3. 状態意味論

### 3.1 AppState の役割

`AppState` は、Kakoune から観測できる事実、そこから導出される値、ヒューリスティックで推定した値、frontend 実行時状態を保持する単一の状態空間である。

`AppState` は「すべてが同じ種類の真実」ではない。各フィールドは認識論的な強さが異なる。

### 3.2 Observed State

Observed State は、Kakoune のプロトコルが明示的に伝えた情報である。これらは Kasane の第一級の事実であり、Kasane 側のポリシーで変更してはならない。

例:

- `draw` で受け取るバッファ行
- `menu_show` / `menu_hide`
- `info_show` / `info_hide`
- `draw_status`
- `set_cursor`

### 3.3 Derived State

Derived State は、Observed State から決定的に再計算できる情報である。Derived State はキャッシュや利便性のために保持されてよいが、意味論上は Observed State から一意に決まる。

例:

- レイアウト結果
- 各種キャッシュの内容
- セクション別の描画データ

### 3.4 Heuristic State

Heuristic State は、Kakoune が明示しない情報を表示データのパターンから推定したものである。これは利便性のために存在するが、正確性は上流プロトコルでは保証されない。

例:

- `FINAL_FG + REVERSE` によるカーソル数推定
- モードライン文字列によるカーソルスタイル推定
- info の同一性推定

Heuristic State は Observed State と同じ強さの真実ではない。ヒューリスティック失敗時の fallback や non-goal を明示する必要がある。

### 3.5 Runtime State

Runtime State は frontend 実行時にのみ存在する状態である。backend のキャッシュ、アニメーション、フォーカス、プラグイン内部状態などが含まれる。

Runtime State は Kakoune の真実を上書きしてはならないが、描画や入力処理の戦略を決めるために保持される。

### 3.6 状態更新の原則

外部から入る入力は、原則として次の流れで処理される。

1. プロトコルまたは frontend 入力を受け取る
2. `AppState` を更新する
3. `DirtyFlags` を生成する
4. プラグインと描画パイプラインへ通知する

状態は描画より先に更新される。描画は常に状態の関数であり、描画結果が状態の真実を生成してはならない。

### 3.7 ヒューリスティックの扱い

ヒューリスティックは以下の原則に従う。

- プロトコルの事実を上書きしない
- 失敗時に明示的な degraded mode を許容する
- 上流で解決されるべき問題は上流依存として分離する
- ヒューリスティック由来の機能は exactness の対象を弱めうる

## 4. 更新意味論

### 4.1 外部入力から状態更新まで

Kasane の更新系は、Kakoune からのプロトコル入力と frontend からのキー/マウス/フォーカス等の入力を受け取り、それらを状態更新とコマンド列に変換する。

基本的な流れは次の通りである。

1. Kakoune からメッセージを受信する
2. `state.apply()` で `AppState` を更新し、dirty を求める
3. 必要なら `update()` で追加の状態遷移と `Command` 生成を行う
4. dirty に基づいてプラグイン通知と描画を行う

### 4.2 TEA update の位置づけ

Kasane は TEA をランタイムモデルとして採用する。`update()` は入力を集約し、状態遷移と副作用指示を一元化する。

TEA の意味論的な利点は次の通りである。

- 状態遷移の入口が明確
- `view` を状態からの純関数として保ちやすい
- Rust の所有権モデルと整合する
- テスト可能な状態遷移単位を作れる

### 4.3 Command の意味

`Command` は副作用そのものではなく、副作用要求の記述である。これには Kakoune への入力送信、設定変更、再描画要求、workspace 操作、プラグイン間通知などが含まれる。

`Command` は view からは生成されず、更新系または plugin hook から生成される。

### 4.4 DirtyFlags の生成

`DirtyFlags` は「どの観測面が変わったか」を表す coarse-grained な変更集合である。`DirtyFlags` はキャッシュ invalidation と選択的再描画の入力であり、状態差分の完全な証明ではない。

重要なのは、`DirtyFlags` が「変更の詳細な内容」ではなく「どの種類の情報が変わったか」を表すことである。

## 5. レンダリング意味論

### 5.1 Exact Semantics

Exact Semantics では、ある状態 `S` に対する描画結果は、参照パスが生成する完全描画結果で定義される。

概念的には次の形で表せる。

```text
render_exact(S) = view(S) -> layout -> paint
```

ここでの正しさは、観測可能な描画結果が `S` の意味と一致することである。

### 5.2 Policy Semantics

Kasane の実際の高速パスは、常に `render_exact(S)` そのものを再計算するわけではない。`DirtyFlags`、各種 cache、`stable()` に基づく policy-relative な増分描画を行う。

そのため実運用上の正しさは次の 2 層に分かれる。

- Exact Semantics: 完全再描画の意味
- Policy Semantics: 現在の invalidation policy を前提にした増分描画の意味

`stable()` がある箇所では、Policy Semantics は Exact Semantics より弱い。これは不具合ではなく、意図的な仕様である。

### 5.3 view, layout, paint の責務分離

- `view`: 状態から宣言的な `Element` ツリーを構築する
- `layout`: `Element` と制約から矩形配置を計算する
- `paint`: `Element` とレイアウト結果を描画バックエンド向け表現に落とす

TUI では `paint` の出力は `CellGrid` であり、GUI では `DrawCommand` 列である。backend ごとの差は存在するが、どちらも同一の UI 意味論を共有する。

### 5.4 TUI と GUI の共通意味論

TUI と GUI は出力表現が異なる。

- TUI: `CellGrid` を diff して terminal I/O へ変換
- GUI: シーン記述を GPU 描画へ変換

しかし両者は、同じ状態に対して同じ UI 構造と同じ意味的内容を表示することを要求される。backend の自由度は「どう描画するか」に限られる。

### 5.5 観測可能な結果とは何か

Kasane の観測等価性は、内部 cache の状態ではなく、最終的に観測される描画結果で定義される。

観測対象の例:

- 表示されるテキスト
- face やスタイル
- 表示位置
- overlay/menu/info の存在と配置
- cursor の表示

## 6. Invalidation とキャッシュ

### 6.1 DirtyFlags の意味

`DirtyFlags` は状態の依存トラッキングと cache invalidation の入力である。これは状態全体の差分を表すものではなく、再計算が必要な観測面の近似表現である。

### 6.2 Section 単位 invalidation

現在の core view は主に `base`、`menu`、`info` のセクションに分割される。キャッシュ invalidation はこのセクション粒度で行われる。

この設計により、メニュー変更が常にバッファ本体の再構築を要求するわけではない。

### 6.3 ViewCache

`ViewCache` は `Element` ツリーまたはその部分木を保持し、対応する dirty が立っていないときに再構築をスキップする。

`ViewCache` は exact な依存解析ではなく、`DirtyFlags` と component deps による policy-driven な再利用を行う。

### 6.4 SceneCache

`SceneCache` は GUI backend 用の `DrawCommand` 列をセクション単位で保持する。`ViewCache` と同様に invalidation mask を持つが、GUI 特有の高速パスに使われる。

### 6.5 PaintPatch

`PaintPatch` は TUI 側で特定の変更パターンに対して直接セル更新を行うコンパイル済み高速パスである。これは full pipeline の代替であり、正しさ条件は参照パスとの観測等価性で定義される。

### 6.6 LayoutCache

`LayoutCache` はセクション別再描画や patched path を支えるためのレイアウト再利用である。レイアウトが状態のどの部分に依存するかは、invalidation policy によって制御される。

### 6.7 `stable()` の意味

`stable()` は「この component が特定の状態変化に対して再構築を要求しない」という policy 宣言である。これは「その状態を一切読まない」という意味ではない。

したがって、`stable(x)` が付いている component は `x` を読むことがありうる。その場合、その component は Exact Semantics に対しては stale になりうるが、Policy Semantics の下では許容される。

### 6.8 `allow()` の意味

`allow()` は `#[kasane::component]` の静的依存解析に対する明示的な escape hatch である。これは soundness を強める機能ではなく、検証器が扱えない依存を意図的に免責するための機能である。

### 6.9 Exactness を意図的に弱める箇所

現在の Kasane は、すべての高速パスに対して完全再描画との逐次一致を要求していない。特に `stable()` が関与する箇所では、warm/cold cache の一貫性が主な正しさ条件になる。

この弱化は設計上の trade-off であり、文書化された仕様として扱う。

## 7. 依存追跡意味論

### 7.1 `#[kasane::component(deps(...))]` の契約

`#[kasane::component(deps(...))]` は、component がどの dirty に依存するかを宣言する契約である。宣言された依存は、再構築を行うべき条件の一部として解釈される。

### 7.2 AST ベース検証の保証

proc macro は AST を解析し、宣言された deps と状態フィールド参照の整合を一部検証する。この検証により、単純な field access の取りこぼしをコンパイル時に検出できる。

### 7.3 手書き依存情報の位置づけ

現行実装では、すべての依存情報が macro から単一生成されるわけではない。手書きの依存表や section deps が並存するため、依存理論はまだ single source of truth ではない。

### 7.4 Soundness の限界

現在の依存追跡は完全には sound ではない。

主な理由:

- helper 関数越しの依存は自動検出できない場合がある
- 手書きの deps 定数と macro 解析が二重管理になっている
- `allow()` は明示的な免責である

したがって、依存追跡は「強い discipline」としては有効だが、「完全証明」ではない。

## 8. Plugin 合成意味論

### 8.1 Extension Point の全体像

Kasane の UI 拡張は主に次のメカニズムで構成される。

- Slot contribution
- LineDecoration
- Overlay
- Decorator
- Replacement
- Transform
- PaintHook

これらは同一レベルの抽象ではなく、自由度と責務が異なる。

### 8.2 Slot contribution

Slot は framework が定義した拡張点に `Element` を挿入する最も制約の強い拡張である。Slot は構造的整合性を最も保ちやすく、可能なら最優先される。

### 8.3 LineDecoration

LineDecoration はバッファ各行のガターや背景を拡張する仕組みである。これは buffer content 自体を変更せず、行単位の視覚的寄与を行う。

### 8.4 Overlay

Overlay は通常の layout フローとは別に重畳される浮動要素である。Overlay は表示レイヤーを追加するが、基底となる protocol state を変更しない。

### 8.5 Decorator

Decorator は既存 `Element` を受け取り、ラップまたは変換して返す。Decorator は原則として styling や軽い構造追加を担当し、入力 `Element` の内部構造を仮定しないことが求められる。

### 8.6 Replacement

Replacement は特定ターゲットのデフォルト view 構築を置き換える。Replacement が差し替えるのは view のみであり、プロトコル処理や core state machine は差し替えない。

### 8.7 Transform

Transform は既存要素またはメニュー項目などに変換列を適用する仕組みである。観測上は replacement と近い結果を作れる場合があるが、seed selection とコストモデルの観点ではまだ完全統合されていない。

### 8.8 合成順序と優先順位

現行の基本原則は次の通りである。

1. seed またはデフォルト要素を決める
2. replacement があればそれを使う
3. decorator を優先順位順に適用する
4. slot contribution や overlay を合成する

重要なのは、replacement が存在しても decorator はその出力に対して適用されうることである。replacement は内容の差し替え、decorator は外側の装飾という関心分離を持つ。

### 8.9 プラグインが変更してよいもの / よくないもの

プラグインが変更してよいのは、policy が分かれうる表示と interaction である。
プラグインが変更してよくないのは、protocol truth、core state machine、backend の意味論そのもの、上流が提供していない事実の捏造である。

## 9. Surface と Workspace

### 9.1 Surface の意味

Surface は画面上の矩形領域を所有し、自身の view 構築、イベント処理、状態変化通知を受け持つ抽象である。コアの主要画面要素は Surface として表現される。

### 9.2 SurfaceId の意味

`SurfaceId` は surface を識別する安定 ID である。buffer、status、menu、info、plugin surface は異なる `SurfaceId` 空間に属する。

### 9.3 Workspace の意味

Workspace は surface の配置とフォーカス、分割、フロートを管理するレイアウト構造である。Workspace は「どの surface がどこにあるか」を表す。

### 9.4 Surface と既存 view 層の関係

現行実装では、Surface 理論は完全には単一化されていない。surface lifecycle は導入されているが、描画構築の一部は依然として legacy view 層に残る。

したがって、Surface は第一級抽象へ向かう途中段階であり、現在の UI 全体を完全に支配する唯一の理論ではない。

### 9.5 現行の制約

現行実装には少なくとも次の制約がある。

- invalidation は依然として global `DirtyFlags` 中心である
- `Surface` が受け取る `rect` と最終描画の整合が完全ではない箇所がある
- overlay の位置決めや一部の core view は legacy path と共存している

### 9.6 将来の per-surface invalidation との関係

`SurfaceId` ベース invalidation は有望な将来方向だが、本ドキュメントでは現行意味論の一部としては扱わない。ここで扱うのは、あくまで現行 system が global dirty を前提としているという事実である。

## 10. 等価性と証明責務

### 10.1 Trace-Equivalence

Kasane は複数のレンダリング最適化パスを持つ。これらは内部手続きが異なっても、観測可能な結果において等価であることを要求される。

### 10.2 Warm/Cold Cache Equivalence

現行テスト戦略では、完全再描画との一致だけでなく、同じ dirty 条件の下で warm cache と cold cache が一貫した結果を返すことが重要な不変条件である。

### 10.3 テストで保証するもの

テストで主に保証するのは次の性質である。

- 参照パスと最適化パスの観測等価性
- cache invalidation の一貫性
- backend 間で共有される意味論の保存

### 10.4 prose でしか表現できない契約

次のような契約は、テストだけでは完全には表現しにくい。

- `stable()` により exactness を弱めることが仕様であること
- heuristic state は protocol truth と同格ではないこと
- plugin が侵してよい境界と侵してはいけない境界

これらは prose とテストの両方で維持する。

### 10.5 backend 間で一致すべきもの

TUI と GUI は出力手段が異なるが、少なくとも次の意味は一致すべきである。

- 何が表示されるか
- どこに表示されるか
- どの状態変化がどの view 変化を生むか
- どの overlay/menu/info が可視か

## 11. Known Gaps

### 11.1 `stable()` による非厳密性

`stable()` は exact semantics に対する厳密一致を意図的に弱める。これは policy 上の仕様だが、どこで stale が許容されるかを慎重に管理する必要がある。

### 11.2 dependency tracking の限界

AST ベース検証と手書き deps は有用だが、完全な soundness を保証しない。依存理論はまだ single source of truth ではない。

### 11.3 global DirtyFlags と Surface 理論の不一致

Surface は局所的な矩形抽象として導入されているが、invalidation は依然として global dirty に大きく依存している。

### 11.4 Workspace ratio と実描画の不一致

Workspace 側で計算される分割比率と、最終的な view 合成側の flex 配分が完全に一致しない余地がある。

### 11.5 plugin overlay invalidation の穴

GUI 側の scene invalidation と plugin overlay の依存が完全には統合されておらず、overlay が stale になる理論的余地がある。

### 11.6 transform と replacement の未統合

新しい transform API は観測上 replacement に近い結果を作れるが、lazy seed selection やコストモデルではなお別物として扱われている。

## 12. Non-Goals

### 12.1 この文書で扱わない最適化

ここでは個別の micro-optimization や benchmark tuning は扱わない。扱うのは、その最適化が保つべき意味論だけである。

### 12.2 この文書で扱わない利用者向け設定

theme、layout、keybind 等の設定方法は扱わない。設定がどの意味的境界に属するかだけを扱う。

### 12.3 この文書で扱わない将来提案

Phase 5 以降の提案や上流変更後の理想設計は、現行意味論と明示的に区別する。

## 13. 変更方針

### 13.1 いつこの文書を更新するか

次のいずれかが変わるとき、本ドキュメントも更新する。

- 状態分類の意味
- DirtyFlags や invalidation policy
- plugin 合成順序
- Surface/Workspace の意味
- 観測等価性の定義

### 13.2 ADR との関係

ADR は「なぜその決定をしたか」の履歴を保持する。本ドキュメントは「現在何が仕様か」の正本である。両者が衝突した場合、現行仕様としては本ドキュメントを優先し、必要なら ADR に注記を追加する。

### 13.3 テスト更新との同時性

意味論の変更は、可能な限り同じ変更で次も更新する。

- 関連 prose
- 関連テスト
- 必要な invalidation / cache 実装

意味論だけ、またはテストだけを先行させる変更は原則として避ける。

## 14. 関連文書

- [architecture.md](./architecture.md) — システム境界とランタイム構成
- [plugin-api.md](./plugin-api.md) — プラグイン API の参照
- [requirements.md](./requirements.md) — 要件本文の正本
- [decisions.md](./decisions.md) — 設計判断の履歴
