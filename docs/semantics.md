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

### 2.4 Default Frontend Semantics と Extended Frontend Semantics

Kasane は二層の意味論を持つ。

- Default Frontend Semantics
  - 一般利用者に対して Kasane が `kak` の代替フロントエンドとして振る舞うときの意味論
  - Kakoune の protocol truth を保守的に表示し、既存の設定・プラグイン・ワークフローと整合することを優先する
- Extended Frontend Semantics
  - plugin や明示的な表示 policy により、表示構造、interaction policy、surface 構成を強く再構成するときの意味論
  - Default Frontend Semantics を素材として追加的に成立する

Kasane の product としての第一義は Default Frontend Semantics にある。Extended Frontend Semantics は Kasane の能力であり重要な目標だが、通常利用者に対する標準意味論を上書きする前提条件ではない。

## 3. 状態意味論

### 3.1 AppState の役割

`AppState` は、Kakoune から観測できる事実、そこから導出される値、ヒューリスティックで推定した値、frontend 実行時状態を保持する単一の状態空間である。

`AppState` は「すべてが同じ種類の真実」ではない。各フィールドは認識論的な強さが異なる。

### 3.2 Observed State

Observed State は、Kakoune のプロトコルが明示的に伝えた情報である。これらは Kasane の第一級の事実であり、Kasane 側のポリシーで変更してはならない。

例:

- `draw` で受け取るバッファ行
- `draw.cursor_pos`
- `menu_show` / `menu_hide`
- `info_show` / `info_hide`
- `draw_status` と `draw_status.cursor_pos`

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

### 3.6 Display Policy State

Display Policy State は、Observed State をどのように表示へ射影するかを表す frontend 側の policy である。これには overlay の可視化方針、表示変形、代理表示、表示単位の grouping、plugin が導入する再構成規則が含まれる。

Display Policy State は Observed State そのものではない。Kasane はこれを用いて Observed State を省略、代理表示、追加表示、再構成してよいが、その結果を「Kakoune がそう言った事実」として扱ってはならない。

Default Frontend Semantics における Display Policy State は、原則として Observed-preserving である。すなわち、Kasane の標準動作は protocol truth の可視構造を保持しつつ、配置、装飾、補助表示、重畳の改善を行う。Observed-eliding transformation や大規模な再構成は Extended Frontend Semantics に属し、明示的 policy または plugin により導入される。

### 3.7 状態更新の原則

外部から入る入力は、原則として次の流れで処理される。

1. プロトコルまたは frontend 入力を受け取る
2. `AppState` を更新する
3. `DirtyFlags` を生成する
4. プラグインと描画パイプラインへ通知する

状態は描画より先に更新される。描画は常に状態の関数であり、描画結果が状態の真実を生成してはならない。

### 3.8 ヒューリスティックの扱い

ヒューリスティックは以下の原則に従う。

- プロトコルの事実を上書きしない
- 失敗時に明示的な degraded mode を許容する
- 上流で解決されるべき問題は上流依存として分離する
- ヒューリスティック由来の機能は exactness の対象を弱めうる

Default Frontend Semantics では、ヒューリスティック失敗は UI 崩壊ではなく graceful degradation として扱われるべきである。ヒューリスティックが成立しない場合でも、Kasane は core frontend としての意味を保ち、拡張機能のみが弱くなる形を優先する。

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

ただし Default Frontend Semantics では、policy による stale 許容は「既存利用者が `kak` の代替として期待する意味」を壊さない範囲に留める必要がある。stale 許容は plugin-defined 拡張の自由のために存在してよいが、core frontend の意味論的整合性より優先されてはならない。

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

- Contribution (`contribute_to`)
- Line Annotation (`annotate_line_with_ctx`)
- Overlay (`contribute_overlay_with_ctx`)
- Transform (`transform`)
- PaintHook

これらは同一レベルの抽象ではなく、自由度と責務が異なる。

### 8.2 Contribution

`contribute_to()` は framework が定義した拡張点 (`SlotId`) に `Element` を寄与する最も制約の強い拡張である。寄与には `priority` と `size_hint` が付随し、構造的整合性を最も保ちやすい。可能なら最優先される。

### 8.3 Line Annotation

`annotate_line_with_ctx()` はバッファ各行のガターや背景を拡張する仕組みである。これは buffer content 自体を変更せず、行単位の視覚的寄与 (`LineAnnotation`) を行う。`BackgroundLayer` と `z_order` により複数プラグインの寄与を合成する。

### 8.4 Overlay

`contribute_overlay_with_ctx()` は通常の layout フローとは別に重畳される浮動要素である。Overlay は表示レイヤーを追加するが、基底となる protocol state を変更しない。`z_index` により表示順序を制御する。

### 8.5 Transform

`transform()` は既存 `Element` を受け取り、変換して返す統合メカニズムである。かつての Decorator (ラップ/装飾) と Replacement (差し替え) の両方の役割を担う。`TransformTarget` で対象を、`transform_priority()` で適用順序を指定する。

Transform は plugin 合成パイプラインにおいて `apply_transform_chain` として一本化されている。

