#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    SystemEvent = b'S',
    StockDirectory = b'R',
    StockTradingAction = b'H',
    RegShoRestriction = b'Y',
    MarketParticipantPosition = b'L',
    MwcbDeclineLevel = b'V',
    MwcbStatus = b'W',
    IpoQuotingPeriodUpdate = b'K',
    LuldAuctionCollar = b'J',
    OperationalHalt = b'h',
    AddOrder = b'A',
    AddOrderWithMpid = b'F',
    OrderExecuted = b'E',
    OrderExecutedWithPrice = b'C',
    OrderCancel = b'X',
    OrderDelete = b'D',
    OrderReplace = b'U',
    Trade = b'P',
    CrossTrade = b'Q',
    BrokenTrade = b'B',
    Noii = b'I',
    Rpii = b'N',
    DirectListingWithCapitalRaisePriceDiscovery = b'O',
    Unknown = 0,
}

impl From<u8> for MessageType {
    fn from(byte: u8) -> Self {
        match byte {
            b'S' => MessageType::SystemEvent,
            b'R' => MessageType::StockDirectory,
            b'H' => MessageType::StockTradingAction,
            b'Y' => MessageType::RegShoRestriction,
            b'L' => MessageType::MarketParticipantPosition,
            b'V' => MessageType::MwcbDeclineLevel,
            b'W' => MessageType::MwcbStatus,
            b'K' => MessageType::IpoQuotingPeriodUpdate,
            b'J' => MessageType::LuldAuctionCollar,
            b'h' => MessageType::OperationalHalt,
            b'A' => MessageType::AddOrder,
            b'F' => MessageType::AddOrderWithMpid,
            b'E' => MessageType::OrderExecuted,
            b'C' => MessageType::OrderExecutedWithPrice,
            b'X' => MessageType::OrderCancel,
            b'D' => MessageType::OrderDelete,
            b'U' => MessageType::OrderReplace,
            b'P' => MessageType::Trade,
            b'Q' => MessageType::CrossTrade,
            b'B' => MessageType::BrokenTrade,
            b'I' => MessageType::Noii,
            b'N' => MessageType::Rpii,
            b'O' => MessageType::DirectListingWithCapitalRaisePriceDiscovery,
            _ => MessageType::Unknown,
        }
    }
}

// Message header (common to all messages)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct MessageHeader {
    pub length: u16,
    pub message_type: u8,
}

// System Event Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct SystemEventMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub event_code: u8,
}

// Stock Directory Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct StockDirectoryMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub market_category: u8,
    pub financial_status_indicator: u8,
    pub round_lot_size: u32,
    pub round_lots_only: u8,
    pub issue_classification: u8,
    pub issue_sub_type: [u8; 2],
    pub authenticity: u8,
    pub short_sale_threshold_indicator: u8,
    pub ipo_flag: u8,
    pub luld_reference_price_tier: u8,
    pub etp_flag: u8,
    pub etp_leverage_factor: u32,
    pub inverse_indicator: u8,
}

// Add Order Message (without MPID)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct AddOrderMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_ref_number: u64,
    pub buy_sell_indicator: u8,
    pub shares: u32,
    pub stock: [u8; 8],
    pub price: u32,
}

// Add Order with MPID Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct AddOrderWithMpidMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_ref_number: u64,
    pub buy_sell_indicator: u8,
    pub shares: u32,
    pub stock: [u8; 8],
    pub price: u32,
    pub attribution: [u8; 4],
}

// Order Executed Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct OrderExecutedMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_ref_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
}

// Order Executed With Price Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct OrderExecutedWithPriceMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_ref_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
    pub printable: u8,
    pub execution_price: u32,
}

// Order Cancel Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct OrderCancelMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_ref_number: u64,
    pub cancelled_shares: u32,
}

// Order Delete Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct OrderDeleteMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_ref_number: u64,
}

// Order Replace Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct OrderReplaceMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub original_order_ref_number: u64,
    pub new_order_ref_number: u64,
    pub shares: u32,
    pub price: u32,
}

// Trade Message
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TradeMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_ref_number: u64,
    pub buy_sell_indicator: u8,
    pub shares: u32,
    pub stock: [u8; 8],
    pub price: u32,
    pub match_number: u64,
}

// Stock Trading Action Message#[allow(dead_code)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct StockTradingActionMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub trading_state: u8,
    pub reserved: u8,
    pub reason: [u8; 4],
}