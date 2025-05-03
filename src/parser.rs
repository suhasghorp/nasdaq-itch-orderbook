use crate::message_types::*;
use crate::orderbook::OrderBook;
use std::io;
use std::mem::size_of;
use std::ptr;
use std::time::Instant;

const MSG_HEADER_SIZE: usize = size_of::<MessageHeader>();


#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub fn stock_symbol_matches(a: &[u8; 8], b: &[u8; 8]) -> bool {
    a == b
}

// Prefetch the next message header
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn prefetch_next(ptr: *const u8, offset: usize) {
    use std::arch::x86_64::*;
    unsafe{_mm_prefetch::<_MM_HINT_T0>(ptr.add(offset) as *const i8)};
}

#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub unsafe fn prefetch_next(_ptr: *const u8, _offset: usize) {
    // No prefetch available on this architecture
}

// Safe function to read a u16 from unaligned memory
#[inline]
unsafe fn read_u16_be(ptr: *const u8) -> u16 {
    let mut val = [0u8; 2];
    unsafe{ptr::copy_nonoverlapping(ptr, val.as_mut_ptr(), 2)};
    u16::from_be_bytes(val)
}

// Safe function to read a 6-byte timestamp into u64
#[inline]
unsafe fn read_timestamp_be(ptr: *const u8) -> u64 {
    let mut val = [0u8; 8]; // Use 8 bytes (filling first 2 with zeros)
    // Copy the 6 bytes of the timestamp into the buffer (skipping the leading 2 zeros)
    unsafe{ptr::copy_nonoverlapping(ptr, val.as_mut_ptr().add(2), 6)};
    u64::from_be_bytes(val)
}


// Process the entire ITCH file
pub fn process_itch_file(data: &[u8], order_book: &mut OrderBook) -> io::Result<()> {
    let mut offset = 0;
    let data_len = data.len();
    let mut count:u128 = 0;
    let start_time = Instant::now();
    // Pre-calculate the prefetch distance - helps with cache efficiency
    let prefetch_distance = 16 * 4; // 4 cache lines ahead

    while offset + MSG_HEADER_SIZE <= data_len {
        // Prefetch the next message header
        if offset + prefetch_distance < data_len {
            unsafe{prefetch_next(data.as_ptr(), offset + prefetch_distance)};
        }

        // Read message header
        let msg_ptr = unsafe{data.as_ptr().add(offset)};
        let msg_length = unsafe{read_u16_be(msg_ptr)};
        let msg_type_byte = unsafe{*msg_ptr.add(2)};

        // Move past the header
        offset += MSG_HEADER_SIZE;

        // Check if we have the full message
        if offset + msg_length as usize > data_len {
            break;
        }

        let message_type = MessageType::from(msg_type_byte);
        let message_data = &data[offset..offset + msg_length as usize - 1]; // -1 for the type byte

        // Extract timestamp if needed
        let timestamp = match message_type {
            MessageType::AddOrder |
            MessageType::AddOrderWithMpid |
            MessageType::OrderExecuted |
            MessageType::OrderExecutedWithPrice |
            MessageType::OrderCancel |
            MessageType::OrderDelete |
            MessageType::OrderReplace |
            MessageType::Trade => {
                // All these messages have timestamp at the same offset (4 bytes in)
                if message_data.len() >= 10 { // Make sure we have enough data
                    unsafe{read_timestamp_be(message_data.as_ptr().add(4))}
                } else {
                    0
                }
            },
            _ => 0,
        };

        // Process message (with sampling if requested)
        if message_type != MessageType::Unknown {
            order_book.handle_message(message_type, message_data, timestamp)?;

        }
        count += 1;
        if count % 10_000_000 == 0 {
            let diff = start_time.elapsed().as_millis();
            tracing::info!("Processed {} Million messages, {} Million messages per second", count/1_000_000,count/diff/1000);
        }

        // Move to next message
        offset += msg_length as usize - 1; // -1 for the type byte already consumed
    }

    Ok(())
}