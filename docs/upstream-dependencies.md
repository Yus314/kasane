# 上流依存項目 (Kakoune プロトコル)

本ドキュメントは、Kakoune 上流の変更なしには完全実装できない項目を追跡する tracker である。
制約の分析自体は [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md) を正本とする。

## 1. 文書の責務

本ドキュメントは、Kasane が Kakoune 上流の変更なしには完全実装できない項目を追跡するための tracker である。

この文書では次だけを扱う。
- 何がブロックされているか
- どの upstream PR / Issue を見るべきか
- いつロードマップへ戻せるか

詳細な制約分析は [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md)、
要件との対応は [requirements-traceability.md](./requirements-traceability.md)、
実装順序は [roadmap.md](./roadmap.md) を参照。

## 2. 現在のスナップショット

2026-03-14 時点の upstream 状態:
- `PR #5455` は 2026-03-11 に merge 済み
- `PR #4707`, `PR #5304` は open
- `#5428`, `#4686`, `#4687`, `#5294` は open
- `#4138` は closed。P-060 / 装飾拡張系ユースケースは upstream blocker ではなく、Kasane 側の描画実装とエコシステム側の課題として扱う
- `PR #4737` は `PR #5455` に吸収され、追跡対象から外す

## 3. 完全ブロック

上流の変更なしには完全実装できない項目。

| ID | 項目 | 欠けている情報 / 機能 | ローカル回避の限界 | upstream | 再統合先 |
|----|------|------------------------|--------------------|----------|----------|
| D-004 | 右側ナビゲーション UI の完全性 | スクロール位置、総行数、ハンドル比率に必要な情報 | カーソル位置からの推定ではビューポート非追従時に破綻 | [PR #5304](https://github.com/mawww/kakoune/pull/5304), [#165](https://github.com/mawww/kakoune/issues/165) | `P-012`, 右側 UI 系ユースケース |
| D-002 | 画面外カーソル / 選択範囲の補助表示 | ビューポート外カーソル数と位置 | view 内で見えているカーソルしか検出できない | [#2727](https://github.com/mawww/kakoune/issues/2727), [#5425](https://github.com/mawww/kakoune/issues/5425) | `D-002` 再統合 |

## 4. 品質制限つきでしか回避できない項目

ローカル実装は可能だが、ヒューリスティック依存または upstream 挙動の確認不足により現時点では正本にしない項目。

| ID | 項目 | 現在の状況 | なぜ未採用か | upstream | 次の一手 |
|----|------|------------|---------------|----------|----------|
| D-003 | ステータスライン文脈推定 | face 名や文字列で推定は可能 | カスタム face やメッセージ構成で壊れる | [#5428](https://github.com/mawww/kakoune/issues/5428) | context 種別が入るまで deferred |
| D-001 | 起動時 info の保持 | ローカルキューで回避できる可能性あり | 上流側の起動時挙動をまだ切り分け中 | [#5294](https://github.com/mawww/kakoune/issues/5294) | upstream 挙動確認後に再統合 |
| P-010 / P-011 | 補助領域寄与の完全版 | `widget_columns` は利用可能。部分実証も済み | atom の意味種別がなく、行番号 / 仮想テキスト / コードの厳密区別ができない | [PR #4707](https://github.com/mawww/kakoune/pull/4707), [#4687](https://github.com/mawww/kakoune/issues/4687) | semantic type 追加後に再統合 |
| P-001 | オーバーレイ合成 (完全版) | overlay 自体は部分実証済み。`widget_columns` も利用可能 | バッファ内の意味位置が atom ambiguity に依存する | [PR #4707](https://github.com/mawww/kakoune/pull/4707), [#4687](https://github.com/mawww/kakoune/issues/4687) | semantic type 追加後に再統合 |

## 5. Upstream watchlist

2026-03-14 時点で追跡している upstream 項目:

| 上流 ID | 内容 | 影響項目 | 状態 |
|---------|------|----------|------|
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | JSON UI に face / semantic type 相当の追加 | P-001, P-010, P-011, C-008 系 | Open |
| [PR #5455](https://github.com/mawww/kakoune/pull/5455) | `draw` に `widget_columns` 追加 | P-001, P-010, P-011 | Merged (2026-03-11) |
| [PR #5304](https://github.com/mawww/kakoune/pull/5304) | scroll position protocol | D-004, P-012 | Open |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | `draw_status` context | D-003 | Open |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | incremental `draw` | NF-004 の上流版 | Open |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | atom type ambiguity | P-001, P-010, P-011, C-008 系 | Open |
| [#5294](https://github.com/mawww/kakoune/issues/5294) | 起動時 `info` 表示 | D-001 | Open |

## 6. 再統合ルール

次の条件を満たしたら、項目を [roadmap.md](./roadmap.md) へ戻す。

1. 必要な upstream PR / protocol change が merge 済み、または upstream 挙動が十分に確認できた
2. Kasane 側の parser / state / render が新情報を取り込める
3. ローカルのヒューリスティック回避を削除または縮退できる
4. [requirements-traceability.md](./requirements-traceability.md) と [roadmap.md](./roadmap.md) の状態を更新する

## 7. 関連文書

- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md) — 制約の分析
- [roadmap.md](./roadmap.md) — Kasane 側の未完了項目
- [requirements-traceability.md](./requirements-traceability.md) — 要件との対応
- [json-ui-protocol.md](./json-ui-protocol.md) — プロトコル参照仕様
