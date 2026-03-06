# Kakoune JSON UI プロトコル仕様

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
| `draw` | `[lines: Line[], default_face: Face, padding_face: Face]` |
| `draw_status` | `[status_line: Line, mode_line: Line, default_face: Face]` |
| `set_cursor` | `[mode: "buffer"\|"prompt", coord: Coord]` |
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

| PR/Issue | 内容 | Kasane への影響 |
|----------|------|----------------|
| [PR #4707](https://github.com/mawww/kakoune/pull/4707) | Face 名の追加 | Face の意味的区別 (PrimaryCursor 等) が可能に |
| [PR #4737](https://github.com/mawww/kakoune/pull/4737) | DisplaySetup コンテキスト | バッファ座標・ウィジェット列数の取得 |
| [#4686](https://github.com/mawww/kakoune/issues/4686) | インクリメンタル draw | 差分描画の効率化 |
| [#4687](https://github.com/mawww/kakoune/issues/4687) | Atom 種別の区別 | 行番号/仮想テキスト/コードの分離描画 |
| [#5428](https://github.com/mawww/kakoune/issues/5428) | ステータスラインコンテキスト | コマンド/検索/情報の区別 |
