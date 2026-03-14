# Kakoune JSON UI プロトコル仕様

本ドキュメントは、Kasane が前提とする Kakoune JSON UI プロトコルの参照仕様である。
制約の分析は [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md)、Kasane 側の意味論は [semantics.md](./semantics.md) を参照。

Kakoune の JSON-RPC 2.0 ベースの外部 UI プロトコルの仕様書。

## データ構造

### Face
```json
{
  "fg": "Color",
  "bg": "Color",
  "underline": "Color",
  "attributes": ["Attribute", "..."]
}
```

**Color:** `"default"` | 名前付き色 (`"red"`, `"blue"`, ...) | HEX (`"#rrggbb"`)

**Attribute:** `"underline"` | `"curly_underline"` | `"double_underline"` | `"reverse"` | `"blink"` | `"bold"` | `"dim"` | `"italic"` | `"strikethrough"` | `"final_fg"` | `"final_bg"` | `"final_attr"`

### Atom
```json
{
  "face": "Face",
  "contents": "string"
}
```

### Line
```json
["Atom", "Atom", "..."]
```

### Coord
```json
{
  "line": 0,
  "column": 0
}
```

## Kakoune → Kasane メッセージ

| メソッド | パラメータ |
|---------|-----------|
| `draw` | `[lines: Line[], cursor_pos: Coord, default_face: Face, padding_face: Face, widget_columns: int]` |
| `draw_status` | `[prompt: Line, content: Line, cursor_pos: int, mode_line: Line, default_face: Face]` |
| `menu_show` | `[items: Line[], anchor: Coord, selected_item_face: Face, menu_face: Face, style: string]` |
| `menu_select` | `[selected: int]` |
| `menu_hide` | `[]` |
| `info_show` | `[title: Line, content: Line[], anchor: Coord, face: Face, style: string]` |
| `info_hide` | `[]` |
| `set_ui_options` | `[options: Map<string, string>]` |
| `refresh` | `[force: bool]` |

## Kasane → Kakoune メッセージ

| メソッド | パラメータ |
|---------|-----------|
| `keys` | `[key: string, ...]` |
| `resize` | `[rows: int, columns: int]` |
| `scroll` | `[amount: int, line: int, column: int]` |
| `mouse_press` | `[button: string, line: int, column: int]` |
| `mouse_release` | `[button: string, line: int, column: int]` |
| `mouse_move` | `[line: int, column: int]` |
| `menu_select` | `[index: int]` |

## 上流プロトコル拡張の追跡

Kasane の機能を強化する未マージの上流 PR/Issue。

| PR/Issue | 内容 | Kasane への影響 | 状態 |
|----------|------|----------------|------|
| [Commit 3dd6f30d](https://github.com/mawww/kakoune/commit/3dd6f30d) | `set_cursor` 削除、`cursor_pos` を `draw`/`draw_status` に統合 | **対応済み** | Merged |
| [PR #5455](https://github.com/mawww/kakoune/pull/5455) | `draw` に `widget_columns` パラメータ追加 | **対応済み** — ガター/コンテンツ分離に利用可能 | Merged |
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | Face 名の追加 | Face の意味的区別 (PrimaryCursor 等) が可能に | Open |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | インクリメンタル draw | 差分描画の効率化 | Open |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Atom 種別の区別 | 行番号/仮想テキスト/コードの分離描画 | Open |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | ステータスラインコンテキスト | コマンド/検索/情報の区別 | Open |

## 関連文書

- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md) — 制約の分析
- [upstream-dependencies.md](./upstream-dependencies.md) — upstream watchlist
- [semantics.md](./semantics.md) — Kasane 側の意味論
- [architecture.md](./architecture.md) — プロトコルが入る位置
