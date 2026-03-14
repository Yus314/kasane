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
- `#4138` は closed。必要な JSON UI プロトコル拡張は未提供
- `PR #4737` は `PR #5455` に吸収され、追跡対象から外す

## 3. 完全ブロック

上流の変更なしには完全実装できない項目。

| ID | 項目 | 欠けている情報 / 機能 | ローカル回避の限界 | upstream | 再統合先 |
|----|------|------------------------|--------------------|----------|----------|
| E-020 | スクロールバー | スクロール位置、総行数、ハンドル比率に必要な情報 | カーソル位置からの推定ではビューポート非追従時に破綻 | [PR #5304](https://github.com/mawww/kakoune/pull/5304), [#165](https://github.com/mawww/kakoune/issues/165) | Phase 5 系プラグイン |
| E-021 | スクロールバーアノテーション | E-020 に加え、検索結果やエラーの全体位置 | スクロールバー本体がないため成立しない | [PR #5304](https://github.com/mawww/kakoune/pull/5304), [#2727](https://github.com/mawww/kakoune/issues/2727) | E-020 と同時 |
| R-052 | 画面外カーソルインジケータ | ビューポート外カーソル数と位置 | view 内で見えているカーソルしか検出できない | [#2727](https://github.com/mawww/kakoune/issues/2727), [#5425](https://github.com/mawww/kakoune/issues/5425) | Phase 4 系プラグイン |
| E-040 | アンダーラインバリエーション | underline style の種別 | on/off しか来ないため描き分け不能 | [#4138](https://github.com/mawww/kakoune/issues/4138) | GUI / renderer 拡張 |

## 4. 品質制限つきでしか回避できない項目

ローカル実装は可能だが、ヒューリスティック依存または upstream 挙動の確認不足により現時点では正本にしない項目。

| ID | 項目 | 現在の状況 | なぜ未採用か | upstream | 次の一手 |
|----|------|------------|---------------|----------|----------|
| R-062 | ステータスラインコンテキスト推定 | face 名や文字列で推定は可能 | カスタム face やメッセージ構成で壊れる | [#5428](https://github.com/mawww/kakoune/issues/5428) | context 種別が入るまで deferred |
| R-027 | 起動時 info キューイング | ローカルキューで回避できる可能性あり | 上流側の起動時挙動をまだ切り分け中 | [#5294](https://github.com/mawww/kakoune/issues/5294) | upstream 挙動確認後に Phase 4 へ戻す |
| E-002 | ガターアイコン (完全版) | `widget_columns` は利用可能。部分実証も済み | atom の意味種別がなく、行番号 / 仮想テキスト / コードの厳密区別ができない | [PR #4707](https://github.com/mawww/kakoune/pull/4707), [#4687](https://github.com/mawww/kakoune/issues/4687) | semantic type 追加後に再統合 |
| E-001 | オーバーレイレイヤー (完全版) | overlay 自体は部分実証済み。`widget_columns` も利用可能 | バッファ内の意味位置が atom ambiguity に依存する | [PR #4707](https://github.com/mawww/kakoune/pull/4707), [#4687](https://github.com/mawww/kakoune/issues/4687) | semantic type 追加後に再統合 |

## 5. Upstream watchlist

2026-03-14 時点で追跡している upstream 項目:

| 上流 ID | 内容 | 影響項目 | 状態 |
|---------|------|----------|------|
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | JSON UI に face / semantic type 相当の追加 | E-001, E-002, C-008 系 | Open |
| [PR #5455](https://github.com/mawww/kakoune/pull/5455) | `draw` に `widget_columns` 追加 | E-001, E-002 | Merged (2026-03-11) |
| [PR #5304](https://github.com/mawww/kakoune/pull/5304) | scroll position protocol | E-020, E-021 | Open |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | `draw_status` context | R-062 | Open |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | incremental `draw` | NF-004 の上流版 | Open |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | atom type ambiguity | E-001, E-002, C-008 系 | Open |
| [#5294](https://github.com/mawww/kakoune/issues/5294) | 起動時 `info` 表示 | R-027 | Open |
| [#4138](https://github.com/mawww/kakoune/issues/4138) | fancy underline variations | E-040 | Closed |

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
