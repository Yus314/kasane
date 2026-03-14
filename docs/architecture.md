# アーキテクチャ設計書

本ドキュメントは Kasane のシステム境界、ランタイム構成、責務分離を説明する。
workspace の詳細なツリーは [repo-layout.md](./repo-layout.md)、状態と描画の意味論は [semantics.md](./semantics.md) を参照。

## システム構成

```text
┌──────────────────────────────────────────────────────────┐
│                   Kasane (フロントエンド)                │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │                 kasane-core                        │  │
│  │  JSON-RPC パーサー / 状態管理 / レイアウトエンジン │  │
│  │  入力マッピング / 設定管理 / RenderBackend trait   │  │
│  └──────────┬───────────────────────┬─────────────────┘  │
│             │                       │                    │
│  ┌──────────▼──────────┐ ┌─────────▼────────────────┐   │
│  │    kasane-tui        │ │     kasane-gui           │   │
│  │  (crossterm 直接)    │ │ (winit + wgpu + glyphon) │   │
│  │  セルグリッド管理     │ │ GPU テキストレンダリング  │   │
│  │  差分描画            │ │ シーンベース描画          │   │
│  └──────────────────────┘ └──────────────────────────┘   │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │              宣言的 UI + Plugin 合成               │  │
│  │  Buffer / Status / Menu / Info / Overlay / Surface │  │
│  └────────────────────────────────────────────────────┘  │
│           ▲ 描画                │ キー/マウス入力        │
│           │ TUI: stdout         ▼ TUI: stdin            │
│           │ GUI: winit + GPU      GUI: winit            │
│  ┌────────────────────────────────────────────────────┐  │
│  │             Kakoune (エディタエンジン)             │  │
│  │             kak -ui json                          │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

## 実行時データフロー

```text
Kakoune message / frontend input
  -> protocol parse / input conversion
  -> state.apply() / update()
  -> DirtyFlags
  -> plugin notification
  -> view construction
  -> layout
  -> paint / scene build
  -> backend draw
```

このフローの意味論的詳細は [semantics.md](./semantics.md) を参照。

## 通信プロトコル

- プロトコル: JSON-RPC 2.0
- Kakoune -> Kasane: `draw`, `draw_status`, `menu_show`, `info_show` などの描画・状態メッセージ
- Kasane -> Kakoune: `keys`, `resize`, `mouse_press` などの入力メッセージ
- 起動形態: `kak -ui json` を子プロセスとして起動し、stdin/stdout を接続する

プロトコルの詳細は [json-ui-protocol.md](./json-ui-protocol.md) を参照。

## 抽象化の境界

コアが管理するのは「何を、どこに表示するか」であり、backend が管理するのは「どう描画するか」である。

### 三層レイヤー責務モデル

| 層 | 定義 | 判断基準 |
|---|---|---|
| 上流 (Kakoune) | プロトコルレベルの関心事 | プロトコル変更が必要か？ |
| コア (`kasane-core`) | プロトコルの忠実なレンダリング + frontend ネイティブ能力 | 唯一の正しい実装が存在するか？ |
| プラグイン | ポリシーが分かれうる機能 | 上記以外 |

詳細な判断基準は [layer-responsibilities.md](./layer-responsibilities.md) を参照。

### 宣言的 UI レイヤーの責務

| コンポーネント | 担当 | 説明 |
|---|---|---|
| `view` 構築 | `kasane-core` | 状態から `Element` ツリーを構築し、plugin 寄与を合成する |
| レイアウト計算 | `kasane-core` | `Element` から矩形配置を計算する |
| TUI paint | `kasane-core` | `Element + LayoutResult -> CellGrid` |
| GUI scene build | `kasane-core` | `Element + LayoutResult -> DrawCommand` |
| plugin dispatch | `kasane-core` | state change と input を plugin hook に配る |
| hit test | `kasane-core` | `InteractiveId` によるマウス対象特定 |

### Backend の責務

| コンポーネント | `kasane-core` | `kasane-tui` | `kasane-gui` |
|---|---|---|---|
| JSON-RPC パース | 担当 | - | - |
| 状態管理 (TEA) | 担当 | - | - |
| `Element` 構築 | 担当 | - | - |
| レイアウト計算 | 担当 | - | - |
| `CellGrid` への paint | 担当 | - | - |
| terminal 出力 | - | crossterm | - |
| GPU 描画 | - | - | wgpu + glyphon |
| キー/マウス入力取得 | - | crossterm | winit |
| クリップボード | - | arboard | arboard |
| IME / D&D など GUI ネイティブ能力 | - | 不可または terminal 依存 | winit ベース |

## レンダリングパス

### TUI パス

```text
view_cached -> place -> paint -> CellGrid -> diff -> backend.draw
```

TUI はセルグリッドベースの差分描画を行い、crossterm でエスケープシーケンスへ変換する。

### GUI パス

```text
view_sections_cached -> scene_paint_section -> SceneCache -> SceneRenderer
```

GUI は `DrawCommand` ベースのシーン記述を生成し、GPU へ直接描画する。

### キャッシュレイヤー

| レイヤー | 対象 | 役割 |
|---|---|---|
| `ViewCache` | `Element` ツリー | セクション別 view 再利用 |
| `LayoutCache` | レイアウト結果 | セクション別再描画支援 |
| `SceneCache` | `DrawCommand` 列 | GUI シーン再利用 |
| `PaintPatch` | `CellGrid` 部分更新 | TUI 高速パス |

各キャッシュの意味論と invalidation policy は [semantics.md](./semantics.md) を参照。

## 関連文書

- [repo-layout.md](./repo-layout.md): workspace と source tree の詳細
- [semantics.md](./semantics.md): 状態、描画、invalidation、等価性
- [plugin-api.md](./plugin-api.md): plugin author 向け API リファレンス
- [plugin-development.md](./plugin-development.md): 最短ガイド
