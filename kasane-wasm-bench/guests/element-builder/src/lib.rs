// Element builder guest module.
// Builds Element-like binary structures in a shared buffer.
//
// Binary format:
// - 0x01 Text: len(u16 BE) + utf8_bytes + face(6 bytes: fg_tag+data, bg_tag+data, attrs)
// - 0x02 Column: count(u16 BE) + children...
// - 0x03 Row: count(u16 BE) + children...
// - 0x04 Empty

use core::cell::UnsafeCell;
use core::ptr::addr_of;

#[repr(C)]
struct Buffer(UnsafeCell<[u8; 16384]>);
unsafe impl Sync for Buffer {}

static BUFFER: Buffer = Buffer(UnsafeCell::new([0; 16384]));
struct SyncOffset(UnsafeCell<usize>);
unsafe impl Sync for SyncOffset {}

static OFFSET: SyncOffset = SyncOffset(UnsafeCell::new(0));

fn buf() -> &'static mut [u8; 16384] {
    unsafe { &mut *BUFFER.0.get() }
}

fn offset() -> &'static mut usize {
    unsafe { &mut *OFFSET.0.get() }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_buffer_ptr() -> i32 {
    addr_of!(BUFFER) as i32
}

fn write_byte(b: u8) {
    let off = offset();
    buf()[*off] = b;
    *off += 1;
}

fn write_u16_be(v: u16) {
    write_byte((v >> 8) as u8);
    write_byte((v & 0xFF) as u8);
}

fn write_text(s: &[u8], fg_r: u8, fg_g: u8, fg_b: u8) {
    write_byte(0x01); // Tag: Text
    write_u16_be(s.len() as u16);
    for &b in s {
        write_byte(b);
    }
    // WireFace: fg=RGB(r,g,b), bg=default, attrs=0
    write_byte(0x02); // fg type: RGB
    write_byte(fg_r);
    write_byte(fg_g);
    write_byte(fg_b);
    write_byte(0x00); // bg type: default
    write_byte(0x00); // attrs=0
}

/// Build a single Text element. Returns byte length.
#[unsafe(no_mangle)]
pub extern "C" fn build_single_text() -> i32 {
    *offset() = 0;
    write_text(b"hello", 200, 200, 200);
    *offset() as i32
}

/// Build a gutter column with `line_count` right-aligned line numbers.
/// Returns byte length.
#[unsafe(no_mangle)]
pub extern "C" fn build_gutter(line_count: i32) -> i32 {
    *offset() = 0;
    write_byte(0x02); // Tag: Column
    write_u16_be(line_count as u16);
    for i in 1..=line_count {
        // Format line number as right-aligned 3-char string + space
        let mut num_buf = [b' '; 4];
        let mut n = i;
        let mut pos = 2usize;
        loop {
            num_buf[pos] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 || pos == 0 {
                break;
            }
            pos -= 1;
        }
        write_text(&num_buf, 0, 200, 200);
    }
    *offset() as i32
}

/// Build a nested structure: Row containing 3 Columns, each containing `depth` Text elements.
/// Returns byte length.
#[unsafe(no_mangle)]
pub extern "C" fn build_nested(depth: i32) -> i32 {
    *offset() = 0;
    write_byte(0x03); // Tag: Row
    write_u16_be(3); // 3 columns
    for col in 0..3u8 {
        write_byte(0x02); // Tag: Column
        write_u16_be(depth as u16);
        for row in 0..depth {
            let label = [b'C', b'0' + col, b'R', b'0' + (row as u8 / 10), b'0' + (row as u8 % 10)];
            write_text(&label, 100 + col * 50, 100, 100);
        }
    }
    *offset() as i32
}
