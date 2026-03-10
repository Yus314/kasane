# 外部プラグイン作成ガイド

Kasane のプラグインシステムは外部クレートから利用できます。`kasane-core` に依存するだけで独自プラグインを作成し、`kasane::run()` を通じてカスタムバイナリとして配布できます。

## クレートのセットアップ

```toml
# Cargo.toml
[package]
name = "kasane-my-plugin"
version = "0.1.0"
edition = "2024"

[dependencies]
kasane = { path = "../kasane" }      # or git/registry
kasane-core = { path = "../kasane-core" }
```

## プラグインの定義

`kasane_core::plugin_prelude` をインポートし、`#[kasane_plugin]` マクロでプラグインモジュールを定義します。

```rust
use kasane::kasane_core::plugin_prelude::*;

#[kasane_plugin]
mod my_plugin {
    use kasane::kasane_core::plugin_prelude::*;

    // プラグイン固有の状態（省略可能、Default 必須）
    #[state]
    #[derive(Default)]
    pub struct State {
        pub counter: u32,
    }

    // ライフサイクルフック（全て省略可能）
    pub fn on_init(state: &mut State, _core: &AppState) -> Vec<Command> {
        state.counter = 0;
        vec![]
    }

    pub fn on_shutdown(_state: &mut State) {
        // クリーンアップ
    }

    pub fn on_state_changed(
        _state: &mut State,
        _core: &AppState,
        _dirty: DirtyFlags,
    ) -> Vec<Command> {
        vec![]
    }

    // Slot: 名前付き挿入点に Element を注入
    #[slot(Slot::BufferLeft)]
    pub fn gutter(_state: &State, core: &AppState) -> Option<Element> {
        let n = core.lines.len();
        Some(Element::text(format!("{n}"), Face::default()))
    }
}
```

## バイナリとして登録

```rust
// src/main.rs
fn main() {
    kasane::run(|registry| {
        registry.register(Box::new(MyPluginPlugin::new()));
    });
}
```

マクロはモジュール名を PascalCase に変換し `Plugin` を付与します:
- `my_plugin` → `MyPluginPlugin`
- `line_numbers` → `LineNumbersPlugin`

## 拡張ポイント

### Slot（挿入点）

`#[slot(Slot::XXX)]` で名前付きスロットに UI 要素を注入します。

| Slot | 位置 |
|------|------|
| `BufferLeft` | バッファの左（ガター） |
| `BufferRight` | バッファの右 |
| `AboveBuffer` | バッファの上 |
| `BelowBuffer` | バッファの下 |
| `AboveStatus` | ステータスバーの上 |
| `StatusLeft` | ステータスバー左 |
| `StatusRight` | ステータスバー右 |
| `Overlay` | オーバーレイ（フローティング UI） |

```rust
#[slot(Slot::Overlay)]
pub fn overlay(state: &State, _core: &AppState) -> Option<Element> {
    if state.show_panel {
        Some(Element::text("panel", Face::default()))
    } else {
        None
    }
}
```

### Decorator（修飾）

既存の要素をラップして見た目を変更します。`priority` で適用順を制御します（小さいほど先に適用）。

```rust
#[decorate(DecorateTarget::Buffer, priority = 10)]
pub fn decorate(_state: &State, element: Element, _core: &AppState) -> Element {
    element // 修飾した Element を返す
}
```

対象: `DecorateTarget::Buffer`, `DecorateTarget::StatusBar`

### Replacement（置換）

コンポーネント全体を置き換えます。

```rust
#[replace(ReplaceTarget::StatusBar)]
pub fn replace(_state: &State, _core: &AppState) -> Option<Element> {
    Some(Element::text("custom status", Face::default()))
}
```

対象: `ReplaceTarget::StatusBar`, `ReplaceTarget::Menu`, `ReplaceTarget::Info`

### LineDecoration（行装飾）

バッファの各行に対してガターや背景色を追加します。

```rust
pub fn contribute_line(state: &State, line: usize, _core: &AppState) -> Option<LineDecoration> {
    if line == state.active_line {
        Some(LineDecoration {
            left_gutter: Some(Element::text("→", Face::default())),
            right_gutter: None,
            background: Some(Face {
                bg: Color::Rgb { r: 40, g: 40, b: 50 },
                ..Face::default()
            }),
        })
    } else {
        None
    }
}
```

### Input（入力フック）

キー入力やマウスイベントを監視・処理します。

```rust
// 全てのキー入力を監視（消費しない）
pub fn observe_key(_state: &mut State, _event: &KeyEvent, _core: &AppState) -> Vec<Command> {
    vec![]
}

// キー入力をハンドル（消費する場合は Some を返す）
pub fn handle_key(state: &mut State, event: &KeyEvent, _core: &AppState) -> Option<Vec<Command>> {
    None // None = 処理しない、Some = 処理して消費
}

// マウスイベント（observe_mouse, handle_mouse も同様）
```

### MenuTransform（メニュー変換）

メニュー項目を変換します（例: アイコン追加）。

```rust
pub fn transform_menu_item(_state: &State, item: &str, _core: &AppState) -> Option<String> {
    Some(format!("★ {item}"))
}
```

## コマンド

プラグインから Kakoune にコマンドを送信したり、他のプラグインと通信できます。

```rust
vec![
    Command::SendToKakoune("echo hello".to_string()),
    Command::RequestRedraw(DirtyFlags::BUFFER),
    Command::PluginMessage {
        target: PluginId("other_plugin".to_string()),
        payload: Box::new(42u32),
    },
]
```

## テスト

`PluginRegistry` を直接使ってユニットテストが書けます。

```rust
#[test]
fn my_plugin_contributes_gutter() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(MyPluginPlugin::new()));

    let state = AppState::default();
    let _ = registry.init_all(&state);

    let elements = registry.collect_slot(Slot::BufferLeft, &state);
    assert_eq!(elements.len(), 1);
}
```

## サンプル

[`examples/line-numbers/`](../examples/line-numbers/) に行番号プラグインの完全な実装例があります。
