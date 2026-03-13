use wasmtime::*;

/// Create a default Engine for benchmarks.
pub fn bench_engine() -> Engine {
    Engine::default()
}

/// Create an Engine + Store + Instance from WAT source.
pub fn instantiate_wat(wat: &str) -> anyhow::Result<(Store<()>, Instance)> {
    let engine = Engine::default();
    let module = Module::new(&engine, wat)?;
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])?;
    Ok((store, instance))
}

/// Create an Engine + Store + Linker + Instance from WAT source with host imports.
pub fn instantiate_wat_with_linker<T: 'static>(
    wat: &str,
    host_state: T,
    setup_linker: impl FnOnce(&mut Linker<T>) -> anyhow::Result<()>,
) -> anyhow::Result<(Engine, Store<T>, Instance)> {
    let engine = Engine::default();
    let module = Module::new(&engine, wat)?;
    let mut linker = Linker::new(&engine);
    setup_linker(&mut linker)?;
    let mut store = Store::new(&engine, host_state);
    let instance = linker.instantiate(&mut store, &module)?;
    Ok((engine, store, instance))
}

/// Load a pre-built .wasm file from the fixtures directory.
pub fn load_wasm_fixture(name: &str) -> anyhow::Result<Vec<u8>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    Ok(std::fs::read(path)?)
}

// --- Element decoding for benchmarks ---

#[derive(Debug, Clone)]
pub enum BenchElement {
    Text(String, BenchFace),
    Column(Vec<BenchElement>),
    Row(Vec<BenchElement>),
    Empty,
}

#[derive(Debug, Clone, Copy)]
pub struct BenchFace {
    pub fg: BenchColor,
    pub bg: BenchColor,
    pub attrs: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum BenchColor {
    Default,
    Named(u8),
    Rgb(u8, u8, u8),
}

/// Decode a BenchElement from a binary buffer produced by a WASM guest.
///
/// Binary format:
/// - 0x01 Text: len(u16 BE) + utf8_bytes + face(7 bytes)
/// - 0x02 Column: count(u16 BE) + children...
/// - 0x03 Row: count(u16 BE) + children...
/// - 0x04 Empty
pub fn decode_element(data: &[u8], offset: &mut usize) -> BenchElement {
    let tag = data[*offset];
    *offset += 1;
    match tag {
        0x01 => {
            let len = u16::from_be_bytes([data[*offset], data[*offset + 1]]) as usize;
            *offset += 2;
            let text = String::from_utf8_lossy(&data[*offset..*offset + len]).into_owned();
            *offset += len;
            let face = decode_face(data, offset);
            BenchElement::Text(text, face)
        }
        0x02 => {
            let count = u16::from_be_bytes([data[*offset], data[*offset + 1]]) as usize;
            *offset += 2;
            let children = (0..count).map(|_| decode_element(data, offset)).collect();
            BenchElement::Column(children)
        }
        0x03 => {
            let count = u16::from_be_bytes([data[*offset], data[*offset + 1]]) as usize;
            *offset += 2;
            let children = (0..count).map(|_| decode_element(data, offset)).collect();
            BenchElement::Row(children)
        }
        0x04 => BenchElement::Empty,
        _ => panic!("unknown element tag: 0x{tag:02x} at offset {}", *offset - 1),
    }
}

fn decode_face(data: &[u8], offset: &mut usize) -> BenchFace {
    let fg = decode_color(data, offset);
    let bg = decode_color(data, offset);
    let attrs = data[*offset];
    *offset += 1;
    BenchFace { fg, bg, attrs }
}

fn decode_color(data: &[u8], offset: &mut usize) -> BenchColor {
    let tag = data[*offset];
    *offset += 1;
    match tag {
        0x00 => BenchColor::Default,
        0x01 => {
            let v = data[*offset];
            *offset += 1;
            BenchColor::Named(v)
        }
        0x02 => {
            let r = data[*offset];
            let g = data[*offset + 1];
            let b = data[*offset + 2];
            *offset += 3;
            BenchColor::Rgb(r, g, b)
        }
        _ => panic!("unknown color tag: 0x{tag:02x}"),
    }
}