### 8.6 合成順序と優先順位

現行の基本原則は次の通りである。

1. seed となるデフォルト要素を構築する
2. transform chain を priority 順に適用する (装飾・差し替えを統合的に処理)
3. contribution と overlay を合成する

Transform chain は、かつて別々だった replacement と decorator を統合したものである。priority により適用順序が決まり、軽い装飾も完全差し替えも同一パイプラインで処理される。

### 8.9 プラグインが変更してよいもの / よくないもの

プラグインが変更してよいのは、policy が分かれうる表示と interaction である。
プラグインが変更してよくないのは、protocol truth、core state machine、backend の意味論そのもの、上流が提供していない事実の捏造である。

plugin-defined UI は core frontend semantics の前提条件ではない。plugin が不在でも Kasane の標準 frontend 意味論は完結していなければならない。plugin が導入する表示変形は additive であることを原則とし、標準利用者に対する唯一の真実を置き換えるかたちで core semantics を capture してはならない。

## 9. 表示変形と表示単位

### 9.1 Display Transformation の意味

Display Transformation は、Observed State を素材として別の表示構造を構成する policy である。これは省略、代理表示、追加表示、再構成を含みうる。Display Transformation は view policy であり、protocol truth の改竄ではない。

### 9.2 Observed-preserving と Observed-eliding

Display Transformation には少なくとも 2 種がある。

- Observed-preserving transformation
  - Observed State の可視要素を保持したまま、装飾、配置、重畳、補助表示を追加する
- Observed-eliding transformation
  - 一部の Observed State を省略し、代理表示や summary を用いて再構成する

Kasane は後者を許してよい。ただし、elide された事実は失われたのではなく、display policy によって直接表示されていないだけである。

Default Frontend Semantics においては、Observed-eliding transformation は標準挙動ではない。Kasane が `kak = kasane` の代替性を維持するため、強い省略、代理表示、再構成は Extended Frontend Semantics の側に位置づけられる。

### 9.3 表示変形の境界

Display Transformation が変更してよいのは表示構造と interaction policy である。変更してよくないのは、Observed State の内容を「上流が与えた事実」として偽装することである。

たとえば fold summary が複数行を 1 行へ要約してもよいが、その summary を Kakoune が送った実バッファ行そのものとして扱ってはならない。

### 9.4 Display Unit の意味

Display Unit は、再構成後 UI における操作可能な表示単位である。Display Unit は単なる layout box ではなく、表示上の対象、source との関係、interaction の可否をまとめて表す。

Display Unit は次の情報を持ちうる。

- geometry
- semantic role
- source mapping
- interaction policy
- 他の Display Unit との navigation 関係

### 9.5 Source Mapping の意味

Display Unit は、対応する buffer position、buffer range、selection、derived object、または plugin 定義オブジェクトへの mapping を持ちうる。

この mapping は必ずしも一対一である必要はない。1 つの Display Unit が複数行を代表してもよいし、逆に 1 行が複数 Display Unit に分割されてもよい。

### 9.6 制限付き interaction

Display Unit が source への完全な逆写像を持たない場合、その unit は読み取り専用または制限付き interaction として扱われてよい。

重要なのは、「操作結果が未定義であること」を暗黙にしないことである。Kasane は interaction が不可能または制限される unit を明示的に表現できるべきである。

### 9.7 Plugin と表示変形の責務

plugin は Display Transformation と Display Unit を導入できるが、次の責務を負う。

- protocol truth を捏造しない
- interaction policy を定義可能な範囲に留める
- source mapping が弱い場合は degraded mode を受け入れる

コアはこれに対して次を保証する。

- transformation が view policy として扱われること
- display unit が hit test、focus、navigation の対象として表現できること
- plugin-defined UI が標準 UI と同じ合成規則へ参加できること
- plugin-defined UI が標準 frontend semantics を前提として成立し、その不在時に core frontend の意味を破壊しないこと

## 10. Surface と Workspace

### 10.1 Surface の意味

Surface は画面上の矩形領域を所有し、自身の view 構築、イベント処理、状態変化通知を受け持つ抽象である。コアの主要画面要素は Surface として表現される。

### 10.2 SurfaceId の意味

`SurfaceId` は surface を識別する安定 ID である。buffer、status、menu、info、plugin surface は異なる `SurfaceId` 空間に属する。

### 10.3 Workspace の意味

Workspace は surface の配置とフォーカス、分割、フロートを管理するレイアウト構造である。Workspace は「どの surface がどこにあるか」を表す。

### 10.4 Surface と既存 view 層の関係

現行実装では、Surface 理論は完全には単一化されていない。surface lifecycle は導入されているが、描画構築の一部は依然として legacy view 層に残る。

したがって、Surface は第一級抽象へ向かう途中段階であり、現在の UI 全体を完全に支配する唯一の理論ではない。

### 10.5 現行の制約

現行実装には少なくとも次の制約がある。

- invalidation は依然として global `DirtyFlags` 中心である
- `Surface` が受け取る `rect` と最終描画の整合が完全ではない箇所がある
- overlay の位置決めや一部の core view は legacy path と共存している

