# NASDAQ ITCH 5.0 Parser and Websocket Orderbook Server and python client

A zero-copy, low-latency NASDAQ TotalView ITCH 5.0 parser written in Rust.

## Features

- **Complete Support**: Handles all 23 NASDAQ ITCH 5.0 message types
- **Zero-Copy Parsing**: Uses Unsafe for efficient binary parsing without unnecessary allocations
- **Memory-Mapped Files**: Fast access to large ITCH data files
- **Low Latency**: Optimized for high-performance market data processing
- **Websocket Simulation**: Replay historical data with realistic timing

## Installation

```bash
# Clone the repository
git clone https://github.com/suhasghorp/nasdaq-itch-orderbook.git
cd nasdaq-itch-orderbook

# Build in release mode
cargo build --release
```

## Usage

### Running the Orderbook Server

```bash
./target/release/nasdaq-itch-orderbook -f /path_to_unzipped_itch_file/01302020.NASDAQ_ITCH50 -s AAPL -o ./orderbooks/AAPL_orderbook.csv

```

Options:
- `-f, --file FILE`: Input ITCH 5.0 file (required)
- `-s, --symbol SYMBOL`: Stock symbol (required)
- `-o, --output OUTPUT`: Output orderbook file (required)

## Supported Message Types

| Type | Message Type | Description |
|------|--------------|-------------|
| S | SystemEvent | System events like market open/close |
| R | StockDirectory | Stock symbol definitions |
| H | StockTradingAction | Trading halts/resumes |
| Y | RegSHORestriction | Short sale restrictions |
| L | MarketParticipantPosition | Market maker positions |
| V | MwcbDeclineLevel | Market-wide circuit breaker levels |
| W | MwcbStatus | Market-wide circuit breaker status |
| K | IpoQuotingPeriodUpdate | IPO related information |
| J | LuldAuctionCollar | Limit Up-Limit Down auction info |
| h | OperationalHalt | Exchange operational halts |
| A | AddOrder | New order added to book |
| F | AddOrderWithMpid | New order with market participant ID |
| E | OrderExecuted | Order execution (partial/full) |
| C | OrderExecutedWithPrice | Execution with price different from order |
| X | OrderCancel | Partial order cancellation |
| D | OrderDelete | Order removal from book |
| U | OrderReplace | Order modification |
| P | Trade | Non-cross trade |
| Q | CrossTrade | Cross trade execution |
| B | BrokenTrade | Trade cancellation |
| I | Noii | Net Order Imbalance Indicator |
| N | RpiiMessage | Retail Price Improvement Indicator |
| O | DirectListingPriceDiscovery | Direct Listing with Capital Raise price discovery |

### python visualization of LOB

```python ./visualize.py```

![orderbook_1.png](orderbook_1.png)

![orderbook_2.png](orderbook_2.png)
## License

This project is licensed under the MIT License - see the LICENSE file for details.

## References

- [NASDAQ TotalView-ITCH 5.0 Specification](https://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHspecification.pdf)