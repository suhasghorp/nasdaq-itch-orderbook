// We're using mimalloc via the global allocator attribute in main.rs

// src/utils.rs
pub fn pad_stock_symbol(symbol: &str) -> [u8; 8] {
    let mut padded = [b' '; 8];
    let bytes = symbol.as_bytes();

    // Copy symbol bytes, limited to 8 characters
    let len = bytes.len().min(8);
    padded[..len].copy_from_slice(&bytes[..len]);

    padded
}