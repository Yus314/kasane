# Kasane Documentation Index

## 1. 読者別入口

- 利用者として使い始めたい
  - [README.md](../README.md)
  - [config.md](./config.md)
- プラグインを書きたい
  - [plugin-development.md](./plugin-development.md)
  - [plugin-api.md](./plugin-api.md)
  - [semantics.md](./semantics.md)
- コア実装を追いたい
  - [architecture.md](./architecture.md)
  - [repo-layout.md](./repo-layout.md)
  - [semantics.md](./semantics.md)
  - [performance.md](./performance.md)
- 設計判断を確認したい
  - [semantics.md](./semantics.md)
  - [decisions.md](./decisions.md)
  - [layer-responsibilities.md](./layer-responsibilities.md)
  - [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md)
- 進捗やブロッカーを確認したい
  - [requirements-traceability.md](./requirements-traceability.md)
  - [roadmap.md](./roadmap.md)
  - [upstream-dependencies.md](./upstream-dependencies.md)

## 2. 文書カテゴリ

### Current

現行の仕様、責務、使い方を定義する文書。

- [requirements.md](./requirements.md) — コア要件・拡張基盤要件・実証ユースケースを含む要件本文の正本
- [semantics.md](./semantics.md) — 現行意味論の正本
- [architecture.md](./architecture.md) — システム境界と責務
- [layer-responsibilities.md](./layer-responsibilities.md) — 上流 / コア / プラグインの責務判断
- [plugin-development.md](./plugin-development.md) — プラグイン開発ガイド
- [plugin-api.md](./plugin-api.md) — プラグイン API リファレンス
- [config.md](./config.md) — 設定リファレンス
- [json-ui-protocol.md](./json-ui-protocol.md) — JSON UI プロトコル仕様
- [repo-layout.md](./repo-layout.md) — ソースツリーと crate 責務
- [performance.md](./performance.md) — 性能方針と読み方

### Tracking

状態、進捗、ブロッカーを追跡する文書。

- [requirements-traceability.md](./requirements-traceability.md) — コア要件 / 拡張基盤 / 縮退動作ごとの状態と Phase
- [roadmap.md](./roadmap.md) — 実装フェーズと未完了項目
- [upstream-dependencies.md](./upstream-dependencies.md) — 上流依存と再統合条件
- [performance-benchmarks.md](./performance-benchmarks.md) — 性能実測と最適化状況

### Historical / Research

履歴、調査、背景分析を保持する文書。

- [decisions.md](./decisions.md) — ADR と設計判断の履歴
- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md) — プロトコル制約の分析
- [kakoune-issues-investigation.md](./kakoune-issues-investigation.md) — Kakoune 側課題の調査

### Supporting Reference

補助的な参照資料。

- [profiling.md](./profiling.md) — 計測手順
- [glossary.md](./glossary.md) — 用語集

## 3. 読み順

### 新規利用者

1. [README.md](../README.md)
2. [config.md](./config.md)

### 新規実装者

1. [semantics.md](./semantics.md)
2. [architecture.md](./architecture.md)
3. [repo-layout.md](./repo-layout.md)
4. [requirements.md](./requirements.md)

### プラグイン作者

1. [plugin-development.md](./plugin-development.md)
2. [plugin-api.md](./plugin-api.md)
3. [semantics.md](./semantics.md)

### 設計議論

1. [semantics.md](./semantics.md)
2. [decisions.md](./decisions.md)
3. [layer-responsibilities.md](./layer-responsibilities.md)
4. [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md)

## 4. 更新ルール

- 現行仕様が変わったら `Current` の文書を更新する
- 状態や進捗が変わったら `Tracking` の文書を更新する
- 決定理由や履歴を残す場合は `Historical / Research` に追記する
- 同じ内容の正本を複数作らない