### 10.6 将来の per-surface invalidation との関係

`SurfaceId` ベース invalidation は有望な将来方向だが、本ドキュメントでは現行意味論の一部としては扱わない。ここで扱うのは、あくまで現行 system が global dirty を前提としているという事実である。

## 11. 等価性と証明責務

### 11.1 Trace-Equivalence

Kasane は複数のレンダリング最適化パスを持つ。これらは内部手続きが異なっても、観測可能な結果において等価であることを要求される。

### 11.2 Warm/Cold Cache Equivalence

現行テスト戦略では、完全再描画との一致だけでなく、同じ dirty 条件の下で warm cache と cold cache が一貫した結果を返すことが重要な不変条件である。

### 11.3 テストで保証するもの

テストで主に保証するのは次の性質である。

- 参照パスと最適化パスの観測等価性
- cache invalidation の一貫性
- backend 間で共有される意味論の保存

### 11.4 prose でしか表現できない契約

次のような契約は、テストだけでは完全には表現しにくい。

- `stable()` により exactness を弱めることが仕様であること
- heuristic state は protocol truth と同格ではないこと
- plugin が侵してよい境界と侵してはいけない境界

Kasane の non-goal として、標準 frontend 意味論において既存の Kakoune 利用者へ Kasane 独自 ecosystem への参加を要求することは含まれない。Kasane は plugin platform を持つが、Default Frontend Semantics はそれに従属しない。

これらは prose とテストの両方で維持する。

### 11.5 backend 間で一致すべきもの

TUI と GUI は出力手段が異なるが、少なくとも次の意味は一致すべきである。

- 何が表示されるか
- どこに表示されるか
- どの状態変化がどの view 変化を生むか
- どの overlay/menu/info が可視か

## 12. Known Gaps

### 12.1 `stable()` による非厳密性

`stable()` は exact semantics に対する厳密一致を意図的に弱める。これは policy 上の仕様だが、どこで stale が許容されるかを慎重に管理する必要がある。

### 12.2 dependency tracking の限界

AST ベース検証と手書き deps は有用だが、完全な soundness を保証しない。依存理論はまだ single source of truth ではない。

### 12.3 global DirtyFlags と Surface 理論の不一致

Surface は局所的な矩形抽象として導入されているが、invalidation は依然として global dirty に大きく依存している。

### 12.4 Workspace ratio と実描画の不一致

Workspace 側で計算される分割比率と、最終的な view 合成側の flex 配分が完全に一致しない余地がある。

### 12.5 plugin overlay invalidation の穴

GUI 側の scene invalidation と plugin overlay の依存が完全には統合されておらず、overlay が stale になる理論的余地がある。

### 12.6 (解決済み) transform と replacement の統合

~~新しい transform API は観測上 replacement に近い結果を作れるが、lazy seed selection やコストモデルではなお別物として扱われている。~~

Plugin trait レベルでは `transform()` が decorator と replacement の両方を吸収し、`apply_transform_chain` として統合済み。旧 API (`decorate()`, `replace()`) は Plugin trait から削除されている。

### 12.7 display transformation と core invalidation の未統合

display transformation と display unit model は要件上は導入されたが、現行の global dirty / section cache はそれらを第一級の invalidation 単位としてまだ扱っていない。

### 12.8 display-oriented navigation の未整備

visual unit 単位の navigation は将来基盤として要求されるが、現行実装では buffer-oriented navigation が依然として中心であり、display unit との完全な統合理論は未完成である。

## 13. Non-Goals

### 13.1 この文書で扱わない最適化

ここでは個別の micro-optimization や benchmark tuning は扱わない。扱うのは、その最適化が保つべき意味論だけである。

### 13.2 この文書で扱わない利用者向け設定

theme、layout、keybind 等の設定方法は扱わない。設定がどの意味的境界に属するかだけを扱う。

### 13.3 この文書で扱わない将来提案

Phase 5 以降の提案や上流変更後の理想設計は、現行意味論と明示的に区別する。

## 14. 変更方針

### 14.1 いつこの文書を更新するか

次のいずれかが変わるとき、本ドキュメントも更新する。

- 状態分類の意味
- DirtyFlags や invalidation policy
- plugin 合成順序
- Surface/Workspace の意味
- 観測等価性の定義

### 14.2 ADR との関係

ADR は「なぜその決定をしたか」の履歴を保持する。本ドキュメントは「現在何が仕様か」の正本である。両者が衝突した場合、現行仕様としては本ドキュメントを優先し、必要なら ADR に注記を追加する。

### 14.3 テスト更新との同時性

意味論の変更は、可能な限り同じ変更で次も更新する。

- 関連 prose
- 関連テスト
- 必要な invalidation / cache 実装

意味論だけ、またはテストだけを先行させる変更は原則として避ける。

## 15. 関連文書

- [architecture.md](./architecture.md) — システム境界とランタイム構成
- [plugin-api.md](./plugin-api.md) — プラグイン API の参照
- [requirements.md](./requirements.md) — 要件本文の正本
- [decisions.md](./decisions.md) — 設計判断の履歴
