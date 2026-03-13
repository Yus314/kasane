# Kakoune プロトコル制約分析 — Kasane への影響と実装の歪み

## 1. 概要

本ドキュメントでは、Kakoune の JSON UI プロトコル (`kak -ui json`) が Kasane の設計・実装に及ぼす制約を体系的に分析する。単なる制約の列挙（[requirements.md §6](./requirements.md) を参照）ではなく、**制約がどのように実装を歪めているか**、**上流での解決見込み**、および **Kasane 側の戦略的対応** を明らかにすることを目的とする。

**関連ドキュメント:**
- [要件定義書 §6 既知の制約事項](./requirements.md) — 制約の簡潔な一覧
- [Kakoune Issue 調査報告書](./kakoune-issues-investigation.md) — 解決可能な課題の全体像
- [JSON UI プロトコル仕様](./json-ui-protocol.md) — プロトコルの技術仕様
- [技術的意思決定記録](./decisions.md) — 制約に起因する設計判断

---

## 2. プロトコルの根本的な設計思想とその帰結

Kakoune の JSON UI プロトコルは、本質的に **「ターミナルエスケープシーケンスの JSON 表現」** である。Kakoune の内蔵ターミナル UI (`terminal_ui.cc`) が行っている描画命令を、ほぼそのまま JSON-RPC メッセージとして送信する設計になっている。

この設計思想は以下の帰結をもたらす:

1. **表示命令のみ、意味情報なし**: プロトコルは「何を描け」とだけ伝え、「これが何か」は伝えない
2. **フロントエンドは受動的**: Kakoune に対して能動的に情報を問い合わせる手段がない
3. **座標系が暗黙的**: ターミナルのセル座標を前提とし、バッファ座標との対応が不明

