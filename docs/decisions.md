# 技術的意思決定記録 (ADR)

本ドキュメントは、Kasane で採用した技術的意思決定と、その後の更新・取り消しを含む履歴文書である。
現行仕様の正本は [semantics.md](./semantics.md) および各 Current 文書を参照。
本章の要約表は現行読者向けのサマリであり、各 ADR 本文は決定当時の文脈を保持する。後続 ADR による上書きがある場合は、各節の状態欄と注記を優先する。

## 決定一覧 (現行読者向けサマリ)

凡例: `現行` = 現在も有効、`提案` = 将来設計。注記欄は後続 ADR による上書きや実装上の補足を示す。

| 項目 | 状態 | 現在の扱い | 注記 |
|------|------|------------|------|
| 実装言語 | 現行 | **Rust** | パフォーマンス・安全性 |
| 対象プラットフォーム | 現行 | **Linux + macOS** | Kakoune の主要ユーザー層 |
| スコープ | 現行 | **完全なフロントエンド置換** | Kakoune のターミナル UI を置き換え、frontend ネイティブ能力を追加 |
| 描画方式 | 現行 | **TUI + GUI ハイブリッド** | TUI で SSH/tmux、GUI で native window |
| TUI ライブラリ | 現行 | **crossterm 直接** | 完全な描画制御 |
| GUI ツールキット | 現行 | **winit + wgpu + glyphon** | 詳細は [ADR-014](#adr-014-gui-技術スタック--winit--wgpu--glyphon) |
| 設定形式 | 現行 | **TOML + ui_options 併用** | 静的設定 + Kakoune 連携 |
| crate 構成 | 現行 | **Cargo workspace** | `kasane-core` / `kasane-tui` / `kasane-gui` / `kasane` / `kasane-macros` / `kasane-wasm` / `kasane-wasm-bench` |
| Kakoune バージョン | 現行 | **最新安定版のみ** | 新しいプロトコル機能を活用 |
| kak-lsp 連携 | 現行 | **純粋な JSON UI フロントエンド** | kak-lsp 固有の特別対応なし |
| 開発環境管理 | 現行 | **Nix flake + direnv** | 再現可能な開発環境 |
| 非同期ランタイム | 現行 | **同期 + スレッド** | backend / event loop と親和 |
| Kakoune プロセス管理 | 現行 | **子プロセス起動 + セッション接続** | `-c` / `-s` に対応 |
| Unicode 幅計算 | 現行 | **unicode-width + 互換パッチ** | Kakoune 不一致ケースを補正 |
| エラー処理 | 現行 | **anyhow + thiserror** | core は構造化、bin は集約 |
| ロギング | 現行 | **tracing + ファイル出力** | `KASANE_LOG` でフィルタ制御 |
| テスト戦略 | 現行 | **ユニット + スナップショット + property-based tests** | `insta` と `proptest` を併用 |
| CI/CD | 現行 | **GitHub Actions + Nix** | Linux/macOS で build / test / lint |
| Rust エディション | 現行 | **Edition 2024 / MSRV なし** | Nix で toolchain 固定 |
| JSON パーサー | 現行 | **simd-json** | serde 互換 API |
| ライセンス | 現行 | **MIT OR Apache-2.0** | Rust 標準的なデュアルライセンス |
| 宣言的 UI | 現行 | **Element ツリー + TEA** | 詳細は [ADR-009](#adr-009-宣言的uiアーキテクチャ--プラグイン基盤への転換) |
| プラグイン実行モデル | 現行 | **WASM Component Model を第一選択、native proc-macro path を併存** | 9-2 の native-only 前提は [ADR-013](#adr-013-wasm-プラグインランタイム--component-model-採用) で更新 |
| Element メモリ | 現行 | **所有型 (Owned)** | ライフタイムなし |
| 状態管理 | 現行 | **TEA (The Elm Architecture)** | 単方向データフロー |
| プラグイン拡張 | 現行 | **Slot + Decorator + Replacement** | 三段階の拡張機構 |
| レイアウト | 現行 | **Flex + Overlay + Grid** | 基本レイアウト + 重なり + 表形式 |
| イベント伝播 | 現行 | **中央ディスパッチ + InteractiveId** | キー集中、マウスは hit test |
| コンパイラ駆動最適化 | 現行 | **deps 検証 + ViewCache / SceneCache / PaintPatch** | 汎用 plugin 向け自動 patch 生成は未実装 |
| CLI 設計 | 現行 | **kak ドロップイン置換** | 非UIフラグは exec 委譲 |
| 三層レイヤー責務 | 現行 | **上流 / コア / プラグイン** | 判断基準は [layer-responsibilities.md](./layer-responsibilities.md) |
| WASM プラグインランタイム | 現行 | **Component Model (wasmtime)** | 詳細な性能値は [ADR-013](#adr-013-wasm-プラグインランタイム--component-model-採用) と [performance-benchmarks.md](./performance-benchmarks.md) を参照 |
| パイプライン等価性テスト | 現行 | **Trace-Equivalence 公理 + proptest** | 現行ハーネスの DirtyFlags 生成は coarse-grained |
| SurfaceId ベース Invalidation | 提案 | **per-surface dirty / cache 設計** | multi-pane 向け、未実装 |
| プラグイン I/O 基盤 | 現行 | **ハイブリッドモデル (WASI 直接 + ホスト媒介)** | Phase P の設計基盤。詳細は [ADR-019](#adr-019-プラグイン-io-基盤--ハイブリッドモデル) |

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

詳細な設計は [plugin-development.md](./plugin-development.md) を参照。

### 9-1: プロトコル結合度 — Kakoune 専用

**状態:** 取り消し済み (当初は「段階的分離」として決定。Kasane は Kakoune 専用 UI 基盤であり、汎用化は目標外と再確認)

**決定:** Kakoune プロトコルと密結合のまま設計する。汎用 UI 基盤への分離は行わない。

**根拠:**
- Kasane は Kakoune のプラグイン作者のための UI 基盤であり、他エディタへの汎用化は目標外
- 不要な抽象化はコードの複雑さを増し、Kakoune プラグイン開発者の体験を損なう
- Kakoune の JSON UI プロトコルに特化することで、最適な設計判断ができる

### 9-2: ネイティブプラグイン開発パス — trait + proc macro

**状態:** 一部更新済み (ランタイムロードの第一選択は [ADR-013](#adr-013-wasm-プラグインランタイム--component-model-採用) の WASM。native path 自体は現行でも有効)

**決定:** ネイティブプラグインは Rust クレートとして実装する。`Plugin` trait の直接実装を基本経路として維持しつつ、`#[kasane::plugin]` / `#[kasane::component]` proc macro はボイラープレート削減と検証補助のために併用する。

**根拠:**
- 型安全性が最高。不正な Msg 送信はコンパイルエラー
- ゼロコストの抽象。モノモーフィゼーションによるランタイムオーバーヘッドなし
- proc macro による恩恵: コンパイル時の構造検証、ボイラープレート削減、レイアウト最適化 (Svelte 的アプローチ)
- Rust エコシステム (crates.io, semver) でプラグインを配布可能

**トレードオフ:**
- プラグイン追加にリビルドが必須。ユーザーに Rust ツールチェインが必要
- プラグイン作者は Rust が書ける必要がある

**後続の更新:**
- [ADR-013](#adr-013-wasm-プラグインランタイム--component-model-採用) で WASM Component Model を追加し、現行の推奨配布経路は WASM に移った
- native path は `kasane::run()` からの登録、`&AppState` への完全アクセス、`Surface` / `PaintHook` 等の escape hatch 用として継続している
- `#[kasane_plugin]` macro の hook parity は段階的に拡張中であり、現時点では一部 hook で `Plugin` trait の直接実装が必要になる

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
- 詳細は [plugin-development.md](./plugin-development.md) のイベント伝播セクションを参照

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

**状態:** 一部実装・一部研究継続

**コンテキスト:**

Svelte の設計哲学は「フレームワークは出荷しない。コンパイラが出荷する」に集約される。コンポーネントを、DOM を外科的に更新する効率的な命令コードにコンパイルし、仮想 DOM の差分検出コストを排除する。この思想を kasane の宣言的 UI 計画 (ADR-009) にどう取り込むかを検討した。

**分析: TEA vs Svelte 的リアクティビティ**

TEA のモデルは「State 変更 → view() で Element 全体を再構築 → layout → paint → CellGrid → diff → 端末」。Svelte のモデルは「State 変更 → コンパイラ生成コードが変更されたノードのみを直接更新」。

kasane の Element ツリーは 20-50 ノードと極めて小規模で、Web UI の数千ノードとは桁が異なる。パフォーマンス分析では view() + layout() のコスト合計は ~2 μs (フレーム時間の 0.1%) に過ぎず、ボトルネックは端末 I/O (~1,500 μs, 75%) にある。Svelte が解決しようとする問題 (仮想 DOM diffing のコスト) は kasane には存在しない。

さらに Rust の所有権モデルは TEA と自然に整合する (`&State` で view、`&mut State` で update)。コンポーネントローカル状態は Rust の借用規則と根本的に非互換であり、Signals/Runes を持ち込むと `Cell<T>` / `RefCell<T>` / `Rc<T>` の嵐になり、Rust の静的安全性を損なう。

**決定:** TEA をランタイムモデルとして維持し、proc macro (`#[kasane::component]`) と policy-driven cache / patch で担える最適化を段階的に導入する「二層レンダリング」方針を採用する。

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

**2026-03 注記:** この節でいう「二層レンダリング」は構想全体の名称である。現行で定着しているのは deps 検証、`ViewCache`、`SceneCache`、`PaintPatch` までであり、汎用 plugin view からの自動 patch 生成は Stage 5 の研究対象に留まる。

### 実装記録

4段階すべて完了済み。

**Stage 1: DirtyFlags ベース view メモ化**

- DirtyFlags を `u8` → `u16` に拡張。`MENU` を `MENU_STRUCTURE` + `MENU_SELECTION` に分割し、選択変更のみの高速パスを実現
- `ViewCache`: セクション別 (base/menu/info) の Element メモ化。`ComponentCache<T>` 汎用ラッパーで `get_or_insert()` + `invalidate()` を提供
- `view()` を `build_base()`, `build_menu_section()`, `build_info_section()` に分解。各セクションの DEPS 定数で必要な DirtyFlags を宣言
- `render_pipeline_cached()` / `scene_render_pipeline_cached()`: DirtyFlags + ViewCache による条件付き再構築

**Stage 2: 検証済み依存追跡**

- `#[kasane::component(deps(FLAG, ...))]` proc macro: AST visitor (`syn::visit`) で関数本体の `state.field` アクセスを走査
- `FIELD_FLAG_MAP`: AppState フィールド → 必要な DirtyFlags のマッピング。宣言された `deps()` に不足があればコンパイルエラー
- `allow(field, ...)` エスケープハッチ: 意図的な依存ギャップ (例: `cursor_pos` は INFO フラグ不要) を明示
- マクロのトークンストリーム走査で `format!` / `println!` 内のフィールドアクセスも検出
- Free reads: `cols`, `rows`, `focused`, `drag`, `smooth_scroll`, `scroll_animation` (フラグ不要)

**Stage 3: SceneCache (DrawCommand レベルキャッシュ)**

- `SceneCache`: セクション別 (base/menu/info) の `Vec<DrawCommand>` キャッシュ。無効化ルールは ViewCache と同一 (`BUFFER_CONTENT|STATUS|OPTIONS`→base, `MENU`→menu, `INFO`→info)
- セルサイズ/画面サイズ変更 → 全セクション無効化
- `view_sections_cached()` + `ViewSections`: セクション分解された view 出力。セクション別処理に対応
- `layout_overlay()`: 単一オーバーレイのレイアウトヘルパー
- `scene_paint_section()`: 個別 Element サブツリーの paint ラッパー
- GUI カーソルアニメーション: `DirtyFlags::BUFFER` を設定せず `cursor_dirty` フラグを使用。カーソルのみのフレームは `scene_cache.composed_ref()` を再利用 (0 μs パイプライン)

**Stage 4: コンパイル済み PaintPatch**

- `PaintPatch` trait: `deps()` / `can_apply()` / `apply_grid()` / `apply_scene()` メソッド
- `StatusBarPatch`: dirty==STATUS → ステータス行のみ直接再描画 (~80 セル vs 1920)
- `MenuSelectionPatch`: dirty==MENU_SELECTION → 旧/新選択項目の face 入れ替え (~10 セル)
- `CursorPatch`: dirty==empty + カーソル移動 → 旧/新位置の face 入れ替え (2 セル)
- `LayoutCache`: `base_layout`, `status_row`, `root_area` のキャッシュ。セクション別再描画に使用
- `render_pipeline_patched()`: パッチ → セクション別 → フルパイプラインのフォールバックチェーン
- デバッグモードの正当性アサーション: パッチ出力 == フルパイプライン出力

**Stage 5: プラグイン向けコンパイル済みレンダリング (設計分析)**

(状態: 分析継続中。プラグイン自体は既に存在し、L1 相当のキャッシュは実装済み。汎用 plugin view 向けの部分レイアウト / 自動 patch 生成は未実装)

*問題の再定義:*

ビルトイン view (StatusBar, Menu, Info, Buffer) は有限個であり構造も既知なので、手書き PaintPatch で十分に最適化できる。コンパイラ駆動の自動生成が必要になるのは**プラグイン作成時**である — プラグイン数が増加すると個別の手動最適化はスケールしない。プラグイン作者に PaintPatch の手書きを要求するのは非現実的である。

*自動生成アプローチの分析結果:*

5つのアプローチを検討し、ビルトイン view への適用には全てに根本的障壁がある:

| アプローチ | 概要 | 障壁 |
|-----------|------|------|
| A: マクロコード生成 | `#[kasane_component]` 拡張で view 関数 AST からパッチコードを自動導出 | proc_macro は単一関数のローカル AST 変換。外部関数展開・レイアウト静的解決が不可能 |
| B: ランタイム追跡 | paint 時にセル出自を記録し、dirty flags で影響セルを特定 | 影響セルは特定できるが**新しい値は計算できない** — view → layout → paint が依然必要 |
| C: 増分差分 (React 方式) | Element ツリー差分で変更箇所のみ再描画 | ViewCache + セクション分割で既にカバー済み。追加の差分レイヤーは複雑さに見合わない |
| D: パッチテンプレート | 再描画可能スロットを定義し、部分的に view + paint を再実行 | **最も現実的**。サブセクション粒度のパイプライン実行 |
| E: 宣言的 DSL | DSL でパッチを記述し、マクロが PaintPatch impl を生成 | paint ロジックは結局手書き。DSL 表現力と Rust 表現力のギャップが問題 |

主因: Rust の view 関数にはアルゴリズム的計算 (word wrap, bin-packing, truncation, 障害物回避配置) が混在しており、コンパイラが静的に解析・変換できない。

Svelte との根本的差異:

```
[Svelte]
Template → Compiler → DOM API 呼出
                         ↓
              ブラウザのレイアウトエンジン (暗黙・自動)
                         ↓
                    画面ピクセル

[Kasane]
view() → Element tree → place() → LayoutResult → paint() → CellGrid → diff() → Terminal
           ↑               ↑                        ↑
        自前構築         自前計算                  自前描画
```

Web では `element.textContent = "new"` で、ブラウザが自動的にレイアウト再計算と再描画を行う。Svelte コンパイラはこの**暗黙のレイアウトエンジン**を前提としている — コンパイラは「何を変えるか」だけを指定すればよく、「どこに配置するか」はブラウザが解決する。Kasane には同等の仕組みがなく、CellGrid への書き込みには自前で計算した座標が必要。

Approach A の詳細障壁 (7つのコンパイルパス):

1. **Element 構築追跡**: `Vec::push()` 系列の記号的実行が必要。条件分岐内の push でパターン空間が指数的に増大
2. **外部関数展開**: proc_macro は単一アイテムのみ操作可能で、他関数の本体を参照できない
3. **レイアウト静的解決**: `measure` は再帰的で常にランタイム計算。Text の Unicode 幅は静的に決定不能
4. **特化 paint コード生成**: Element バリアントが静的に既知なら機械的に可能
5. **DirtyFlags 条件分岐挿入**: 単一 view 関数が異なる DirtyFlags に依存するフィールドを混在使用
6. **GPU パス (DrawCommand) 生成**: CellGrid に加えて DrawCommand 列も生成する必要がありコード量が倍増
7. **正当性検証コード生成**: デバッグ用のフルパイプライン比較コード

DSL (Approach E) の困難な要素:

1. **アルゴリズム的計算の混在**: word wrap, bin-packing, truncation がElement 構築と不可分
2. **レイアウトの内容依存**: Info ポップアップのサイズは word wrap 結果に依存 (循環的)
3. **コンポーネント間位置依存**: Info overlay の位置は Menu Rect + 先行 overlay Rect に依存
4. **構造的バリエーション**: Menu 4分岐、Info 3分岐で組合せ爆発
5. **レイアウト結果の paint 伝播**: LayoutResult ツリーの再帰構造をインラインコードに平坦化が必要
6. **DSL と Rust の二重世界問題**: ヘルパー関数を DSL プリミティブとして再定義する必要
7. **Stack + Overlay 自己参照構造**: 非単調な描画順序で「各 Element を独立にパッチ可能」という前提が崩壊

*プラグインが有利な理由:*

| 障壁 | ビルトイン view | プラグイン Slot 関数 |
|------|----------------|---------------------|
| アルゴリズム的計算 | word_wrap, packing, truncate | **ほぼなし** — 主に生データ表示 |
| レイアウト内容依存 | measure → place 循環 | **Slot Rect は外部提供** — 自己位置計算不要 |
| コンポーネント間位置依存 | Info が Menu を回避 | **Slot 位置は固定** — Slot 間干渉なし |
| 構造的バリエーション | MenuStyle 4分岐 | **通常1パターン** |
| ネスト深度 | 5階層以上 | **1-2階層が典型** |
| 外部関数呼出 | 多数の内部ヘルパー | **自己完結的** |
| Stack + Overlay | Info prompt 自己参照構造 | **Slot に Overlay なし** (Overlay は別 Slot) |

根本的な理由: プラグイン Slot 貢献は**制約付きタスク** — 「既知の位置に小さな Element を挿入」。ビルトイン view は**制約なしタスク** — 「画面全体の構造を構築」。この差が DSL/コンパイルの実現可能性を決定する。

*5段階ロードマップ (L0-L5):*

推奨導入順: L0 → L1 → L3 → L2 → L4 → L5 (最小コストで最大効果)

- **L0: 初期状態 (履歴)** — プラグイン寄与はフルパイプラインで再構築していた
- **L1: プラグイン状態キャッシュ (実装済み)** — `PluginRegistry` 内の `PluginSlotCache` が `contribute_to()` 結果を slot ごとにキャッシュし、`state_hash()` 変化時のみ invalidation する
- **L3: DirtyFlags 依存の明示 (一部実装)** — `contribute_deps()` / `transform_deps()` / `annotate_deps()` により plugin 側が依存を宣言できる。自動導出は未実装
- **L2: Slot 位置キャッシュ (未実装)** — `LayoutCache` を slot 別 Rect キャッシュで拡張し、plugin 状態変更時はその slot のみ部分再描画する
- **L4: パッチコード自動生成 (未実装)** — 単純な plugin view に対して `apply_grid()` 相当を自動生成し、非対応パターンは L2 にフォールバックする
- **L5: Decorator パターン認識 (未実装)** — 典型的な Decorator パターンを認識し、既存 patch の末尾に style 上書きを差し込む

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

## ADR-012: レイヤー責務モデル

**状態:** 決定済み (四層→三層に改定)

**コンテキスト:**
Phase 4a/4b の項目分類で、機能がどの層に属するかの体系的な基準が必要になった。既存の「解決層」(現在は [requirements-traceability.md](./requirements-traceability.md) に移動) は実装メカニズム (HOW) の分類であり、責務境界 (WHERE) の判断基準としては不十分。

当初は四層 (上流/コア/組み込みプラグイン/外部プラグイン) だったが、組み込みプラグイン (`kasane-core/src/plugins/`) を WASM バンドルに移行・削除したことで、組み込みと外部の区別が不要になった。三層モデルに改定。

**決定:** 三層レイヤー責務モデルを採用する。

詳細な設計は [layer-responsibilities.md](./layer-responsibilities.md) を参照。

### 12-1: 三層の定義

| 層 | 定義 | 判断基準 |
|---|---|---|
| 上流 (Kakoune) | プロトコルレベルの関心事 | プロトコル変更が必要か？ |
| コア (kasane-core) | プロトコルの忠実なレンダリング + フロントエンドネイティブ能力 | 唯一の正しい実装が存在するか？ |
| プラグイン | ポリシーが分かれうる機能 | 上記以外 |

プラグイン層は配布形態で分かれる: バンドル WASM (デフォルト UX) / FS 発見 WASM / ネイティブ (`kasane::run()`)。

### 12-2: コアの判断基準 — 「唯一の正しい実装」

「ポリシーの分散があるか」で判定する。

- **ポリシーが一つ:** 複数カーソル描画 (R-050) — face の解析方法は一つしかない → コア
- **ポリシーが複数:** カーソル行の背景色 — 色の選択はユーザーの好み → プラグイン
- **フロントエンドネイティブ:** フォーカス検知 (R-051)、D&D (`P-023` の実証ユースケース) — OS/ウィンドウシステム固有 → コア

### 12-3: API パリティ

WASM プラグインは WIT インターフェース経由で Plugin trait API のサブセットを使用する。`contribute_to`、`transform`、`annotate_line_with_ctx`、`contribute_overlay_with_ctx`、`transform_menu_item`、`cursor_style_override` は WASM (WIT v0.4.0+) で利用可能。`Surface`、`PaintHook`、`Pane` API はネイティブプラグインでのみ使用可能。

### 12-4: 上流の判断基準

プロトコルにない情報のヒューリスティック回避策は原則構築しない。

**例外:** 既存の信頼性の高いヒューリスティックは維持する:
- FINAL_FG+REVERSE によるカーソル検出 (R-064) — 事実上の標準動作
- face 名パターンマッチによる補助領域寄与の推定 (`P-010` / `P-011` 部分実証) — 完全版は上流依存

**根拠:**
- ヒューリスティックは上流の実装変更で破綻するリスクがある
- 上流に正式な解決を促す動機付けを維持する
- 信頼性の低い推測に基づく機能はユーザー体験を損なう

**トレードオフ:**
- 上流の変更を待つ間、一部の機能が利用できない
- 既存ヒューリスティック (FINAL_FG+REVERSE 等) は信頼性が高く実用的であるため例外として維持する
- 新規ヒューリスティックの追加は個別に信頼性を評価し、判断する

## ADR-013: WASM プラグインランタイム — Component Model 採用

**状態:** 決定済み

**コンテキスト:**
Phase 5b で外部プラグインのランタイムロード方式を検討する中で、WASM サンドボックスのパフォーマンス実現可能性を定量評価する必要があった。現行のコンパイル時結合方式 (`kasane::run()` + `#[kasane::plugin]`) は型安全だが、プラグインの追加にリビルドが必要。WASM ならリビルド不要のインストール・有効化が可能になり、プラグインエコシステムの拡大が見込める。

**ベンチマーク環境:** `kasane-wasm-bench` クレート (wasmtime 42, criterion)

**評価方法:** 4 段階のゲート方式で、各ゲートの合格基準を事前に定義し、段階的に評価。

### 13-1: ベンチマーク結果

#### Gate 1: Raw WASM Overhead — ✅ 合格

WASM 呼び出しの基本オーバーヘッドの測定。

| 測定項目 | 結果 | 合格基準 |
|---------|------|----------|
| 空関数呼び出し (noop) | **26.5 ns** | < 200 ns |
| 整数演算 (add) | **23.5 ns** | < 200 ns |
| ホスト関数呼び出し (1 回) | **29.2 ns** | < 300 ns |
| ホスト関数呼び出し (10 回) | **77.5 ns** | < 500 ns |
| ネイティブ noop 比較 | 1.2 ns | — |

WASM 境界越えの固定コストは **~25 ns/call**。追加のホスト関数呼び出しは **~5 ns/call**。

#### Gate 2: Data Crossing Boundary — ✅ 合格

WASM 境界を越えるデータ転送の測定 (raw module, 手動メモリ管理)。

| 測定項目 | 結果 | 合格基準 |
|---------|------|----------|
| 文字列エコー 100B | **59 ns** | < 1 μs |
| 文字列ゲスト生成+読み取り 100B | **165 ns** | < 1 μs |
| 文字列ゲスト生成+読み取り 1KB | **1.17 μs** | < 5 μs |
| Element ガター 24 行 | **1.50 μs** | < 3 μs |
| Element ネスト 3x24 | **4.50 μs** | < 10 μs |
| ホスト関数 state_changed (3 回) | **42 ns** | — |
| ホスト関数 state_changed (6 回) | **56 ns** | — |
| contribute_lines 24 行 | **75 ns** | — |
| フルサイクル (state+lines) | **115 ns** | < 3 μs |

バイナリ Element プロトコルのデコードは 987 ns (24 行ガター)。ホスト関数密度は良好。

#### Gate 3: Component Model Overhead — ✅ 条件付き合格

Component Model (WIT + canonical ABI) と raw module の比較。

| 測定項目 | Raw Module | Component Model | 倍率 |
|---------|-----------|----------------|------|
| noop | 26.5 ns | **552 ns** | 20.8x |
| add | 23.5 ns | **556 ns** | 23.7x |
| echo_string 100B | 59 ns | **758 ns** | 12.9x |
| build_gutter 24 行 | 1.50 μs | **6.12 μs** | 4.1x |
| on_state_changed | 42 ns | **787 ns** | 18.7x |
| contribute_lines 24 行 | 75 ns | **1.04 μs** | 13.9x |
| full_cycle | 115 ns | **1.84 μs** | 16.0x |

| インスタンス化 | 時間 |
|-------------|------|
| Component コンパイル | **9.97 ms** (起動時 1 回) |
| Component インスタンス化 | **24.8 μs** (Store 生成ごと) |

Component Model は canonical ABI の lift/lower で **~500 ns の固定オーバーヘッド** を加える。倍率基準 (< 5x) は軽量関数で不合格だが、実用上の絶対値はすべてフレーム予算内に収まる。ペイロードが大きい関数 (build_gutter) では倍率が 4.1x まで低下し、固定コストが償却される。

#### Gate 4: Realistic Simulation — ✅ 合格

Component Model での実際のプラグイン使用パターンの測定。

| 測定項目 | 結果 | 合格基準 |
|---------|------|----------|
| 1 プラグイン フルフレーム | **1.80 μs** | < 8 μs |
| 3 プラグイン フルフレーム | **5.45 μs** | < 20 μs |
| 5 プラグイン フルフレーム | **8.91 μs** | < 30 μs |
| 10 プラグイン フルフレーム | **18.0 μs** | < 40 μs |
| キャッシュヒット (ホスト側) | **0.26 ns** | — |

| スケーリング | 値 |
|------------|-----|
| プラグインあたりコスト | **~1.8 μs/plugin** (線形) |
| 1 プラグインインスタンス化 | 29.3 μs |
| 5 プラグインインスタンス化 | 131 μs |
| 10 プラグインインスタンス化 | 280 μs |

| WASM vs ネイティブ比較 | ネイティブ | WASM (CM) | 倍率 |
|-----------------------|----------|-----------|------|
| cursor_line フルサイクル | 9.5 ns | 2.01 μs | 212x |
| gutter_24 | 1.63 μs | 6.18 μs | 3.8x |

cursor_line の倍率 (212x) は大きいが、絶対値 (2 μs) はフレーム予算の 5%。実質的な計算を伴う gutter_24 では 3.8x まで低下する。

### 13-2: フレーム予算分析

~49 μs @ 80x24 のフレーム予算に対する WASM プラグインの占有率:

| プラグイン数 | WASM コスト | 予算占有率 | 残りの予算 |
|------------|-----------|----------|----------|
| 1 | 1.80 μs | 3.7% | 47.2 μs |
| 3 | 5.45 μs | 11.1% | 43.6 μs |
| 5 | 8.91 μs | 18.2% | 40.1 μs |
| 10 | 18.0 μs | 36.7% | 31.0 μs |

L1 キャッシュ (DirtyFlags) により、状態変更のないフレームでは WASM 呼び出しを完全にスキップ可能 (キャッシュヒット: 0.26 ns)。実際のエディタ使用では大半のフレームがキャッシュヒットとなるため、実効コストはさらに低い。

### 13-3: 決定

**Component Model (wasmtime) をプラグインランタイムの基盤として採用する。**

**根拠:**

1. **絶対性能が十分**: 5 プラグインで予算の 18%、10 プラグインでも 37%。ホスト側パイプラインに十分な余裕がある。
2. **DX 優位性**: WIT による型安全なインターフェース定義、自動シリアライゼーション (canonical ABI)、手動メモリ管理不要。raw module のバイナリプロトコル保守コストと比較して圧倒的に優位。
3. **言語非依存**: Rust、C/C++、Go、JavaScript (wasm-bindgen) など、wasm32-wasip2 ターゲットをサポートする任意の言語でプラグインを記述可能。
4. **サンドボックス安全性**: WASM の線形メモリモデルにより、プラグインがホストのメモリを破壊できない。
5. **起動コスト許容範囲**: コンパイル 10 ms + 10 インスタンス 280 μs ≈ 10 ms。ユーザーに知覚されない。
6. **キャッシュとの相乗効果**: 既存の DirtyFlags + PluginSlotCache (L1/L3) の仕組みにより、状態変更のないフレームで WASM 呼び出しを完全に回避できる。

**トレードオフ:**

- Component Model は軽量関数で 13-21x のオーバーヘッドを加える。ただし絶対値は ~550 ns であり、フレーム予算 (~49 μs) の 1.1% にすぎない。
- raw module 方式はオーバーヘッドが 10-20 分の 1 だが、手動メモリ管理・バイナリプロトコル・型安全性の欠如により DX が大幅に低下する。
- ネイティブプラグイン (現行方式) は依然として最高性能だが、リビルド必須のためエコシステムのスケーラビリティに限界がある。

**今後の方針:**

- Phase W1: WIT インターフェース設計 (kasane の Plugin trait 相当を WIT で定義)
- ネイティブプラグインは Decorator/Replacement 等 WIT 未公開 API のためのエスケープハッチとして維持
- ホスト関数パターン (ゲスト→ホスト呼び出しで状態取得) を主要なデータフローとして確立
- Component Model のコンパイル結果のキャッシュ (`Engine::precompile_component`) を活用し、2 回目以降の起動を高速化

## ADR-014: GUI 技術スタック — winit + wgpu + glyphon

**状態:** 決定済み

**コンテキスト:**
ADR-001 で TUI + GUI ハイブリッド方式を採用した後、GUI バックエンドの具体的な技術スタックとイベントループ設計を検討した。

### 14-1: 描画スタック — winit + wgpu + glyphon

**決定:** ウィンドウ管理に winit、GPU 描画に wgpu、テキストレンダリングに glyphon を採用する。

| ライブラリ | 役割 |
|-----------|------|
| winit | ウィンドウ管理・入力イベント・IME |
| wgpu | GPU 描画 API (Vulkan/Metal/DX12/GL 抽象) |
| glyphon | テキストレンダリング (cosmic-text + swash + etagere アトラス) |

**選定根拠:** cosmic-term (COSMIC Desktop 公式ターミナル) が同一スタックを本番運用しており、モノスペースグリッド描画の実績がある。glyphon は cosmic-text のフォントシェーピング (rustybuzz) + swash ラスタライズ + etagere アトラスパッキングを wgpu パイプラインに統合する。

**不採用の選択肢:**

| 候補 | 不採用理由 |
|------|-----------|
| OpenGL (glutin + glow) | macOS が OpenGL を非推奨化。wgpu が内部で OpenGL ES バックエンドを持つ |
| Native API (Metal/Vulkan 直接) | プラットフォーム毎に個別レンダラーが必要。保守コストが倍増 |
| CPU のみ (softbuffer + tiny-skia) | 60fps スムーズスクロールの主パスとしては不足。フォールバックとして検討したが未実装 |
| egui | イミディエイトモードが TEA リテインドモードと競合。モノスペースグリッドに非特化 |
| Vello (Linebender) | グリフキャッシュなし (毎フレームベクターパス描画)、API 不安定 (3-5ヶ月毎に破壊的変更)、compute shader 必須 |

### 14-2: イベントループ — run_tui/run_gui 分岐

**決定:** CLI 引数 `--ui gui` でイベントループ全体を切り替える方式 (run_tui/run_gui 分岐) を採用する。

**根拠:**
- winit の `run_app()` はメインスレッドを完全に占有するため、TUI の既存 `recv_timeout` ループとは共存できない
- GUI 側はメインスレッドに winit イベントループ (`ApplicationHandler`)、別スレッドに Kakoune Reader を配置し、`EventLoopProxy` で合流する

**不採用:** `pump_events` 方式 — macOS で動作しない (Cocoa/AppKit の制約。winit ドキュメントに "not supported on iOS, macOS, Web" と明記)。

---

## ADR-015: レンダリングパイプライン性能改善

**決定:** レンダリングパイプラインの4つの構造的非効率を段階的に解消する。

### 背景

CPU パイプラインは ~49 μs (80×24) でフレーム予算内だが、以下の非効率がスケーリングとリソースを浪費していた:

1. **フレーム毎アロケーション**: `grid.diff()` が毎フレーム `Vec<CellDiff>` を割り当て (フル再描画時 ~196 KB、フレーム毎ヒープ割り当ての 71%)
2. **非効率なエスケープシーケンス生成**: `TuiBackend::draw()` が全セルに `MoveTo` を出力し、Face 変更のたびに全 SGR 属性をリセット+再適用
3. **line_dirty 最適化の狭いカバレッジ**: `dirty == DirtyFlags::BUFFER` の完全一致のみ。`BUFFER|STATUS` (最も一般的なバッチ) では無効
4. **コンテナ塗りつぶしオーバーヘッド**: `paint_container` が per-cell `put_char(" ")` でワイド文字クリーンアップチェックを実行

### 実装 (4 ステージ)

| ステージ | 内容 | 主要変更 | 改善効果 |
|----------|------|----------|----------|
| P4 | コンテナ塗りつぶし最適化 | `put_char()` ループ → `clear_region()` | ~0.5-2 μs/container |
| P1 | ゼロアロケーション diff | `diff_into()`, `iter_diffs()`, `is_first_frame()` | 196 KB/frame → 0 |
| P3 | line_dirty カバレッジ拡張 | `selective_clear()` で BUFFER\|STATUS 対応 | ~57% CPU 削減 (1行変更時) |
| P2 | 直接グリッド描画 + SGR 差分 | `draw_grid()` + カーソル自動前進 + `emit_sgr_diff()` | 2.4-3x 高速化 |

### ベンチマーク結果

| 指標 | Before | After | 改善率 |
|------|--------|-------|--------|
| `backend.draw()` 80×24 全セル | 163 μs | 58 μs (`draw_grid`) | 2.8x |
| `backend.draw()` 200×60 全セル | 1,010 μs | 335 μs (`draw_grid`) | 3.0x |
| diff フェーズ アロケーション | 196 KB/frame | 0 | 100% 削減 |
| BUFFER\|STATUS 1行変更 | ~49 μs | ~21 μs | 57% 削減 |

### 変更ファイル

| ファイル | 変更内容 |
|----------|----------|
| `kasane-core/src/render/grid.rs` | `diff_into()`, `iter_diffs()`, `is_first_frame()` 追加 |
| `kasane-core/src/render/paint.rs` | コンテナ塗りつぶしを `clear_region()` に置換 |
| `kasane-core/src/render/pipeline.rs` | `selective_clear()` ヘルパー、line_dirty ゲート拡張 |
| `kasane-core/src/render/mod.rs` | `RenderBackend` に `draw_grid()` 追加 (デフォルト実装付き) |
| `kasane-tui/src/backend.rs` | `draw_grid()` 実装 (カーソル自動前進 + SGR 差分) |
| `kasane-tui/src/lib.rs` | イベントループを `draw_grid()` に切り替え |

## ADR-016: パイプライン等価性テスト — Trace-Equivalence 公理

**状態:** 決定済み

### 背景

Kasane のレンダリングパイプラインは複数の最適化バリアントを持つ:

1. `render_pipeline()` — 完全パイプライン (参照実装)
2. `render_pipeline_cached()` — ViewCache によるサブツリーメモ化
3. `render_pipeline_sectioned()` — セクション単位の選択的再描画
4. `render_pipeline_patched()` — コンパイル済みパッチによる直接セル書き込み
5. Surface 系バリアント (`render_pipeline_surfaces_cached/sectioned/patched`)

現在、バリアント間の等価性は `debug_assert` (debug ビルドのみ) と手動テスト (`cache_soundness.rs`) で検証されているが、以下の課題がある:

- `cache_soundness.rs` は 1 つの固定状態 (`rich_state()`) のみテスト
- `debug_assert` はリリースビルドで無効
- DirtyFlags と状態変異の組み合わせ空間が広く、エッジケースを見逃すリスク

### 決定

任意の有効な `AppState` と `DirtyFlags` の組み合わせに対して、全パイプラインバリアントが**観測等価**であることを形式的不変条件として定義し、proptest による property-based testing で検証する。

**等価性公理:**
```
∀ S ∈ ValidAppState, ∀ D ∈ DirtyFlags:
  render_pipeline(S) ≡ render_pipeline_cached(S, D, warm_cache(S))
                     ≡ render_pipeline_sectioned(S, D, warm_cache(S))
                     ≡ render_pipeline_patched(S, D, warm_cache(S))
```

ここで `warm_cache(S)` は状態 S で ALL フラグによる完全レンダリング後のキャッシュ。

### テスト戦略

1. **Mutation-based fuzzing**: `rich_state()` をベースに、ランダムな状態変異 (カーソル移動、行変更、メニュー toggle 等) を適用
2. **ランダム DirtyFlags**: 現行ハーネスでは 6 つの coarse categories (`BUFFER`, `STATUS`, `MENU_STRUCTURE`, `MENU_SELECTION`, `INFO`, `OPTIONS`) の組み合わせをランダム生成
3. **Warm → Mutate → Render**: キャッシュを warm した後に変異を加え、部分フラグでのレンダリング結果を完全レンダリングと比較

完全 Arbitrary 実装は不要 — mutation-based 戦略で組み合わせ空間を効率的にカバー。

### 実装注記 (2026-03)

- `DirtyFlags` は現在 `BUFFER_CONTENT`, `BUFFER_CURSOR`, `STATUS`, `MENU_STRUCTURE`, `MENU_SELECTION`, `INFO`, `OPTIONS` を持つ
- 現行 `trace_equivalence.rs` の戦略は `BUFFER_CURSOR` を独立に生成せず、coarse-grained な `BUFFER` カテゴリに畳み込んでいる
- したがって本 ADR の公理は現行意味論上の要求であり、検証ハーネスの粒度はまだその完全列挙には到達していない

### 保存メカニズム

```
DirtyFlags → ViewCache invalidation → 各セクション再構築判定
          → SceneCache invalidation → DrawCommand 再生成判定
          → LayoutCache invalidation → レイアウト再計算判定
```

各キャッシュの invalidation mask が正しければ、全バリアントは参照実装と等価。

## ADR-017: SurfaceId ベース Invalidation (設計)

**状態:** 提案 (Phase 5 開始時に実装検討)

### 背景

現在の `DirtyFlags` は global: Kakoune からの Draw メッセージは全 ViewCache/SceneCache/LayoutCache を invalidate する。Phase 5 (multi-pane) では、pane A の Draw が pane B のキャッシュを不要に invalidate する問題が生じる。

### 提案設計

1. **`SurfaceDirtyMap`**: `HashMap<SurfaceId, DirtyFlags>` で global `DirtyFlags` を置換
2. **Per-surface ViewCache**: `HashMap<SurfaceId, ViewCache>` でサーフェスごとにキャッシュ
3. **`apply()` の戻り値変更**: `DirtyFlags` → `Vec<(SurfaceId, DirtyFlags)>`
4. **Global イベント**: Refresh, SetUiOptions は全サーフェスに `ALL` を broadcast
5. **BUFFER_CURSOR split との統合**: per-surface `BUFFER_CONTENT` で pane 間分離

### Surface ↔ DirtyFlags の対応

| Surface | Primary DirtyFlags |
|---|---|
| `SurfaceId::BUFFER` (per-pane) | `BUFFER_CONTENT`, `BUFFER_CURSOR` |
| `SurfaceId::STATUS` | `STATUS` |
| `SurfaceId::MENU` | `MENU_STRUCTURE`, `MENU_SELECTION` |
| `SurfaceId(INFO_BASE + i)` | `INFO` |
| Plugin surfaces | `OPTIONS` (config変更) + カスタム |

### 既存機構との整合

- `PaintHook::surface_filter()` (既存) — per-surface フックフィルタ。設計と整合
- `EffectiveSectionDeps` — per-surface deps に拡張可能
- `PluginSlotCache` — surface ごとに独立したキャッシュエントリ

### Migration Path

1. 内部的に `SurfaceDirtyMap` を導入しつつ、global `DirtyFlags` をフォールバックとして維持
2. `apply()` で Draw の場合は target surface のみフラグ設定、他は全 surface broadcast
3. ViewCache を per-surface に段階的移行
4. テスト: 既存 `cache_soundness.rs` + `trace_equivalence.rs` が single-surface 等価性を保証

### リスク

- Plugin API 互換性: `on_state_changed(dirty: DirtyFlags)` は global のまま維持が安全 (OR 合算)
- 複雑度増加: multi-pane 未実装の段階では premature。Phase 5 開始時に再評価

## ADR-018: Display Policy Layer と Display Transformation / Display Unit Model

**状態:** 決定済み

### 背景

Kasane の要件体系を整理する中で、次の区別を明示する必要が生じた。

- Kasane 本体が直接保証するコア機能
- Kasane が拡張基盤として提供する能力
- その基盤の上で実現される実証ユースケース

とくに、overlay、folding、補助領域 UI、表示行ナビゲーション、workspace UI などを一貫して扱うには、「Observed State をそのまま描く」だけでは足りず、frontend 側に表示 policy の層が必要であることが明確になった。

従来の語彙では `Overlay`、`Decorator`、`Replacement`、`Transform`、`Surface` が個別に存在したが、それらがどの理論の一部なのかが不明瞭だった。結果として、Issue 起点の要求が個別機能の列挙へ流れやすく、「Kasane が直接実装するもの」と「Kasane が可能にするもの」が混線していた。

### 決定

Kasane は `Display Policy Layer` を第一級の設計概念として採用する。

この層は、Observed State を描画へ渡す前に「どのような表示構造へ射影するか」を決める層であり、少なくとも次を含む。

- overlay 合成
- 補助領域への寄与
- 表示変形
- 代理表示
- display unit の grouping
- interaction policy

### 18-1: Display Transformation を許容する

Kasane は、plugin および将来の標準 UI が `Display Transformation` を用いて Observed State を再構成することを許容する。

Display Transformation は次を含みうる。

- 省略
- 代理表示
- 追加表示
- 再構成

ただし、これは **display policy** であり、protocol truth の改竄ではない。

### 18-2: Observed-eliding transformation を許容する

Kasane は `Observed-preserving transformation` だけでなく、`Observed-eliding transformation` も許容する。

例:

- fold summary による複数行の要約表示
- outline view による別構造への再編
- 補助 UI への内容の再配置

ただし、elide された Observed State を「上流がそのように送ってきた事実」として扱ってはならない。elision は表示上の省略であり、真実の削除ではない。

### 18-3: Display Unit Model を導入する

Kasane は、再構成後 UI の操作可能な最小単位として `Display Unit` を導入する。

Display Unit は単なる layout box ではなく、少なくとも次を持ちうる。

- geometry
- semantic role
- source mapping
- interaction policy
- 他の unit との navigation 関係

これにより、表示再構成後の UI に対しても hit test、focus、navigation、selection を意味づけられるようにする。

### 18-4: Source Mapping が弱い場合の扱い

Display Unit が source への完全な逆写像を持たない場合、Kasane はその unit を読み取り専用または制限付き interaction として扱ってよい。

重要なのは、未定義の操作を暗黙にしないことである。Kasane は interaction が不可能または制限される unit を明示的に表現できるべきである。

### 18-5: コアとプラグインの責務分担

プラグインが担うもの:

- transformation policy の定義
- display unit の導入
- plugin 固有 UI の interaction policy

コアが担うもの:

- protocol truth と display policy の分離
- plugin-defined UI を標準 UI と同じ合成規則に乗せること
- display unit を hit test、focus、navigation の対象として表現する基盤
- source mapping が弱い場合の degraded mode の意味付け

### 18-6: 既存 API との関係

現行 API では `Display Transformation` と `Display Unit` の専用抽象は未完成である。

当面の実証手段:

- `Overlay`
- `Decorator`
- `Replacement`
- `Transform`
- `LineDecoration`
- `Surface`

これらは将来の Display Policy Layer の断片的表現であり、完全な同義ではない。とくに source mapping と display-oriented navigation は今後の基盤整備対象とする。

### 18-7: 非目標

本 ADR は、即座に一般目的 UI フレームワーク化することを意味しない。

Kasane は引き続き Kakoune 専用の frontend runtime であり、Display Policy Layer も Kakoune JSON UI から受け取る Observed State を前提に設計される。

### 18-8: 帰結

この決定により、要件文書は次のように整理される。

- コア要件
- 拡張基盤要件
- 実証対象・代表ユースケース
- 上流依存・縮退動作

また、意味論文書は `Display Policy State`、`Display Transformation`、`Display Unit` を第一級概念として扱う。

実装上の次段階は、Phase 5 系で以下を段階的に導入することである。

1. display transformation hook
2. display unit model
3. display-oriented hit test / navigation
4. source mapping と interaction policy の整備

## ADR-019: プラグイン I/O 基盤 — ハイブリッドモデル

**状態:** 決定済み

**コンテキスト:**

Phase P でプラグインに I/O 能力を付与する設計を検討した。現状の WASM プラグインは `WasiCtxBuilder::new().build()` で一切のケイパビリティを付与されておらず、ファイルシステム、プロセス実行、ネットワーク通信が利用できない。これにより、ファジーファインダー、ファイルブラウザ、リンター連携といったプラグインが作れない制限がある。

wasmtime は同期モード (`add_to_linker_sync`) でリンクされており、Plugin trait のすべてのメソッド、adapter.rs の全 WASM 呼び出し、両イベントループ (TUI/GUI) が同期で動作している。

### 19-1: I/O アーキテクチャモデル — ハイブリッドモデル

**検討した選択肢:**

| 選択肢 | 概要 |
|---|---|
| A: ホスト媒介のみ | すべての I/O を `Command` + `update()` で代行 |
| B: WASI 直接のみ | `WasiCtxBuilder` にすべてのケイパビリティを付与し、wasmtime async 化 |
| C: ハイブリッド | 同期 I/O は WASI 直接、非同期 I/O はホスト媒介 |

**決定:** ハイブリッドモデル (C) を採用する。

**切り分け基準:** 「無期限にブロックし得るか？」

| I/O 操作 | ブロック特性 | モデル |
|---|---|---|
| ファイルシステム読み書き | 通常 μs〜ms | WASI 直接 (`preopened_dir`) |
| 環境変数取得 | ns | WASI 直接 (`env`) |
| 単調時計 / 乱数 | ns | WASI 直接 (`inherit_monotonic_clock`) |
| 外部プロセス実行 | 無期限 | ホスト媒介 (`Command::SpawnProcess`) |
| ネットワーク通信 | 無期限 | ホスト媒介 (将来) |

**根拠:**

1. **wasmtime async 化の回避:** B (WASI 直接のみ) では `add_to_linker_sync` → `add_to_linker_async` への変更が必要。これは adapter.rs の全 19 メソッド、Plugin trait の全メソッドシグネチャ、registry.rs、両イベントループ、レンダリングパイプラインに波及する大規模リファクタであり、工数的にも設計的にも不釣り合い。ハイブリッドモデルでは `add_to_linker_sync` を維持したまま、WASI の同期サブセット (`wasi:filesystem`, `wasi:clocks`, `wasi:random`) のみを有効化する。

2. **WASI 仕様の制約:** `wasi:cli/command` は「WASM コンポーネントをコマンドとして実行する」仕様であり、「ゲストからホスト上の任意プログラムを起動する」仕様ではない。B でもプロセス起動にはホスト側の独自 WIT interface が必要となり、結局ホスト媒介と同じ構造に着地する。

3. **ホットパス保護:** B ではプラグインが `contribute_to()` 内で `std::process::Command` を呼べてしまい、レンダリングスレッドが無期限にブロックされる。ハイブリッドモデルではプロセス実行とネットワーク通信を構造的にホットパスから排除できる。

4. **ストリーミングとバックプレッシャー:** ホスト媒介のプロセス実行では、ホストが stdout を 16ms バッチで配送し、バッファサイズ制限とキャンセルを管理できる。WASM 内での同期的なパイプ処理ではこれらの制御が困難。

5. **漸進的移行パス:** ハイブリッドモデルは B への漸進的な移行パス上にある。将来 wasmtime async 化が必要になった場合、既存の `Command::SpawnProcess` + `IoEvent` のパターンはそのまま維持でき、追加的に `wasi:sockets` 等を有効化できる。

**セキュリティモデル:**

| 層 | メカニズム | 制御 |
|---|---|---|
| WASI 層 (同期 I/O) | `preopened_dir` | プラグイン専用ディレクトリのみ。マニフェスト宣言 + ユーザー承認 |
| ホスト層 (非同期 I/O) | `Command::SpawnProcess` のホスト側バリデーション | プログラム許可リスト、引数検証 |

**トレードオフ:**

- プラグイン作者は 2 つの I/O パターン (ファイルは `std::fs`、プロセスは `Command`) を使い分ける必要がある
- ファイル I/O はホットパス内でも呼べてしまうため、ドキュメントでの注意喚起とランタイム計測が必要
- NFS / FUSE マウントなどの例外的に遅いファイルシステムでは、同期 I/O がブロッキングするリスクがある

### 19-2: I/O イベント配送方式 — IoEvent 統一型

**検討した選択肢:**

| 選択肢 | 概要 |
|---|---|
| A: 既存 `update(Box<dyn Any>)` 再利用 | ProcessEvent を `Box<dyn Any>` に包んで `deliver_message()` で配送 |
| B: 専用メソッド `on_process_event()` | Plugin trait に ProcessEvent 専用メソッドを追加 |
| C: 統一型 `on_io_event(IoEvent)` | Plugin trait に IoEvent enum を受け取る 1 メソッドを追加 |

**決定:** IoEvent 統一型 (C) を採用する。

```rust
enum IoEvent {
    Process(ProcessEvent),
    // 将来: Http(HttpResponse), FileWatch(FileWatchEvent), ...
}

enum ProcessEvent {
    Stdout { job_id: u64, data: Vec<u8> },
    Stderr { job_id: u64, data: Vec<u8> },
    Exited { job_id: u64, exit_code: i32 },
}

// Plugin trait に 1 メソッド追加
fn on_io_event(&mut self, _event: IoEvent, _state: &AppState) -> Vec<Command> {
    vec![]
}
```

**WIT:**

```wit
variant io-event {
    process(process-event),
}

record process-event {
    job-id: u64,
    kind: process-event-kind,
}

variant process-event-kind {
    stdout(list<u8>),
    stderr(list<u8>),
    exited(s32),
}

on-io-event: func(event: io-event) -> list<command>;
```

**根拠:**

1. **型安全性:** A の `Box<dyn Any>` + downcast はサイレント無視のリスクがある。C は構造化型により IDE 補完・コンパイル時検証が効く。

2. **スケーラビリティ:** B (専用メソッド) では将来の I/O 種別ごとに Plugin trait にメソッドが増える (`on_http_response()`, `on_file_changed()`, ...)。C は `IoEvent` variant の追加のみで Plugin trait は変わらない。

3. **役割の明確化:** `update()` はプラグイン間メッセージ専用、`on_io_event()` はホストからの I/O 完了通知、`on_state_changed()` は Kakoune プロトコルの状態変更通知。3 つの非同期入力パスが明確に分離される。

4. **WASM 互換性:** WIT で `io-event` variant 型として定義すれば、WASM ゲスト側でタグバイトやシリアライゼーション規約なしに構造化データとして受信できる。

### 19-3: サブフェーズ構成

19-1, 19-2 の決定を反映し、Phase P のサブフェーズを再構成する。

**旧構成 (P-a / P-b / P-c) の問題:**

- P-a (非同期タスク基盤) と P-b (SpawnProcess) は配送先 (`IoEvent` 型) と配送元 (`ProcessManager`) が不可分であり、分離実装できない
- P-c (WASI ケイパビリティ) はハイブリッドモデルでプロセス実行と独立になり、先行実装が可能

**新構成:**

| サブフェーズ | 内容 | 依存 |
|---|---|---|
| P-1 | WASI ケイパビリティ基盤: マニフェストのケイパビリティ宣言、`WasiCtxBuilder` のプラグイン別設定注入 (`preopened_dir`, `env`, `inherit_monotonic_clock`) | なし |
| P-2 | プロセス実行基盤: `IoEvent` / `ProcessEvent` 型定義、`Plugin::on_io_event()` + WIT 追加、`Command::SpawnProcess` + `ProcessManager`、イベントループ統合、16ms バッチ配送、ジョブ ID / キャンセル | P-1 (マニフェストのプログラム許可リスト) |
| P-3 | 実証・安定化: ファジーファインダー参照実装 (WASM ゲスト)、ランタイムフレームタイム計測、バックプレッシャー調整 | P-2 |

**変更の要点:**

- P-a/P-b を統合して P-2 に (分離不可能だったため)
- P-c を P-1 に前倒し (プロセス実行とは独立に先行実装可能)
- P-3 を新設 (実証フェーズ)

## 関連文書

- [semantics.md](./semantics.md) — 現行仕様の正本
- [architecture.md](./architecture.md) — システム境界と責務
- [layer-responsibilities.md](./layer-responsibilities.md) — レイヤー判断基準
- [index.md](./index.md) — docs 全体の入口
