use crate::message_types::*;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const MAX_BOOK_DEPTH: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

impl From<u8> for Side {
    fn from(byte: u8) -> Self {
        match byte {
            b'B' => Side::Buy,
            _ => Side::Sell,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Order {
    pub ref_number: u64,
    pub timestamp: u64,
    pub price: u32,
    pub shares: u32,
    pub side: Side,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceLevel {
    pub price: u32,
    pub total_volume: u32,
}

pub struct OrderBook {
    symbol: [u8; 8],
    buy_orders: FxHashMap<u64, Order>,
    sell_orders: FxHashMap<u64, Order>,
    // Price to volume mapping for quick access - using BTreeMap to keep prices sorted
    buy_price_map: BTreeMap<u32, u32>,  // Descending price order for bids
    sell_price_map: BTreeMap<u32, u32>, // Ascending price order for asks
    writer: BufWriter<File>,
    // Track last known state for delta comparison
    last_state: Option<OrderbookState>,
    // Counters for statistics
    message_count: u64,
    update_count: u64,
    // Pre-allocate buffers for string operations
    line_buffer: String,
}

// Snapshot of orderbook state used for delta comparison
#[derive(Clone, PartialEq)]
struct OrderbookState {
    timestamp: u64,
    bid_levels: Vec<PriceLevel>,
    ask_levels: Vec<PriceLevel>,
    mid_price: f64,
    imbalance: f64,
}

#[inline]
unsafe fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    let bytes = [data[offset], data[offset + 1], data[offset + 2], data[offset + 3]];
    u32::from_be_bytes(bytes)
}


#[inline(always)]
fn read_order_ref_be(data: &[u8], offset: usize) -> u64 {
    let mut result = 0u64;
    for i in 0..8 {
        result <<= 8 ;
        result |= data[offset + i] as u64;
    }
    result
}

#[inline]
unsafe fn read_stock(data: &[u8], offset: usize) -> [u8; 8] {
    let mut stock = [0u8; 8];
    stock.copy_from_slice(&data[offset..offset + 8]);
    stock
}

#[inline]
// Fix the calculate_imbalance method to use buy_price_map and sell_price_map
fn calculate_imbalance(bids : &[PriceLevel], asks : &[PriceLevel]) -> f64 {
    let total_bid_volume: u32 = bids.iter().map(|price_level| price_level.total_volume).sum();
    let total_ask_volume: u32 = asks.iter().map(|price_level| price_level.total_volume).sum();

    if total_bid_volume == 0 && total_ask_volume == 0 {
        return 0.0;
    }

    let total_volume = total_bid_volume as f64 + total_ask_volume as f64;
    (total_bid_volume as f64 - total_ask_volume as f64) / total_volume
}

impl OrderBook {
    pub fn new(symbol: [u8; 8], output_path: &Path) -> Result<Self, std::io::Error> {
        let file = File::create(output_path)?;
        let mut writer = BufWriter::new(file);

        // Write CSV header
        let mut header = String::from("timestamp");
        for level in 1..=MAX_BOOK_DEPTH {
            header.push_str(&format!(",{}_bid_price,{}_bid_vol,{}_ask_price,{}_ask_vol",
                                     level, level, level, level));
        }
        header.push_str(",mid_price,orderbook_imbalance");
        header.push('\n');
        writer.write_all(header.as_bytes())?;

        Ok(OrderBook {
            symbol,
            buy_orders: FxHashMap::default(),
            sell_orders: FxHashMap::default(),
            buy_price_map: BTreeMap::new(),
            sell_price_map: BTreeMap::new(),
            writer,
            last_state: None,
            message_count: 0,
            update_count: 0,
            line_buffer: String::new(),
        })
    }


    pub fn handle_message(&mut self, message_type: MessageType, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        self.message_count+=1;
        unsafe {
            match message_type {
                MessageType::AddOrder => self.handle_add_order(data, timestamp),
                MessageType::AddOrderWithMpid => self.handle_add_order_with_mpid(data, timestamp),
                MessageType::OrderExecuted => self.handle_order_executed(data, timestamp),
                MessageType::OrderExecutedWithPrice => self.handle_order_executed_with_price(data, timestamp),
                MessageType::OrderCancel => self.handle_order_cancel(data, timestamp),
                MessageType::OrderDelete => self.handle_order_delete(data, timestamp),
                MessageType::OrderReplace => self.handle_order_replace(data, timestamp),
                MessageType::Trade => self.handle_trade(data),
                _ => Ok(()),
            }
        }
    }

    pub fn handle_add_order(&mut self, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        // Directly read fields from data slice - offsets based on struct layout
        // ITCH 5.0 field layout for Add Order:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - order_ref_number (8 bytes) -> offset 10
        // - buy_sell_indicator (1 byte) -> offset 18
        // - shares (4 bytes) -> offset 19
        // - stock (8 bytes) -> offset 23
        // - price (4 bytes) -> offset 31

        // Check if the message is for our symbol before doing more work
        let stock = unsafe {read_stock(data, 23)};

        // Check if the message is for our symbol
        if stock != self.symbol {
            return Ok(());
        }

        let order_ref_number = read_order_ref_be(data, 10);
        let buy_sell_indicator = data[18];
        let shares = unsafe{read_u32_be(data, 19)};
        let price = unsafe{read_u32_be(data, 31)};

        let side = Side::from(buy_sell_indicator);
        let order = Order {
            ref_number: order_ref_number,
            timestamp,
            price,
            shares,
            side,
        };

        self.add_order(order)?;

        Ok(())
    }

    unsafe fn handle_add_order_with_mpid(&mut self, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        // Similar to handle_add_order but with MPID field
        // ITCH 5.0 field layout for Add Order with MPID:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - order_ref_number (8 bytes) -> offset 10
        // - buy_sell_indicator (1 byte) -> offset 18
        // - shares (4 bytes) -> offset 19
        // - stock (8 bytes) -> offset 23
        // - price (4 bytes) -> offset 31
        // - attribution (4 bytes) -> offset 35

        let stock = unsafe{read_stock(data, 23)};

        // Check if the message is for our symbol
        if stock != self.symbol {
            return Ok(());
        }

        let order_ref_number = read_order_ref_be(data, 10);
        let buy_sell_indicator = data[18];
        let shares = unsafe{read_u32_be(data, 19)};
        let price = unsafe{read_u32_be(data, 31)};

        let side = Side::from(buy_sell_indicator);
        let order = Order {
            ref_number: order_ref_number,
            timestamp,
            price,
            shares,
            side,
        };

        self.add_order(order)?;

        Ok(())
    }

    unsafe fn handle_order_executed(&mut self, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        // ITCH 5.0 field layout for Order Executed:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - order_ref_number (8 bytes) -> offset 10
        // - executed_shares (4 bytes) -> offset 18
        // - match_number (8 bytes) -> offset 22

        let order_ref_number = read_order_ref_be(data, 10);
        let executed_shares = unsafe{read_u32_be(data, 18)};

        // Look up the order
        if let Some(order) = self.buy_orders.get_mut(&order_ref_number) {
            // Reduce the shares
            order.shares = order.shares.saturating_sub(executed_shares);

            // Update the price map
            if let Some(volume) = self.buy_price_map.get_mut(&order.price) {
                *volume = volume.saturating_sub(executed_shares);
                if *volume == 0 {
                    self.buy_price_map.remove(&order.price);
                }
            }

            // Remove the order if no shares left
            if order.shares == 0 {
                self.buy_orders.remove(&order_ref_number);
            }

            // Write updated orderbook state
            self.write_orderbook(timestamp)?;
        } else if let Some(order) = self.sell_orders.get_mut(&order_ref_number) {
            // Reduce the shares
            order.shares = order.shares.saturating_sub(executed_shares);

            // Update the price map
            if let Some(volume) = self.sell_price_map.get_mut(&order.price) {
                *volume = volume.saturating_sub(executed_shares);
                if *volume == 0 {
                    self.sell_price_map.remove(&order.price);
                }
            }

            // Remove the order if no shares left
            if order.shares == 0 {
                self.sell_orders.remove(&order_ref_number);
            }

            // Write updated orderbook state
            self.write_orderbook(timestamp)?;
        }

        Ok(())
    }

    unsafe fn handle_order_executed_with_price(&mut self, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        // ITCH 5.0 field layout for Order Executed With Price:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - order_ref_number (8 bytes) -> offset 10
        // - executed_shares (4 bytes) -> offset 18
        // - match_number (8 bytes) -> offset 22
        // - printable (1 byte) -> offset 30
        // - execution_price (4 bytes) -> offset 31

        let order_ref_number = read_order_ref_be(data, 10);
        let executed_shares = unsafe{read_u32_be(data, 18)};

        // Similar to handle_order_executed but with price override
        if let Some(order) = self.buy_orders.get_mut(&order_ref_number) {
            // Reduce the shares
            order.shares = order.shares.saturating_sub(executed_shares);

            // Update the price map
            if let Some(volume) = self.buy_price_map.get_mut(&order.price) {
                *volume = volume.saturating_sub(executed_shares);
                if *volume == 0 {
                    self.buy_price_map.remove(&order.price);
                }
            }

            // Remove the order if no shares left
            if order.shares == 0 {
                self.buy_orders.remove(&order_ref_number);
            }

            // Write updated orderbook state
            self.write_orderbook(timestamp)?;
        } else if let Some(order) = self.sell_orders.get_mut(&order_ref_number) {
            // Reduce the shares
            order.shares = order.shares.saturating_sub(executed_shares);

            // Update the price map
            if let Some(volume) = self.sell_price_map.get_mut(&order.price) {
                *volume = volume.saturating_sub(executed_shares);
                if *volume == 0 {
                    self.sell_price_map.remove(&order.price);
                }
            }

            // Remove the order if no shares left
            if order.shares == 0 {
                self.sell_orders.remove(&order_ref_number);
            }

            // Write updated orderbook state
            self.write_orderbook(timestamp)?;
        }

        Ok(())
    }

    unsafe fn handle_order_cancel(&mut self, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        // ITCH 5.0 field layout for Order Cancel:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - order_ref_number (8 bytes) -> offset 10
        // - cancelled_shares (4 bytes) -> offset 18

        let order_ref_number = read_order_ref_be(data, 10);
        let cancelled_shares = unsafe{read_u32_be(data, 18)};

        // Look up the order
        if let Some(order) = self.buy_orders.get_mut(&order_ref_number) {
            // Reduce the shares
            order.shares = order.shares.saturating_sub(cancelled_shares);

            // Update the price map
            if let Some(volume) = self.buy_price_map.get_mut(&order.price) {
                *volume = volume.saturating_sub(cancelled_shares);
                if *volume == 0 {
                    self.buy_price_map.remove(&order.price);
                }
            }

            // Remove the order if no shares left
            if order.shares == 0 {
                self.buy_orders.remove(&order_ref_number);
            }

            // Write updated orderbook state
            self.write_orderbook(timestamp)?;
        } else if let Some(order) = self.sell_orders.get_mut(&order_ref_number) {
            // Reduce the shares
            order.shares = order.shares.saturating_sub(cancelled_shares);

            // Update the price map
            if let Some(volume) = self.sell_price_map.get_mut(&order.price) {
                *volume = volume.saturating_sub(cancelled_shares);
                if *volume == 0 {
                    self.sell_price_map.remove(&order.price);
                }
            }

            // Remove the order if no shares left
            if order.shares == 0 {
                self.sell_orders.remove(&order_ref_number);
            }

            // Write updated orderbook state
            self.write_orderbook(timestamp)?;
        }

        Ok(())
    }


    unsafe fn handle_order_delete(&mut self, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        // ITCH 5.0 field layout for Order Delete:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - order_ref_number (8 bytes) -> offset 10

        let order_ref_number = read_order_ref_be(data, 10);

        // Use peek_entry and take to avoid double hash computation
        let mut price = 0;
        let mut shares = 0;
        let mut side = Side::Buy;
        let mut exists = false;

        // Check buy orders first as they're typically more frequent
        if let Some(entry) = self.buy_orders.get(&order_ref_number) {
            price = entry.price;
            shares = entry.shares;
            side = Side::Buy;
            exists = true;
        } else if let Some(entry) = self.sell_orders.get(&order_ref_number) {
            price = entry.price;
            shares = entry.shares;
            side = Side::Sell;
            exists = true;
        }

        if exists {
            // Now perform the actual removal with the cached order info
            match side {
                Side::Buy => {
                    self.buy_orders.remove(&order_ref_number);
                    if let Some(volume) = self.buy_price_map.get_mut(&price) {
                        *volume = volume.saturating_sub(shares);
                        if *volume == 0 {
                            self.buy_price_map.remove(&price);
                        }
                    }
                },
                Side::Sell => {
                    self.sell_orders.remove(&order_ref_number);
                    if let Some(volume) = self.sell_price_map.get_mut(&price) {
                        *volume = volume.saturating_sub(shares);
                        if *volume == 0 {
                            self.sell_price_map.remove(&price);
                        }
                    }
                }
            }

            // Write updated orderbook state
            self.write_orderbook(timestamp)?;
        }

        Ok(())
    }


    unsafe fn handle_order_replace(&mut self, data: &[u8], timestamp: u64) -> Result<(), std::io::Error> {
        // ITCH 5.0 field layout for Order Replace:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - original_order_ref_number (8 bytes) -> offset 10
        // - new_order_ref_number (8 bytes) -> offset 18
        // - shares (4 bytes) -> offset 26
        // - price (4 bytes) -> offset 30

        let original_order_ref_number = read_order_ref_be(data, 10);
        let new_order_ref_number = read_order_ref_be(data, 18);
        let new_shares = unsafe{read_u32_be(data, 26)};
        let new_price = unsafe{read_u32_be(data, 30)};

        // Two-phase approach: first check, then remove
        let mut side = Side::Buy;
        let mut old_price = 0;
        let mut old_shares = 0;
        let mut found = false;

        // Check buy orders first
        if let Some(order) = self.buy_orders.get(&original_order_ref_number) {
            side = order.side;
            old_price = order.price;
            old_shares = order.shares;
            found = true;
        } else if let Some(order) = self.sell_orders.get(&original_order_ref_number) {
            side = order.side;
            old_price = order.price;
            old_shares = order.shares;
            found = true;
        }

        if found {
            // Now actually remove the old order
            match side {
                Side::Buy => {
                    self.buy_orders.remove(&original_order_ref_number);
                    if let Some(volume) = self.buy_price_map.get_mut(&old_price) {
                        *volume = volume.saturating_sub(old_shares);
                        if *volume == 0 {
                            self.buy_price_map.remove(&old_price);
                        }
                    }
                },
                Side::Sell => {
                    self.sell_orders.remove(&original_order_ref_number);
                    if let Some(volume) = self.sell_price_map.get_mut(&old_price) {
                        *volume = volume.saturating_sub(old_shares);
                        if *volume == 0 {
                            self.sell_price_map.remove(&old_price);
                        }
                    }
                }
            }

            // Add the new order
            let new_order = Order {
                ref_number: new_order_ref_number,
                timestamp,
                price: new_price,
                shares: new_shares,
                side,
            };

            self.add_order(new_order)?;
        }

        Ok(())
    }

    pub fn handle_trade(&mut self, data: &[u8]) -> Result<(), std::io::Error> {
        // ITCH 5.0 field layout for Trade:
        // - stock_locate (2 bytes)
        // - tracking_number (2 bytes)
        // - timestamp (6 bytes)
        // - order_ref_number (8 bytes) -> offset 10
        // - buy_sell_indicator (1 byte) -> offset 18
        // - shares (4 bytes) -> offset 19
        // - stock (8 bytes) -> offset 23
        // - price (4 bytes) -> offset 31
        // - match_number (8 bytes) -> offset 35

        let stock = unsafe{read_stock(data, 23)};

        // Check if the message is for our symbol
        if stock != self.symbol {
            return Ok(());
        }

        // Trades don't directly affect the orderbook unless they're executed against an order
        // This is already handled by the order executed messages

        Ok(())
    }

    fn add_order(&mut self, order: Order) -> Result<(), std::io::Error> {
        let ts = order.timestamp;
        // Update the price map
        if order.side == Side::Buy {
            *self.buy_price_map.entry(order.price).or_insert(0) += order.shares;
            self.buy_orders.insert(order.ref_number, order);
        } else {
            *self.sell_price_map.entry(order.price).or_insert(0) += order.shares;
            self.sell_orders.insert(order.ref_number, order);
        }

        // Write updated orderbook state
        self.write_orderbook(ts)?;

        Ok(())
    }

    #[inline]
    fn price_to_decimal_fast(&self, price: u32) -> (u32, u32) {
        // Returns the integer part and 4 decimal places
        let integer = price / 10000;
        let decimal = price % 10000;
        (integer, decimal)
    }

    fn write_orderbook(&mut self, timestamp: u64) -> Result<(), std::io::Error> {
        // Get the top levels for bids and asks
        let bids = self.get_top_bids(MAX_BOOK_DEPTH);
        let asks = self.get_top_asks(MAX_BOOK_DEPTH);

        //let mid_price = self.calculate_mid_price();
        let mid_price = (bids.get(0).map_or(0, |p| p.price) as f64 +
            asks.get(0).map_or(0, |p| p.price) as f64) / 20000.0;
        //println!("old mid price: {}, new mid price: {}", mid_price, mid_price_new);
        let imbalance = calculate_imbalance(&bids, &asks);

        // Create a new state to check for changes
        let new_state = OrderbookState {
            timestamp,
            bid_levels: bids.clone(),
            ask_levels: asks.clone(),
            mid_price,      // Initialize with calculated mid price
            imbalance,      // Initialize with calculated imbalance
        };

        // Check if the orderbook state has actually changed (other than timestamp)
        if let Some(ref last_state) = self.last_state {
            let same_bids = last_state.bid_levels == new_state.bid_levels;
            let same_asks = last_state.ask_levels == new_state.ask_levels;

            if same_bids && same_asks {
                // No meaningful change, skip writing
                //return Ok(());
            }
        }

        // Increment update counter
        self.update_count += 1;

        // Update the last known state
        self.last_state = Some(new_state);

        // Clear the existing buffer
        self.line_buffer.clear();

        // Start with timestamp
        self.line_buffer.push_str(&timestamp.to_string());

        // Add padded bids and asks
        let padded_bids = self.pad_levels(bids, MAX_BOOK_DEPTH);
        let padded_asks = self.pad_levels(asks, MAX_BOOK_DEPTH);

        use std::io::Write;

        // Write timestamp directly
        write!(self.writer, "{}", timestamp)?;

        // Use a specialized approach for price decimal formatting
        // that avoids floating-point operations entirely
        for i in 0..MAX_BOOK_DEPTH {
            // Get integer and decimal parts for prices
            let (bid_int, bid_dec) = self.price_to_decimal_fast(padded_bids[i].price);
            let (ask_int, ask_dec) = self.price_to_decimal_fast(padded_asks[i].price);

            // Write formatted prices with proper decimal padding
            write!(self.writer, ",{}.{:04},{},{}.{:04},{}",
                   bid_int, bid_dec,
                   padded_bids[i].total_volume,
                   ask_int, ask_dec,
                   padded_asks[i].total_volume)?;
        }

        write!(self.writer, ",{:.4},{:.6}", mid_price, imbalance)?;

        // End the line
        self.writer.write_all(b"\n")?;

        // Only flush periodically to reduce I/O overhead
        if self.update_count % 100 == 0 {
            self.writer.flush()?;
        }

        Ok(())
    }

    // Ensure we have exactly 'count' levels by padding with zeros if needed
    fn pad_levels(&self, mut levels: Vec<PriceLevel>, count: usize) -> Vec<PriceLevel> {
        while levels.len() < count {
            levels.push(PriceLevel { price: 0, total_volume: 0 });
        }
        levels
    }

    fn get_top_bids(&self, count: usize) -> Vec<PriceLevel> {
        // Get keys in reverse order (highest to lowest) for bids
        self.buy_price_map.iter()
            .rev() // Reverse to get highest prices first
            .take(count)
            .map(|(&price, &volume)| PriceLevel {
                price,
                total_volume: volume,
            })
            .collect()
    }

    fn get_top_asks(&self, count: usize) -> Vec<PriceLevel> {
        // BTreeMap already gives us keys in ascending order (lowest to highest) for asks
        self.sell_price_map.iter()
            .take(count)
            .map(|(&price, &volume)| PriceLevel {
                price,
                total_volume: volume,
            })
            .collect()
    }

    pub fn finalize(&mut self) -> Result<(), std::io::Error> {
        // Ensure all data is flushed to disk
        self.writer.flush()?;

        // Print statistics
        println!("Processed {} messages", self.message_count);
        println!("Wrote {} orderbook updates", self.update_count);

        Ok(())
    }
}