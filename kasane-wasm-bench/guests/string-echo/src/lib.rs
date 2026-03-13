// String echo guest module.
// Host writes input string into BUFFER, guest echoes it back (or transforms it).

use core::cell::UnsafeCell;
use core::ptr::addr_of;

#[repr(C)]
struct Buffer(UnsafeCell<[u8; 8192]>);
unsafe impl Sync for Buffer {}

static BUFFER: Buffer = Buffer(UnsafeCell::new([0; 8192]));

#[unsafe(no_mangle)]
pub extern "C" fn get_buffer_ptr() -> i32 {
    addr_of!(BUFFER) as i32
}

/// Echo: just return the same length (data already in buffer).
#[unsafe(no_mangle)]
pub extern "C" fn echo(len: i32) -> i32 {
    len
}

/// Build a fixed-size string of N bytes in the buffer and return the length.
#[unsafe(no_mangle)]
pub extern "C" fn build_string(size: i32) -> i32 {
    let size = size as usize;
    let buf = BUFFER.0.get();
    unsafe {
        let slice = &mut *buf;
        for i in 0..size.min(slice.len()) {
            slice[i] = b'A' + (i % 26) as u8;
        }
    }
    size.min(8192) as i32
}
