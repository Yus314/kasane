// State reader guest module.
// Simulates a plugin that reads host state via imported functions.

use core::cell::UnsafeCell;
use core::ptr::addr_of;

unsafe extern "C" {
    safe fn host_get_cursor_line() -> i32;
    safe fn host_get_cursor_col() -> i32;
    safe fn host_get_line_count() -> i32;
    safe fn host_get_cols() -> i32;
    safe fn host_get_rows() -> i32;
    safe fn host_is_focused() -> i32;
}

struct SyncCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncCell<T> {}

static ACTIVE_LINE: SyncCell<i32> = SyncCell(UnsafeCell::new(-1));

#[repr(C)]
struct LineBuffer(UnsafeCell<[u8; 4096]>);
unsafe impl Sync for LineBuffer {}

// Output buffer for contribute_lines results.
// Each entry: 0 = None, 1 = Some(bg) followed by 3 bytes (r, g, b).
static LINE_BUFFER: LineBuffer = LineBuffer(UnsafeCell::new([0; 4096]));

#[unsafe(no_mangle)]
pub extern "C" fn get_line_buffer_ptr() -> i32 {
    addr_of!(LINE_BUFFER) as i32
}

fn active_line() -> &'static mut i32 {
    unsafe { &mut *ACTIVE_LINE.0.get() }
}

fn line_buf() -> &'static mut [u8; 4096] {
    unsafe { &mut *LINE_BUFFER.0.get() }
}

/// Simulate on_state_changed: read cursor position from host.
/// Returns 1 if state actually changed, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn on_state_changed(dirty_flags: i32) -> i32 {
    if dirty_flags & 0x01 != 0 {
        let line = host_get_cursor_line();
        let _col = host_get_cursor_col();
        let _count = host_get_line_count();
        let al = active_line();
        if *al != line {
            *al = line;
            return 1;
        }
    }
    0
}

/// Simulate on_state_changed with more host calls (6 total).
#[unsafe(no_mangle)]
pub extern "C" fn on_state_changed_heavy(dirty_flags: i32) -> i32 {
    if dirty_flags & 0x01 != 0 {
        let line = host_get_cursor_line();
        let _col = host_get_cursor_col();
        let _count = host_get_line_count();
        let _cols = host_get_cols();
        let _rows = host_get_rows();
        let _focused = host_is_focused();
        let al = active_line();
        if *al != line {
            *al = line;
            return 1;
        }
    }
    0
}

/// Simulate contribute_lines: write line decorations for visible range.
/// Returns the number of lines processed.
/// Output format in LINE_BUFFER: for each line, 1 byte (0=None, 1=Some) + if Some: 3 bytes (r,g,b).
#[unsafe(no_mangle)]
pub extern "C" fn contribute_lines(start: i32, end: i32) -> i32 {
    let active = *active_line();
    let buf = line_buf();
    let mut offset = 0;
    for line in start..end {
        if line == active {
            buf[offset] = 1; // Some
            offset += 1;
            buf[offset] = 40; // r
            offset += 1;
            buf[offset] = 40; // g
            offset += 1;
            buf[offset] = 50; // b
            offset += 1;
        } else {
            buf[offset] = 0; // None
            offset += 1;
        }
    }
    end - start
}