上流 Issue [#2019](https://github.com/mawww/kakoune/issues/2019) (2018年〜、7コメント) がこの問題を包括的に記録しており、kakoune-gtk の Screwtapello、Kakoune Qt の casimir を含む複数のフロントエンド開発者が議論に参加している。

---

## 3. 実装の歪みの分類

Kasane が受ける歪みは3つの層に分類できる。

### 3.1 推定層 — Kakoune が明示しない情報のヒューリスティック推定

プロトコルが伝えない意味情報を、表示データのパターンマッチで推定する層。**Kakoune の内部実装に暗黙的に依存**するため、バージョンアップで予告なく壊れるリスクがある。

### 3.2 二重計算層 — Kakoune と独立した再計算

Kakoune が内部で持つ計算結果（文字幅、メニュースクロール位置など）がプロトコルに含まれないため、Kasane 側で独立に再計算する層。**精度の乖離**がレイアウトのずれとして顕在化するリスクがある。

### 3.3 迂回層 — プロトコルを迂回した直接アクセス

プロトコルが対応しない機能を、OS API 等に直接アクセスして実現する層。Kakoune 側の状態と**同期できない**。

---

## 4. 推定層の詳細分析

### 4.1 カーソル検出 — `FINAL_FG + REVERSE` ヒューリスティック

**制約:** Kakoune は `draw` メッセージの `cursor_pos` でカーソル座標を伝えるが、以下の情報を提供しない:
- マルチカーソルの総数
- カーソルの種別 (Primary / Secondary)
- カーソルの面名 (PrimaryCursor / SecondaryCursor 等)

**実装の歪み** (`kasane-core/src/state/apply.rs:13-21`):

```rust
self.cursor_count = lines
    .iter()
    .flat_map(|line| line.iter())
    .filter(|atom| {
        atom.face.attributes.contains(Attributes::FINAL_FG)
            && atom.face.attributes.contains(Attributes::REVERSE)
    })
    .count();
```

`FINAL_FG` + `REVERSE` 属性の同時存在をカーソルのシグネチャとして利用している。これは Kakoune の `terminal_ui.cc` がカーソル位置の Atom に `FINAL_FG | REVERSE` を設定するという内部実装の知識に依存している。

**影響範囲:**
- R-050 (マルチカーソル描画) — Primary/Secondary の区別不可
- R-064 (カーソル数バッジ) — 表示上は機能するが保証なし

**上流での解決:** [PR #4707](https://github.com/mawww/kakoune/pull/4707) (Atom にセマンティックな Face 名を追加)。ただし mawww は [PR #4737](https://github.com/mawww/kakoune/pull/4737) の DisplayAtom フラグによるアプローチを推奨しており、#4707 自体のマージ見込みは不透明。

---

### 4.2 編集モード推定 — ステータスモードラインの文字列マッチ

**制約:** Kakoune は現在の編集モード (normal / insert / replace) を明示的に通知するメッセージを持たない。

**実装の歪み** (`kasane-core/src/render/mod.rs:74-100`):

```rust
pub fn cursor_style(state: &AppState) -> CursorStyle {
    // 1. ui_option による明示的オーバーライド
    if let Some(style) = state.ui_options.get("kasane_cursor_style") { ... }
    // 2. フォーカス喪失時
    if !state.focused { return CursorStyle::Outline; }
    // 3. プロンプトモード
    if state.cursor_mode == CursorMode::Prompt { return CursorStyle::Bar; }
    // 4. モードラインの文字列マッチによる推定
    let mode = state.status_mode_line.iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}
```

ステップ4でモードラインの Atom 内容が文字列 `"insert"` / `"replace"` に一致するかを検査している。ユーザーが `modelinefmt` を変更して日本語のモード名を表示した場合や、プラグインがモードラインを改変した場合に壊れる。

**緩和策:** `kasane_cursor_style` ui_option による明示的オーバーライドを最優先にすることで、ヒューリスティックが失敗した場合のフォールバックを提供。

---

### 4.3 ステータスラインのコンテキスト推定 (先送り中)

**制約:** `draw_status` メッセージは `status_line` と `mode_line` の2つの `Line` を送るのみで、それが以下のいずれであるかを示さない:
- コマンドプロンプト (`:`)
- 検索プロンプト (`/`, `?`)
- `echo` によるメッセージ
- ファイル情報表示

**影響:** R-062 (コンテキスト推定) が先送りされている。プロンプトの種類に応じた UI 分岐（コマンドパレット風の表示、検索ハイライトなど）が実装不能。

**上流:** [#5428](https://github.com/mawww/kakoune/issues/5428) — `draw_status` に `status_style` パラメータを追加する提案。コメント0件で議論が進んでいない。

**他フロントエンドの状況:** [#2019](https://github.com/mawww/kakoune/issues/2019) で casimir が `:` プレフィックスの検出を試みたが、信頼性が低いと報告。

---

### 4.4 Info ポップアップの同一性判定

**制約:** Kakoune は info ウィンドウに一意な ID を付与しない。`info_show` / `info_hide` は単一のスタック的操作を想定している。

**実装の歪み** (`kasane-core/src/state/mod.rs:181-197`):

```rust
pub struct InfoIdentity {
    pub style: InfoStyle,
    pub anchor_line: u32,
}
```

`(InfoStyle, anchor_line)` のタプルを近似的な ID として使用し、同一 identity の info は上書き、異なる identity は共存させている。

**既知の衝突パターン:**
- 同一行上の lint エラーと LSP ホバー情報（両方 `Inline` スタイル）
- 複数の `Modal` スタイル info（anchor_line が同じ場合）

**上流:** [#1516](https://github.com/mawww/kakoune/issues/1516) — 複数 info ボックスの同時表示。根本解決には Kakoune 側での info ID 導入が必要。

---

## 5. 二重計算層の詳細分析

### 5.1 文字幅の独立計算

**制約 (C-002):** Atom は文字列のみを含み、表示幅情報を持たない。

**二重計算の構造:**

| 計算主体 | 幅計算ソース | 用途 |
|---------|------------|------|
| Kakoune | libc の `wcwidth()` / `wcswidth()` | バッファ内カーソル移動、行折り返し判定、Atom 分割 |
| Kasane | `unicode-width` クレート + 互換パッチ | レイアウト計算、セルグリッド配置 |

**乖離リスク:**
- libc 版の Unicode データベースと `unicode-width` クレートの Unicode バージョンが異なる
- 特に CJK 曖昧幅文字 (Ambiguous Width) の解釈差異
- 絵文字シーケンス (ZWJ, Variation Selector) の幅計算差異
- macOS の `iswprint()` が古い Unicode データベースに依存 ([#4257](https://github.com/mawww/kakoune/issues/4257))

**顕在化の例:**
- Kasane が2セル幅と判定した文字を Kakoune が1セル幅と扱うと、カーソル位置がずれる
- メニュー内の CJK テキストでアイテム境界がずれる ([#3598](https://github.com/mawww/kakoune/issues/3598))

**上流:** [#2019](https://github.com/mawww/kakoune/issues/2019) で Screwtapello が「Atom に期待幅を含めるべき」と提案。mawww は未回答。

---

### 5.2 メニュースクロール位置の再計算

**制約:** Kakoune は `menu_select(index)` で選択インデックスのみを伝え、スクロール位置は伝えない。

**実装の歪み:** Kasane は `MenuState::scroll_column_based()` および `MenuState::scroll_search()` で Kakoune の `terminal_ui.cc` のスクロールロジックを Rust に移植している。

```
// Kakoune の terminal_ui.cc を逆算:
// stride = win_height
// first_item = (selected / stride) * stride
```

Kakoune 側のロジックが変更された場合、メニューのスクロール位置がずれる。

---

### 5.3 インクリメンタルな差分検出

**制約 (C-007):** `draw` メッセージは毎回すべての表示行を送信する。変更行のみの差分送信は行われない。

**二重計算:** Kasane は NF-004 (差分描画) として、前フレームの `CellGrid` と現フレームを比較し、変更セルのみをバックエンドに送信する。Kakoune の内部でも同様の差分検出を行っている（ターミナル UI 用）が、その結果はプロトコルに含まれない。

**上流:** [#4686](https://github.com/mawww/kakoune/issues/4686) — インクリメンタル draw 通知の提案。コメント0件。

---

## 6. 迂回層の詳細分析

### 6.1 クリップボード

**制約 (C-001):** プロトコルにクリップボード関連のメッセージが存在しない。

**迂回:** `arboard` クレートでシステムクリップボード API に直接アクセス (R-080)。

**同期問題:**
- Kakoune の yank レジスタ (`"`) と Kasane のクリップボードは独立
- Kakoune 内で `y` した内容は Kasane のクリップボードに反映されない
- Kasane 経由でペーストした内容は Kakoune の `"` レジスタに入らない

[#2019](https://github.com/mawww/kakoune/issues/2019) で Screwtapello がクリップボード連携の5つのシナリオを列挙し、すべてが JSON UI プロトコルでは不可能であると指摘。

**上流:** [#3935](https://github.com/mawww/kakoune/issues/3935) — ビルトインクリップボード統合の要望。

---

### 6.2 マウス修飾キー

**制約 (C-004):** マウスイベントのプロトコルメッセージに修飾キーフィールドがない。

```rust
// プロトコル上のマウスイベント:
MousePress { button: String, line: u32, column: u32 }
// ← Ctrl/Alt/Shift の情報なし
```

**迂回:** Kasane はマウスイベント受信時に OS のキー状態を検査し、`Ctrl+Click` 等をフロントエンド側で独自処理する。Kakoune にはこの修飾キー情報を伝える手段がない。

---

## 7. プロトコルで原理的に不可能な操作

以下は、プロトコルの設計上、Kasane から行うことが**不可能**な操作である。

| 操作 | 現在の代替手段 | 限界 |
|------|-------------|------|
| コマンド実行 (`evaluate-commands`) | `keys` メッセージでキー入力をシミュレート | 複雑なコマンドの発行が困難。実行結果を取得できない |
| バッファメタデータ取得 | なし | ファイルパス、変更状態、開いているバッファ一覧を知る手段がない |
| レジスタ監視 | なし | yank/delete の内容変化を検知できない |
| バッファ内容の任意範囲取得 | なし | 画面に表示されている部分しかアクセスできない |
| ビューポート位置の取得 | `resize` メッセージの送信のみ | バッファの何行目から表示しているかが不明 |
| コマンド実行の応答確認 | なし | `keys` 送信に対する ACK がない (fire-and-forget) |
| オプション値の能動的取得 | `set_ui_options` の受信待ち | 特定オプションの値を問い合わせることができない |

---

## 8. 上流 Issue/PR の現状と解決見込み

### 8.1 追跡中の上流 PR

| PR | タイトル | 状態 | 最終更新 | 見込み |
|----|---------|------|---------|-------|
| [#4707](https://github.com/mawww/kakoune/pull/4707) | JSON UI に Face 名を追加 | OPEN | 2026-01 | **停滞**。mawww は DisplayAtom フラグを推奨。6コメント |
| [#4737](https://github.com/mawww/kakoune/pull/4737) | draw に DisplaySetup コンテキスト追加 | CLOSED | 2026-01 | PR #5455 に統合 |
| [PR #5455](https://github.com/mawww/kakoune/pull/5455) | `set_cursor` 削除 + `widget_columns` 追加 | MERGED | 2026-03 | **対応済み** |

**分析:** PR #5455 がマージされ、`set_cursor` メッセージは削除された。`cursor_pos` は `draw` と `draw_status` に統合され、`widget_columns` が `draw` に追加された。PR #4707 は mawww が別アプローチを推奨しているため、そのままではマージされない見込み。

### 8.2 追跡中の上流 Issue

| Issue | タイトル | 状態 | コメント数 | 見込み |
|-------|---------|------|-----------|-------|
| [#2019](https://github.com/mawww/kakoune/issues/2019) | JSON UI の制限事項まとめ | OPEN | 7 | 総合スレッド。個別 PR で段階的に改善 |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | ステータスラインコンテキスト | OPEN | 0 | 議論未開始。提案は合理的 |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | インクリメンタル draw | OPEN | 0 | 議論未開始。大規模変更のため見込み低 |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Atom 種別の区別 | OPEN | 0 | 議論未開始。#4737 で部分的に解決される可能性 |

---

## 9. Kasane の戦略的対応

### 9.1 短期 (Phase 4a)

- **現行のヒューリスティックを維持**しつつ、壊れやすいポイントを明確にドキュメント化する（本ドキュメント）
- `kasane_cursor_style` のような **ui_option によるフォールバック機構**を活用し、ヒューリスティック失敗時のユーザー制御を担保する
- 先送り項目 (R-027, R-050, R-052, R-062) は上流 PR の進展を待つ

### 9.2 中期 (Phase 4b 以降)

- **`widget_columns` 利用** (PR #5455 マージ済み): ガター/コンテンツの分離を実装可能。E-001 (オーバーレイ), E-002 (ガターアイコン) の精度向上に活用
- **PR #5428 が進展した場合**: `status_style` を利用して R-062 のヒューリスティック推定を廃止
- **上流が進まない場合**: Kakoune パッチのフォークを検討するか、Kasane 側のヒューリスティックをより堅牢にする

### 9.3 上流への貢献

- PR #4737 のレビュー・フィードバックへの参加を優先
- #5428 に対する実装パッチの提案（`draw_status` に `status_style` を追加する最小限の変更）
- Kasane の実装経験に基づく、プロトコル改善の具体的ユースケース提供

---

## 10. 影響度マトリクス

各制約が Kasane のどの機能をブロックしているかを整理する。

| 制約 | ブロックされている機能 | 歪みの層 | 深刻度 |
|------|---------------------|---------|-------|
| カーソル種別なし | R-050 マルチカーソル描画 | 推定 | **高** — 壊れると全カーソル描画が崩壊 |
| 編集モード通知なし | カーソルスタイル自動切替 | 推定 | 中 — ui_option フォールバックあり |
| ステータスコンテキストなし | R-062 コンテキスト推定 | 推定 | 中 — 先送り中 |
| Info ID なし | 複数 info の正確な管理 | 推定 | 低 — 衝突は稀なケース |
| 文字幅情報なし | 全テキストレイアウト | 二重計算 | **高** — 乖離はカーソル位置ずれとして顕在化 |
| スクロール位置なし | メニュー表示 | 二重計算 | 中 — 実装済みだが Kakoune 変更で壊れうる |
| インクリメンタル draw なし | パフォーマンス | 二重計算 | 低 — 現状で 60fps を維持 |
| クリップボード通知なし | クリップボード同期 | 迂回 | 中 — 片方向は機能 |
| マウス修飾キーなし | Ctrl+Click 等 | 迂回 | 低 — フロントエンド側で対処可能 |
| コマンド実行 RPC なし | バッファ操作の抽象化 | 原理的不可能 | **高** — 代替手段なし |
| ビューポート位置なし | R-052 画面外カーソル, E-023 表示行ナビゲーション | 原理的不可能 | **高** — PR #4737 待ち |

---

## 付録 A: 他フロントエンドプロジェクトの対処法

| プロジェクト | 技術 | 制約への対処 |
|------------|------|------------|
| [kakoune-gtk](https://gitlab.com/Screwtapello/kakoune-gtk) | GTK | #2019 の議論をリード。プロトコル改善を上流に要求 |
| [Kakoune Qt](https://discuss.kakoune.com/t/announcing-kakoune-qt/2522) | Qt | 分割、ボーダー、マルチフォントサイズを独自実装 |
| [kak-ui](https://docs.rs/kak-ui/latest/kak_ui/) | Rust crate | プロトコルラッパーのみ。制約は利用者に委ねる |

---

## 付録 B: 上流 Issue 相互参照

本ドキュメントで言及した上流 Issue/PR の完全なリスト。

| 番号 | タイトル | 本文での言及箇所 |
|------|---------|----------------|
| [#2019](https://github.com/mawww/kakoune/issues/2019) | JSON UI の制限事項まとめ | §2, §4.3, §5.1, §6.1 |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | ステータスラインコンテキスト | §4.3, §8.2 |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | インクリメンタル draw 通知 | §5.3, §8.2 |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Atom 種別の区別 | §8.2 |
| [#1516](https://github.com/mawww/kakoune/issues/1516) | 複数 info ボックスの同時表示 | §4.4 |
| [#3935](https://github.com/mawww/kakoune/issues/3935) | ビルトインクリップボード統合 | §6.1 |
| [#3598](https://github.com/mawww/kakoune/issues/3598) | CJK 文字の補完候補表示崩れ | §5.1 |
| [#4257](https://github.com/mawww/kakoune/issues/4257) | macOS 絵文字問題 | §5.1 |
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | JSON UI に Face 名追加 | §4.1, §8.1 |
| [PR #4737](https://github.com/mawww/kakoune/pull/4737) | draw に DisplaySetup 追加 | §4.1, §8.1, §9.2 |
