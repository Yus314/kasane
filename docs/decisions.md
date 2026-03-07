# 技術的意思決定記録 (ADR)

本ドキュメントでは、Kasane プロジェクトの要件策定にあたり決定済みの技術的意思決定を記録する。

## 決定一覧

| 項目 | 決定 | 根拠 |
|------|------|------|
| 実装言語 | **Rust** | パフォーマンス・安全性。kak-ui crate (JSON-RPC ラッパー) 等のエコシステム |
| 対象プラットフォーム | **Linux + macOS** | Kakoune の主要ユーザー層 |
| スコープ | **完全なフロントエンド置換** | ターミナル UI を完全に置き換え、段階的に拡張機能を追加 |
| 描画方式 | **TUI + GUI ハイブリッド** | TUI (MVP) で SSH/tmux ワークフローを維持、GUI で全 Issue 解決 |
| TUI ライブラリ | **crossterm 直接** | 完全な描画制御。GUI バックエンドとの抽象化に最適 |
| GUI ツールキット | **winit + wgpu + cosmic-text** | Alacritty/Zed 同等のアプローチ。Phase 4 で実装 |
| 設定形式 | **TOML + ui_options 併用** | TOML で静的設定 (型安全)、Kakoune ui_options で動的設定 |
| crate 構成 | **Cargo workspace** | kasane-core + kasane-tui + kasane (bin)。Phase 4 で kasane-gui 追加 |
| Kakoune バージョン | **最新安定版のみ** | 新しいプロトコル機能を活用 |
| kak-lsp 連携 | **純粋な JSON UI フロントエンド** | プロトコル準拠。kak-lsp 固有の特別対応なし |
| 開発環境管理 | **Nix flake + direnv** | `flake.nix` + `.envrc` で再現可能な開発環境を提供 |
| 非同期ランタイム | **同期 + スレッド** | crossterm との親和性最高。依存最小。Helix/Alacritty と同じ構成 |
| Kakoune プロセス管理 | **子プロセス起動 + セッション接続** | デフォルトは子プロセス起動、`-c` で既存セッション接続も対応 |
| Unicode 幅計算 | **unicode-width + 互換パッチ** | unicode-width ベースに Kakoune 不一致ケースを個別パッチ |
| エラー処理 | **anyhow + thiserror** | kasane-core は thiserror で構造化、kasane (bin) は anyhow でラップ |
| ロギング | **tracing + ファイル出力** | 構造化ログをファイルに出力。KASANE_LOG 環境変数でフィルタ制御 |
| テスト戦略 | **ユニット + スナップショット (insta)** | コアロジックのユニットテスト + セルグリッドのスナップショット回帰テスト |
| CI/CD | **GitHub Actions + Nix** | Nix 環境で Linux/macOS ビルド・テスト・lint。環境差異なし |
| Rust エディション | **Edition 2024 / MSRV なし** | 最新言語機能をフル活用。Nix でツールチェイン固定のため MSRV 不要 |
| JSON パーサー | **simd-json** | SIMD 活用の高速パース。serde 互換 API で型安全なデシリアライゼーション |
| ライセンス | **MIT OR Apache-2.0** | Rust エコシステム標準のデュアルライセンス |
| 宣言的 UI | **Element ツリー + TEA** | 命令的描画から宣言的 UI 基盤に転換。詳細は [ADR-009](#adr-009-宣言的uiアーキテクチャ--プラグイン基盤への転換) |
| プラグインロード | **コンパイル時 (trait + proc macro)** | 型安全・ゼロコスト。`#[kasane::plugin]` macro でボイラープレート削減 |
| Element メモリ | **所有型 (Owned)** | ライフタイムなし。プラグイン作者にとって最もシンプル |
| 状態管理 | **TEA (The Elm Architecture)** | 単方向データフロー。Rust の所有権モデルと整合 |
| プラグイン拡張 | **Slot + Decorator + Replacement** | 三段階の拡張メカニズムで安全性と自由度を両立 |
| レイアウト | **Flex + Overlay + Grid** | Flexbox 簡略版を基本に、重なりと表形式を追加 |
| イベント伝播 | **中央ディスパッチ + InteractiveId** | キーは TEA update() 集約。マウスは InteractiveId でヒットテスト |
| コンパイラ駆動最適化 | **Svelte 的二層レンダリング** | TEA 維持 + proc macro 強化。詳細は [ADR-010](#adr-010-コンパイラ駆動最適化--svelte-的二層レンダリング) |
| CLI 設計 | **kak ドロップイン置換** | 非UIフラグは exec 委譲、`--` 前後分離、config.toml でデフォルト UI。詳細は [ADR-011](#adr-011-cli-設計--kak-ドロップイン置換) |

## ADR-001: 描画方式 — TUI + GUI ハイブリッド

**状態:** 決定済み

**コンテキスト:**
Kasane の描画方式として TUI (ターミナル内)、GUI (ネイティブウィンドウ)、GPU ターミナル内蔵型、TUI + GUI ハイブリッドの4つの選択肢を検討した。

**選択肢の評価:**

| 方式 | 解決可能 Issue | MVP 期間 | SSH/tmux |
|------|--------------|---------|----------|
| TUI (Kitty 前提) | ~71/80件 | ~2ヶ月 | 対応 |
| GUI | ~80/80件 | ~4-5ヶ月 | 非対応 |
| GPU ターミナル内蔵型 | ~80/80件 | ~5-6ヶ月 | 非対応 |
| TUI + GUI ハイブリッド | TUI: ~71 / GUI: ~80 | TUI: ~2ヶ月 | TUI: 対応 |

**決定:** TUI + GUI ハイブリッドを採用する。

**根拠:**
- SSH/tmux ワークフローの維持が必要 → TUI バックエンドが必須
- GUI のメリット (サブピクセル描画、D&D、フォントサイズ変更等) も欲しい → GUI バックエンドが必要
- コアロジックを `RenderBackend` trait で抽象化し、TUI と GUI を差し替え可能にする
- MVP は TUI で素早くリリースし、Phase 4 で GUI バックエンドを追加

## ADR-002: TUI ライブラリ — crossterm 直接

**状態:** 決定済み

**コンテキスト:**
TUI バックエンドのライブラリとして ratatui + crossterm、crossterm 直接、termwiz の3つを検討した。

**選択肢の評価:**

| ライブラリ | 開発速度 | パフォーマンス | GUI 抽象化との親和性 |
|-----------|---------|--------------|-------------------|
| ratatui + crossterm | 最速 | 中 (フレームワーク制約) | 中 |
| crossterm 直接 | 遅い | 最高 (完全制御) | 高 |
| termwiz | 中間 | 高 | 中 |

**決定:** crossterm 直接を採用する。

**根拠:**
- セルグリッドの差分描画アルゴリズムを独自に最適化できる
- GUI バックエンドとの抽象化が容易 — セルグリッドの差分計算をコアに配置可能
- ratatui のウィジェット再構築オーバーヘッドを回避
- パフォーマンス重視の設計方針に合致

**トレードオフ:**
- ボーダー描画、ポップアップクリッピング、レイアウト計算を全て自前実装する必要あり
- ratatui が提供する 2,000〜3,000 行相当のコードを再実装するコスト

## ADR-003: 設定形式 — TOML + ui_options 併用

**状態:** 決定済み

**コンテキスト:**
設定形式として TOML、KDL、Kakoune コマンド経由 (ui_options のみ) の3つと、TOML + ui_options の併用を検討した。

**決定:** TOML + ui_options 併用を採用する。

**根拠:**
- **TOML (静的設定):** `~/.config/kasane/config.toml` — テーマ、フォント、GUI 設定、デフォルト動作。`serde` による型安全なデシリアライゼーション
- **ui_options (動的設定):** Kakoune `set-option global ui_options kasane_*=*` — ランタイムで変更可能な UI 挙動。Kakoune のフック・条件分岐と組み合わせ可能
- 型安全な静的設定と Kakoune 連携の動的設定を両立

## ADR-004: kak-lsp 連携 — 純粋な JSON UI フロントエンド

**状態:** 決定済み

**コンテキスト:**
kak-lsp は info/menu を多用するため Kasane のフローティングウィンドウの最大の恩恵を受けるプラグインだが、kak-lsp 固有の特別対応を行うかどうかを検討した。

**決定:** 純粋な JSON UI フロントエンドとして、kak-lsp 固有の対応は行わない。

**根拠:**
- プロトコル準拠のみで主要な恩恵 (スクロール可能ポップアップ、配置カスタマイズ、ボーダー) は自然に享受される
- kak-lsp の実装詳細に依存するとバージョンアップで壊れるリスクがある
- kak-lsp 以外のプラグイン (parinfer.kak, kak-tree-sitter 等) との公平性を維持
- 将来的に必要であれば `ui_options` 経由の明示的な連携を検討

## ADR-005: 開発環境管理 — Nix flake + direnv

**状態:** 決定済み

**コンテキスト:**
Rust ツールチェイン (rustc, cargo, clippy, rustfmt) やシステム依存ライブラリ (crossterm が利用する各種 C ライブラリ、Phase 4 の wgpu 依存等) を開発者間で一貫した環境で提供する必要がある。

**決定:** `flake.nix` + `.envrc` (`use flake`) で開発環境を管理する。

**根拠:**
- `nix develop` / `direnv allow` でツールチェインと依存ライブラリが一発で揃う
- `flake.lock` によりビルド再現性が保証される
- macOS (darwin) と Linux の両プラットフォームを単一の `flake.nix` で対応可能
- CI でも同じ Nix 環境を利用することで「ローカルでは通るが CI で落ちる」問題を回避
- Rust ツールチェインは `rust-overlay` または `fenix` で管理し、`rust-toolchain.toml` と整合させる

## ADR-006: 非同期ランタイム — 同期 + スレッド

**状態:** 決定済み

**コンテキスト:**
Kasane の I/O は (1) Kakoune stdout 読み取り、(2) crossterm 入力イベント受信、(3) Kakoune stdin 書き込み、(4) ターミナル出力、(5) タイマーの5本。これらをどう並行処理するかを検討した。

**選択肢の評価:**

| 方式 | 実装コスト | crossterm 親和性 | バイナリサイズ | デバッグ容易性 |
|------|----------|----------------|-------------|-------------|
| 同期 + スレッド | 低 | 最高 | 最小 | 高 |
| tokio | 中 | 中 (EventStream は内部で別スレッド spawn) | +1-2MB | 中 |
| polling / mio 直接 | 高 | 低 (crossterm と二重管理) | 最小 | 中 |

**決定:** 同期 + スレッドを採用する。

**根拠:**
- crossterm の `read()` は同期ブロッキング API であり、非同期版 `EventStream` より信頼性が高い
- Kasane の I/O パターンは3本のストリームを合流させるだけで、tokio の機能の大部分が不要
- Helix, Alacritty, Zellij の入力処理部分も同様のスレッドベース構成
- `std::sync::mpsc` または `crossbeam-channel` でスレッド間メッセージパッシング
- タイマーは `crossbeam-channel::select!` の timeout で実現

## ADR-007: Kakoune プロセス管理 — 子プロセス起動 + セッション接続

**状態:** 決定済み

**コンテキスト:**
Kasane が Kakoune をどう起動し管理するかを検討した。

**決定:** デフォルトは `kak -ui json` を子プロセスとして起動し、`-c` オプションで既存デーモンセッションにも接続可能にする。

**起動パターン:**
- `kasane file.txt` → 内部で `kak -ui json file.txt` を spawn
- `kasane -- -e 'edit file.txt' -s mysession` → Kakoune に引数パススルー
- `kasane -c mysession` → 既存デーモンセッションに `kak -ui json -c mysession` で接続

**根拠:**
- Kakoune のデーモンモード (`kak -d -s` / `kak -c`) はマルチクライアントの重要なワークフロー
- `-c` 非対応は Kakoune ユーザーにとって大きな制限
- JSON UI 接続は新規/接続どちらも `kak -ui json` プロセス経由のため、パイプの仕組みは同一

## ADR-008: JSON パーサー — simd-json

**状態:** 決定済み

**コンテキスト:**
`draw` メッセージは行数×Atom数の JSON が毎フレーム届くため、パーサー性能が描画レイテンシ (NF-001: 16ms 以下) に直結する。

**決定:** simd-json を採用する。

**根拠:**
- SIMD 命令 (SSE4.2/AVX2/NEON) を活用した高速パース
- serde 互換 API (`serde_json` と同じ `Deserialize` derive) で型安全なデシリアライゼーション
- `draw` メッセージは数十〜数百の Atom を含む大きな JSON になり得るため、パーサー性能の差が現れやすい
- 必要に応じて `serde_json` へのフォールバックも容易 (API 互換)

## ADR-009: 宣言的UIアーキテクチャ — プラグイン基盤への転換

**状態:** 決定済み

**コンテキスト:**
kasane を単なる Kakoune フロントエンドから、プラグイン作成者のための UI 基盤に転換する。機能そのものの提供より、拡張性・設定可能性を重視する。命令的な描画パイプラインを宣言的な Element ツリーベースに移行する。

**決定:** 以下の7つの設計判断をパッケージとして採用する。

詳細な設計は [declarative-ui.md](./declarative-ui.md) を参照。

### 9-1: プロトコル結合度 — Kakoune 専用

**状態:** 取り消し済み (当初は「段階的分離」として決定。Kasane は Kakoune 専用 UI 基盤であり、汎用化は目標外と再確認)

**決定:** Kakoune プロトコルと密結合のまま設計する。汎用 UI 基盤への分離は行わない。

**根拠:**
- Kasane は Kakoune のプラグイン作者のための UI 基盤であり、他エディタへの汎用化は目標外
- 不要な抽象化はコードの複雑さを増し、Kakoune プラグイン開発者の体験を損なう
- Kakoune の JSON UI プロトコルに特化することで、最適な設計判断ができる

### 9-2: プラグインロード — コンパイル時 (trait + proc macro)

**決定:** プラグインは Rust クレートとして実装し、`#[kasane::plugin]` / `#[kasane::component]` proc macro でボイラープレートを自動生成する。

**根拠:**
- 型安全性が最高。不正な Msg 送信はコンパイルエラー
- ゼロコストの抽象。モノモーフィゼーションによるランタイムオーバーヘッドなし
- proc macro による恩恵: コンパイル時の構造検証、ボイラープレート削減、レイアウト最適化 (Svelte 的アプローチ)
- Rust エコシステム (crates.io, semver) でプラグインを配布可能

**トレードオフ:**
- プラグイン追加にリビルドが必須。ユーザーに Rust ツールチェインが必要
- プラグイン作者は Rust が書ける必要がある

### 9-3: Element メモリモデル — 所有型 (Owned)

**決定:** `Element` はライフタイムパラメータを持たず、全データを所有する。

**根拠:**
- ライフタイムが API 全体に伝搬しない。プラグイン作者の認知負荷が最も低い
- proc macro が生成するコードにライフタイムの挿入が不要
- Decorator パターンで Element を受け取り加工する際、所有権移動で自由に変形可能
- TUI の Element ツリーは小規模 (20-50 ノード) であり、clone コストはマイクロ秒単位で無視できる

**トレードオフ:**
- State からのデータコピーが発生する (ゼロコピーではない)
- proc macro による Svelte 的最適化 (Element ツリーを経由せず直接描画) で軽減

### 9-4: 状態管理 — TEA (The Elm Architecture)

**決定:** グローバル TEA + プラグインごとのネスト TEA を採用する。

**根拠:**
- 既存の `AppState::apply(KakouneRequest)` が既に TEA 的。移行コストが低い
- Kakoune プロトコル自体が TEA 的 (Kakoune→Frontend: Msg、Frontend→Kakoune: Command)
- Rust の所有権モデルと整合 (`&State` で view、`&mut State` で update)
- プラグインは自分の State/Msg/update/view を持ち、フレームワークが合成。プラグイン間の干渉なし
- テスト容易性が高い。update() は純粋関数的にテスト可能
- Component-local state は Rust の借用規則と根本的に非互換

### 9-5: プラグイン拡張モデル — Slot + Decorator + Replacement

**決定:** 三段階の拡張メカニズムを全て提供する。

- **Slot:** 定義済みの拡張ポイントに Element を挿入
- **Decorator:** 既存 Element を受け取りラップして返す
- **Replacement:** 既存コンポーネントを完全に差し替える

**根拠:**
- Slot のみでは拡張の自由度が不足 (フレームワークが想定しない拡張が不可能)
- Decorator で既存要素の拡張 (行番号追加、ボーダー変更等) を実現
- Replacement で根本的な UI 変更 (メニューの fzf 風差替等) を可能に
- 自由度の段階を設けることで、プラグイン作者が適切なレベルを選択可能

**リスク緩和:**
- Decorator の適用順序は優先度 + ユーザー設定で管理
- Replacement 対象はプロトコル不整合のリスクが低いコンポーネントに限定
- Replacement は明示的な opt-in (`#[unsafe_replace]` 的なマーカー) を検討

**三段階の合成ルール:**
- Replacement が登録されたターゲットでは、デフォルト Element の構築をスキップし、Replacement の Element を使用する
- Decorator は Replacement の出力に対しても適用される。Replacement はコンテンツの差替、Decorator はスタイリング（ボーダー、シャドウ等）を担当し、関心が分離される。これによりテーマプラグイン (Decorator) とカスタムメニュープラグイン (Replacement) が自然に共存する
- Decorator は受け取る Element の内部構造を仮定してはならない（Replacement との合成で構造が変わるため）。Element をそのまま Container でラップするパターンのみ安全
- Decorator で入力 Element を無視して完全に別の Element を返すことは、Replacement と意図が重複するため非推奨。差し替えが目的なら Replacement を使用すべき

**キーイベントルーティング:**
- 明示的なフォーカス概念を持たず、全プラグインの `handle_key()` を優先度順に問い合わせる方式を採用
- 各プラグインは `AppState` を参照して自分が処理すべきか自己判断する（例: Menu Replacement プラグインは `state.menu.is_some()` のとき処理）
- TEA の原則（state が真実の源泉）に合致し、暗黙的なフォーカス状態遷移の複雑さを回避する
- 詳細は [declarative-ui.md](./declarative-ui.md) のイベント伝播セクションを参照

### 9-6: レイアウトモデル — Flex + Overlay + Grid

**決定:** Flexbox 簡略版を基本に、Stack/Overlay と Grid を追加したハイブリッドモデル。

**根拠:**
- Flexbox (Direction + flex-grow + min/max) で TUI のほぼ全てのレイアウトを表現可能
- Overlay は Kakoune のメニュー/情報ポップアップの位置計算 (compute_pos) に必須。Flexbox だけでは重なりを表現できない
- Grid は補完メニューの列揃え等のテーブル形式に必要
- 制約ベース (Cassowary) は TUI には過剰。Ratatui も制約ベースから Flexbox 的アプローチに移行した実績あり
- O(n) で計算可能。段階的に実装可能 (まず Flex、次に Overlay、最後に Grid)

### 9-7: イベント伝播 — ハイブリッド (中央ディスパッチ + InteractiveId)

**決定:** キーイベントは TEA の update() に中央集約。マウスイベントは Element に付与した InteractiveId でヒットテストし、対象を特定した上で update() に渡す。

**根拠:**
- kasane ではほとんどのキー入力が Kakoune に転送される。「大半はデフォルト動作、例外的にプラグインが処理」は中央ディスパッチに最適
- Element はクロージャを含まず純粋なデータ構造のまま維持 (Owned Element との整合)
- マウスのヒットテストはフレームワークがレイアウト結果を使って自動的に行い、プラグインは座標計算不要
- InteractiveId は軽量 (enum or 整数) で Clone/Debug/PartialEq が自然に実装可能

## ADR-010: コンパイラ駆動最適化 — Svelte 的二層レンダリング

**状態:** 決定済み

**コンテキスト:**

Svelte の設計哲学は「フレームワークは出荷しない。コンパイラが出荷する」に集約される。コンポーネントを、DOM を外科的に更新する効率的な命令コードにコンパイルし、仮想 DOM の差分検出コストを排除する。この思想を kasane の宣言的 UI 計画 (ADR-009) にどう取り込むかを検討した。

**分析: TEA vs Svelte 的リアクティビティ**

TEA のモデルは「State 変更 → view() で Element 全体を再構築 → layout → paint → CellGrid → diff → 端末」。Svelte のモデルは「State 変更 → コンパイラ生成コードが変更されたノードのみを直接更新」。

kasane の Element ツリーは 20-50 ノードと極めて小規模で、Web UI の数千ノードとは桁が異なる。パフォーマンス分析では view() + layout() のコスト合計は ~2 μs (フレーム時間の 0.1%) に過ぎず、ボトルネックは端末 I/O (~1,500 μs, 75%) にある。Svelte が解決しようとする問題 (仮想 DOM diffing のコスト) は kasane には存在しない。

さらに Rust の所有権モデルは TEA と自然に整合する (`&State` で view、`&mut State` で update)。コンポーネントローカル状態は Rust の借用規則と根本的に非互換であり、Signals/Runes を持ち込むと `Cell<T>` / `RefCell<T>` / `Rc<T>` の嵐になり、Rust の静的安全性を損なう。

**決定:** TEA をランタイムモデルとして維持し、proc macro (`#[kasane::component]`) を Svelte コンパイラ的に強化する「二層レンダリングモデル」を採用する。

**採用するもの:**

| 概念 | 実現方法 | 時期 |
|------|---------|------|
| コンパイル時依存解析 | proc macro が view() の AST を解析し、各 Element が依存する入力パラメータを特定 | Phase 2 |
| 静的レイアウトキャッシュ | 構造が入力に依存しない部分のレイアウトを一度だけ計算 | Phase 2 |
| 細粒度更新コード生成 | Element 単位の依存追跡により、変更されたセルのみ CellGrid を直接更新 | Phase 2 |
| 二層レンダリングモデル | コンパイル済みパス (proc macro 生成) + インタープリタパス (汎用 Element ツリー) | Phase 2 |

**採用しないもの:**

| 概念 | 理由 |
|------|------|
| コンポーネントローカル状態 | Rust の借用規則と非互換。TEA の中央状態管理が Rust に最適 |
| Signals / Runes | `Cell<T>` / `RefCell<T>` で Rust の静的安全性を損なう。TEA の `&T` / `&mut T` が優れる |
| JSX / テンプレート構文 | IDE 対応が悪く、エラーメッセージが不明瞭。Rust のビルダーパターンの方が型チェック・補完で有利 |
| `$derived` (導出状態) | 手動で十分。形式化は proc macro の複雑度を大幅に増す |

**二層レンダリングモデル:**

```
                  +---------------------+
                  |   宣言的 API 層      |  ← プラグイン作者が触る
                  |  (Element, view())   |
                  +------+--------------+
                         |
             +-----------+----------+
             v                      v
  +------------------+   +----------------------+
  | コンパイル済みパス |   | インタープリタパス     |
  | (proc macro 生成) |   | (汎用 Element ツリー)  |
  |                  |   |                      |
  | 静的構造 → 直接   |   | Element → layout()   |
  |   CellGrid 更新   |   |  → paint() → CellGrid |
  +------------------+   +----------------------+
    ^ #[kasane::component]    ^ Plugin::contribute()
    ^ core_view の静的部分    ^ 動的 Slot/Decorator/Replacement
```

- **コンパイル済みパス**: `#[kasane::component]` が静的解析できる部分。Element ツリーを経由せず、直接 CellGrid を更新。Svelte がコンパイル結果を命令的コードにするのと同じ構造
- **インタープリタパス**: プラグインが動的に Element を提供する部分。従来の Element → layout → paint のフルパス。正しさの保証として常に存在する
- **フォールバックの安全性**: `#[kasane::component]` なしで書いたコードはインタープリタパスで動作する。最適化はオプトインであり、正しさはインタープリタパスが保証する

**根拠:**
- Svelte の真の恩恵は「ランタイムモデルの変更」ではなく「コンパイラに仕事をさせる」思想にある
- ADR-009 の proc macro 計画 (9-2) の自然な延長として位置づけられる
- 宣言的 API を維持しつつ実行時コードを命令的にする、Svelte と同じ二重性を実現
- Phase 2 以降のプラグイン増加時に真価を発揮。Phase 1 では設計上の考慮のみで実装しない

## ADR-011: CLI 設計 — kak ドロップイン置換

**状態:** 決定済み

**コンテキスト:**
kasane は Kakoune の UI フロントエンドであり、「別のエディタ」ではない。kak ユーザーが kasane に移行する際の摩擦を最小化し、`alias kak=kasane` で完全に動作する状態を目指す。

**決定:** kasane を kak のドロップイン置換として設計する。以下の10項目を採用する。

### 11-1: 基本方針 — ドロップイン置換

**決定:** `alias kak=kasane` または PATH 操作で kak を kasane に置き換えた場合に、全ての kak ワークフローが正しく動作することを保証する。

**根拠:**
- kasane は Kakoune の「別の UI」であり、ユーザーは「Kakoune を使っている」と認識すべき
- Neovide (nvim の GUI フロントエンド) と同じパターン: フロントエンド名で起動し、バックエンドに引数を渡す
- `$EDITOR=kasane` 設定時に git commit、ranger 等すべてで kasane UI が使われる

### 11-2: 非UI操作の委譲 — exec

**決定:** 非UI操作 (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`) を検出した場合、kasane プロセスを `exec` で kak に置き換える。`-ui json` は付加しない。

**根拠:**
- exec で kasane プロセスが kak に完全に置き換わるため、オーバーヘッドがゼロ
- Unix 的に最も正しい方式 (不要な親プロセスが残らない)
- 非UI操作に `-ui json` を付加する現状の不正確さを解消

**非UIフラグの検出:** 明示的リスト (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`) をハードコード。kak が新フラグを追加した場合は手動で追加する。

### 11-3: フラグ体系 — `--` 前後分離

**決定:** kasane 固有フラグは GNU 慣例の `--long-option` 形式。kak フラグはそのままパススルー。`--` で明示的に分離可能。

**kasane 固有フラグ:**
- `--ui {tui|gui}` — バックエンド選択 (ワンショット上書き)
- `--version` — kasane + kak 両方のバージョンを表示
- `--help` — kasane のヘルプを表示

**パース規則:**
1. `--` の前: kasane 固有フラグ (`--ui`, `--version`, `--help`) を抽出。それ以外は kak 引数として蓄積
2. `--` の後: すべて kak 引数として蓄積
3. kasane 固有フラグと非UIフラグが混在した場合はエラーで拒否

**根拠:**
- `--` (double dash) は kasane、`-` (single dash) は kak という明確な分離
- kak の `-ui` との衝突を回避 (`kasane -ui gui` は `-ui` と `gui` を kak に渡す)
- 将来のフラグ追加 (`--config`, `--log-level` 等) が安全

### 11-4: セッション名のインターセプト — `-c` と `-s` 両方

**決定:** `-c` (セッション接続) と `-s` (セッション作成) の両方をインターセプトしてセッション名を kasane が保持する。引数は kak にもパススルーする。

**根拠:**
- GUI ウィンドウタイトルにセッション名を表示 (`kasane — project`)
- ログに `[session=project]` として記録
- 将来のセッション固有設定 (`~/.config/kasane/sessions/project.toml`) への拡張
- 追加コストが極めて小さい (数行の変更)

### 11-5: デフォルト UI モード — config.toml で設定可能

**決定:** デフォルトの UI モード (TUI/GUI) を `config.toml` の `[ui] default` で設定可能にする。`--ui` フラグはワンショットの上書き用。

**根拠:**
- GUI をデフォルトにしたいユーザーがエイリアスに `--ui gui` を含める必要がなくなる
- kasane 固有フラグと非UIフラグの混在エラーが実質的に発生しなくなる
- `alias kak=kasane` だけで完全移行が可能

### 11-6: `--version` 出力 — kasane + kak 両方

**決定:** `kasane --version` で kasane と kak 両方のバージョンを表示する。

```
kasane 0.1.0 (kakoune vXXXX.XX.XX)
```

**根拠:**
- デバッグ時に両方のバージョンが分かると有用
- `kasane -version` は kak に exec 委譲され、kak のバージョンのみ表示される (明確な使い分け)

### 11-7: フラグ混在時の挙動 — エラー拒否

**決定:** kasane 固有フラグ (`--ui`, `--version`, `--help`) と非UIフラグ (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`) が同時に指定された場合はエラーで拒否する。

```
kasane --ui gui -l
→ error: --ui cannot be combined with -l (non-UI operation)
```

**根拠:**
- 非UI操作にバックエンド選択は無意味であり、ユーザーのミスを早期に検出できる
- config.toml でデフォルト UI を設定可能にすることで、エイリアスに `--ui` を含める動機がなくなり、このエラーが実質的に発生しない
- 暗黙的な無視よりも明示的なエラーが Rust エコシステムの慣例に沿う

### 11-8: ネイティブ kak UI フォールバック — 不要

**決定:** kasane 経由でネイティブ kak terminal UI にフォールバックする手段は提供しない。

**根拠:**
- ネイティブ UI が欲しいユーザーは kak を直接実行すればよい
- kasane の存在意義は「別の UI を提供する」ことであり、ネイティブ UI に戻す機能は矛盾する

### 処理フロー

```
parse_cli_args(args)
├── 1. kasane 固有フラグを抽出 (--ui, --version, --help)
├── 2. インターセプト対象を抽出 (-c, -s → セッション名保持 + kak にも渡す)
├── 3. 非UIフラグを検出 (-l, -f, -p, -d, -clear, -version, -help)
├── 4. 混在チェック (kasane固有 ∩ 非UI ≠ ∅ → エラー)
└── 結果:
    ├── CliAction::KasaneVersion        ← --version
    ├── CliAction::KasaneHelp           ← --help
    ├── CliAction::DelegateToKak(args)  ← 非UIフラグ検出 → exec kak
    └── CliAction::RunKasane { session, ui_mode, kak_args }  ← UI起動
```

### 具体例

```bash
# 基本的な使い方（ドロップイン）
kasane file.txt                    # → kak -ui json file.txt
kasane -c project                  # → kak -ui json -c project (session名を保持)
kasane -s myses file.txt           # → kak -ui json -s myses file.txt (session名を保持)
kasane -e "buffer-next"            # → kak -ui json -e "buffer-next"
kasane -n -ro file.txt             # → kak -ui json -n -ro file.txt

# kasane 固有フラグ
kasane --ui gui file.txt           # → GUI バックエンドで起動
kasane --version                   # → "kasane 0.1.0 (kakoune vXXXX.XX.XX)"
kasane --help                      # → kasane のヘルプを表示

# 非UI操作（exec で kak に委譲）
kasane -l                          # → exec kak -l
kasane -f "gg"                     # → exec kak -f "gg"
kasane -p session                  # → exec kak -p session
kasane -d -s daemon                # → exec kak -d -s daemon
kasane -version                    # → exec kak -version
kasane -help                       # → exec kak -help

# エラーケース
kasane --ui gui -l                 # → エラー: --ui と -l は併用不可

# -- による明示的分離
kasane --ui gui -- -e "echo hello" # → kak -ui json -e "echo hello"（GUI起動）
```
