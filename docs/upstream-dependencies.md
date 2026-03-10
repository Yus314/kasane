# 上流依存項目 (Kakoune プロトコル)

Kakoune 上流の変更・PR に依存しており、ロードマップから分離した項目。
上流で解決され次第、該当フェーズに再統合する。

詳細な制約分析は [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md) を参照。

---

## 完全ブロック

上流の変更なしには実装不可能な項目。ヒューリスティック回避も信頼性が不十分。

### E-020: スクロールバー

| | |
|---|---|
| **要件** | プロポーショナルハンドル付きスクロールバー。クリック/ドラッグ対応 |
| **使用する API** | `Slot::BufferRight` (未実証) |
| **ブロッカー** | Kakoune の `draw` メッセージにスクロール位置 (現在行 / 総行数) が含まれない |
| **回避策の限界** | カーソル位置からの推定は、ビューポートがカーソルと離れている場合に不正確 |
| **上流追跡** | [PR #5304](https://github.com/mawww/kakoune/pull/5304) |
| **関連 Issue** | [#165](https://github.com/mawww/kakoune/issues/165) |

### E-021: スクロールバーアノテーション

| | |
|---|---|
| **要件** | 検索結果、エラー、選択範囲の位置をスクロールバー上にマーカー表示 |
| **ブロッカー** | E-020 (スクロールバー本体) に依存。加えて、検索結果やエラーの全バッファ位置情報がプロトコルにない |
| **関連 Issue** | [#2727](https://github.com/mawww/kakoune/issues/2727) |

### R-052: 画面外カーソルインジケータ

| | |
|---|---|
| **要件** | ビューポート外に存在するカーソル/選択範囲の方向と数をビューポート端に表示 |
| **使用する API** | `Slot::BufferTop` / `BufferBottom` (未実証) |
| **ブロッカー** | `draw` メッセージにカーソルの総数が含まれない。ビューポート内のカーソルのみ検出可能 |
| **回避策の限界** | ビューポート外のカーソル数・位置を正確に把握する方法がない |
| **元の分類** | Phase 4b 組み込みプラグイン |

### E-040: アンダーラインバリエーション

| | |
|---|---|
| **要件** | 波線 (curly)・点線 (dotted)・二重線 (double) 等のアンダーラインスタイル描画 |
| **ブロッカー** | Face の `underline` 属性が on/off のみ。バリエーション情報をプロトコルが送信しない |
| **回避策の限界** | バリエーション情報がプロトコルに含まれない限り、どのスタイルを適用すべきか判断不可能 |
| **関連 Issue** | [#4138](https://github.com/mawww/kakoune/issues/4138) |

---

## ヒューリスティック回避可能だが品質に制限

上流の変更なしでもヒューリスティックで部分的に実装可能だが、完全な実装には上流サポートが必要。
現時点ではヒューリスティック版の実装も見送り、上流での解決を待つ方針。

### R-062: ステータスラインコンテキスト推定

| | |
|---|---|
| **要件** | `draw_status` の内容からコマンド/検索/情報メッセージをヒューリスティックに区別 |
| **ブロッカー** | `draw_status` にコンテキスト種別が含まれない |
| **回避策** | face 名やテキストパターンによる推定。ユーザーカスタマイズ (カスタム face) で破綻する |
| **上流追跡** | [#5428](https://github.com/mawww/kakoune/issues/5428) |

### R-027: 起動時 info キューイング

| | |
|---|---|
| **要件** | 起動時に受信した info メッセージをキューイングし、UI 準備完了後に表示 |
| **状態** | 保留 — 上流挙動を検証中 |
| **関連 Issue** | [#5294](https://github.com/mawww/kakoune/issues/5294) |
| **備考** | 上流で起動時の info 表示タイミングが改善される可能性あり。確認後、最小限のコア実装を検討 |

### E-002: ガターアイコン (完全版)

| | |
|---|---|
| **要件** | 行番号ガターにコードアクション、エラー/警告、git diff 等のアイコンをネイティブ描画 |
| **使用する API** | `Slot::BufferLeft` (実証済み — ColorPreviewPlugin) |
| **ブロッカー** | `draw` メッセージの atom に種別 (行番号 / 仮想テキスト / コード) が含まれない。行番号の区別には [PR #4707](https://github.com/mawww/kakoune/pull/4707) / [PR #4737](https://github.com/mawww/kakoune/pull/4737) が必要 |
| **回避策** | `LineNumbers` / `LineNumberCursor` 等の face 名パターンマッチで推定可能だが、カスタム face に非対応 |
| **部分実証** | ColorPreviewPlugin でガタースウォッチ (色コードのある行のみ) は動作済み |

### E-001: オーバーレイレイヤー (完全版)

| | |
|---|---|
| **要件** | メインバッファ上に独立した描画レイヤーを重畳。仮想テキストをバッファ変更なしに表示 |
| **使用する API** | `Slot::Overlay` + `Decorator(Buffer)` |
| **ブロッカー (部分)** | バッファ内の正確な位置にオーバーレイを配置するには、atom の意味 (行番号 vs コード) を区別する必要がある (C-008) |
| **回避策** | face ヒューリスティックで行番号幅を推定し、コード領域のオフセットを計算 |
| **部分実証** | ColorPreviewPlugin のカラーピッカーオーバーレイは動作済み (ただしバッファ左端固定) |

---

## 上流 PR/Issue の追跡状況

| 上流 ID | 内容 | 影響する項目 | 状態 |
|---------|------|-------------|------|
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | atom に type フィールド追加 | E-001, E-002, C-008 | Open |
| [PR #4737](https://github.com/mawww/kakoune/pull/4737) | 行番号 atom の種別区別 | E-002 | Open |
| [PR #5304](https://github.com/mawww/kakoune/pull/5304) | scroll position protocol | E-020, E-021 | Open |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | draw_status context | R-062 | Open |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | incremental draw | NF-004 (回避済み) | Open |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | atom type ambiguity | C-008 | Open |
| [#5294](https://github.com/mawww/kakoune/issues/5294) | 起動時 info 表示 | R-027 | Open |
| [#4138](https://github.com/mawww/kakoune/issues/4138) | underline variations | E-040 | Open |

---

## 再統合の条件

各項目は以下の条件で該当フェーズに再統合する:

1. 上流 PR がマージされ、安定版リリースに含まれる
2. プロトコル変更に対応するパーサー実装を追加
3. ロードマップの該当フェーズに項目を移動し、本ドキュメントから「解決済み」に更新
