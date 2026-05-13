#![allow(dead_code)]

use kasane_core::element::{Element, FlexChild, InteractiveId};
use kasane_core::input::{
    InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind,
};
use kasane_core::layout::{Rect, build_hit_map};
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{Coord, InfoStyle};
use kasane_core::state::{AppState, InfoIdentity, InfoState};

pub fn state_80x24() -> AppState {
    kasane_core::test_support::test_state_80x24()
}

pub fn registry_empty() -> PluginRuntime {
    PluginRuntime::new()
}

pub fn make_info_state(anchor_line: u32, anchor_column: u32, lines: &[&str]) -> InfoState {
    InfoState {
        title: kasane_core::test_support::make_line("Info"),
        content: lines
            .iter()
            .map(|line| kasane_core::test_support::make_line(line))
            .collect(),
        anchor: Coord {
            line: anchor_line as i32,
            column: anchor_column as i32,
        },
        face: kasane_core::protocol::Style::default(),
        style: InfoStyle::Prompt,
        identity: InfoIdentity {
            style: InfoStyle::Prompt,
            anchor_line,
        },
        scroll_offset: 0,
    }
}

pub fn install_hit_region(state: &mut AppState, id: InteractiveId, area: Rect) {
    let line = "x".repeat(area.w.max(1) as usize);
    let child = Element::column(
        (0..area.h.max(1))
            .map(|_| FlexChild::fixed(Element::plain_text(line.clone())))
            .collect(),
    );
    let interactive = Element::Interactive {
        child: Box::new(child),
        id,
    };
    let layout = kasane_core::layout::flex::place(&interactive, area, state);
    state.runtime.hit_map = build_hit_map(&interactive, &layout);
}

pub fn install_info_hit_region(state: &mut AppState, index: usize, area: Rect) {
    install_hit_region(
        state,
        InteractiveId::framework(InteractiveId::INFO_BASE + index as u32),
        area,
    );
}

pub fn mouse_scroll_down(line: u32, column: u32) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        line,
        column,
        modifiers: Modifiers::empty(),
    })
}

pub fn mouse_scroll_up(line: u32, column: u32) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        line,
        column,
        modifiers: Modifiers::empty(),
    })
}

pub fn mouse_press_left(line: u32, column: u32) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line,
        column,
        modifiers: Modifiers::empty(),
    })
}

pub fn mouse_release_left(line: u32, column: u32) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Release(MouseButton::Left),
        line,
        column,
        modifiers: Modifiers::empty(),
    })
}

pub fn key_pageup() -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: Key::PageUp,
        modifiers: Modifiers::empty(),
    })
}

pub fn key_pagedown() -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: Key::PageDown,
        modifiers: Modifiers::empty(),
    })
}

pub fn key_ctrl_pageup() -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: Key::PageUp,
        modifiers: Modifiers::CTRL,
    })
}
