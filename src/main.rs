use clap::Parser;
use std::path::PathBuf;

use std::time::Instant;
use crate::websocket::WebSocketServer;

mod file_io;
mod message_types;
mod orderbook;
mod parser;
mod utils;
mod websocket;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the ITCH 5.0 data file
    #[arg(short, long)]
    file: PathBuf,

    /// Stock symbol to track
    #[arg(short, long)]
    symbol: String,

    /// Output file for the orderbook
    #[arg(short, long)]
    output_file: PathBuf,

    /// Enable WebSocket server
    #[arg(short, long, value_parser, default_value = "false")]
    websocket: bool,

    /// WebSocket server port
    #[arg(short = 'p', long, value_parser, default_value = "8473")]
    port: u16,
}

/*
samply record ./target/release/nasdaq-itch-orderbook \
-f /home/suhasghorp/Downloads/01302020.NASDAQ_ITCH50 \
-s INTC \
-o /home/suhasghorp/RustProjects/nasdaq-itch-orderbook/orderbooks/AAPL_orderbook.csv

conda activate base

python visualize.py ../orderbooks/AAPL_orderbook.csv
 */
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Parse command line arguments
    let args = Args::parse();

    // Convert stock symbol to fixed-length array expected by ITCH format
    let symbol = utils::pad_stock_symbol(&args.symbol);

    tracing::info!("Processing ITCH data for symbol: {}", args.symbol);


    // Memory map the input file
    let mapped_file = file_io::map_file(&args.file)?;
    tracing::info!("File mapped: {} bytes", mapped_file.len());

    // Create orderbook
    let mut order_book = orderbook::OrderBook::new(symbol, &args.output_file)?;
    tracing::info!("Created Limit Orderbook for symbol: {}", args.symbol);

    let start_time = Instant::now();
    // Process the file
    parser::process_itch_file(&mapped_file, &mut order_book)?;


    // Finalize and print statistics
    order_book.finalize()?;

    let duration = start_time.elapsed();
    let throughput = mapped_file.len() as f64 / (1024.0 * 1024.0) / duration.as_secs_f64();

    tracing::info!("Processing completed in {:.2?}", duration);
    tracing::info!("Throughput: {:.2} MB/s", throughput);

    // Start WebSocket server if enabled
    if args.websocket {
        println!("Starting WebSocket server on port {}", args.port);
        let server = WebSocketServer::new(&args.output_file.to_string_lossy(), args.port);
        server.start().await?;
    }

    Ok(())
}