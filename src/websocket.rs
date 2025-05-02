use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::net::SocketAddr;
use std::path::Path;
use std::thread;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::select;
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message};

pub struct WebSocketServer {
    csv_path: String,
    port: u16,
}

impl WebSocketServer {
    pub fn new(csv_path: &str, port: u16) -> Self {
        WebSocketServer {
            csv_path: csv_path.to_string(),
            port,
        }
    }

    // Start the WebSocket server
    pub async fn start(&self) -> io::Result<()> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let listener = TcpListener::bind(&addr).await?;

        println!("WebSocket server started on: {}", addr);

        // Create a broadcast channel for distributing messages to all clients
        let (broadcast_tx, _) = broadcast::channel::<String>(1000);
        let csv_path = self.csv_path.clone();

        // Start CSV reading task
        let tx_clone = broadcast_tx.clone();
        self.start_csv_reader(csv_path, tx_clone);

        // Accept and handle client connections
        while let Ok((stream, addr)) = listener.accept().await {
            println!("New connection from: {}", addr);

            // Clone the broadcast sender for this client
            let rx = broadcast_tx.subscribe();

            // Spawn a new task to handle this client
            tokio::spawn(handle_connection(stream, addr, rx));
        }

        Ok(())
    }

    // Convert a CSV line with column names to a JSON object
    fn csv_line_to_json(header: &[String], line: &str) -> String {
        let values: Vec<&str> = line.split(',').collect();
        if values.len() != header.len() {
            return format!("{{\"error\": \"Column count mismatch: expected {}, got {}\"}}",
                           header.len(), values.len());
        }

        let mut json_str = String::from("{");

        for (i, (key, value)) in header.iter().zip(values.iter()).enumerate() {
            if i > 0 {
                json_str.push_str(", ");
            }

            // Handle numeric values (don't quote them in JSON)
            if i == 0 && key == "timestamp" {
                // Timestamp is a special case, it's numeric but we keep it as string
                json_str.push_str(&format!("\"{}\":\"{}\"", key, value));
            } else if key == "mid_price" {
                // Ensure mid_price is handled as numeric value
                match value.parse::<f64>() {
                    Ok(num) => json_str.push_str(&format!("\"{}\":{:.4}", key, num)),
                    Err(_) => json_str.push_str(&format!("\"{}\":0.0", key)),
                }
            } else if key == "orderbook_imbalance" {
                // Ensure imbalance is handled as numeric value with proper precision
                match value.parse::<f64>() {
                    Ok(num) => json_str.push_str(&format!("\"{}\":{:.6}", key, num)),
                    Err(_) => json_str.push_str(&format!("\"{}\":0.0", key)),
                }
            } else if value.parse::<f64>().is_ok() {
                // General numeric value, don't quote it
                json_str.push_str(&format!("\"{}\":{}", key, value));
            } else {
                // String value, quote it
                json_str.push_str(&format!("\"{}\":\"{}\"", key, value));
            }
        }

        json_str.push_str("}");
        json_str
    }

    // Start a thread to read the CSV file and broadcast updates
    fn start_csv_reader(&self, csv_path: String, tx: broadcast::Sender<String>) {
        thread::spawn(move || {
            // Wait for the CSV file to be created if it doesn't exist yet
            let mut retry_count = 0;
            while !Path::new(&csv_path).exists() {
                if retry_count > 30 {
                    eprintln!("Error: CSV file not found after 30 seconds: {}", csv_path);
                    return;
                }
                println!("Waiting for CSV file to be created: {}", csv_path);
                thread::sleep(Duration::from_secs(1));
                retry_count += 1;
            }

            // Open the CSV file for reading
            let file = match File::open(&csv_path) {
                Ok(file) => file,
                Err(e) => {
                    eprintln!("Error opening CSV file: {}", e);
                    return;
                }
            };

            println!("CSV file opened, starting broadcast: {}", csv_path);

            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            // Get the header line to extract column names
            let header = match lines.next().transpose() {
                Ok(Some(header_line)) => {
                    // Split the header line by commas to get column names
                    header_line.split(',').map(|s| s.trim().to_string()).collect::<Vec<String>>()
                },
                _ => {
                    eprintln!("Error reading CSV header or empty file");
                    return;
                }
            };

            println!("Parsed CSV header with {} columns", header.len());

            // Send a metadata message to clients with column information
            let metadata_json = format!("{{\"type\":\"metadata\",\"columns\":{}}}",
                                        serde_json::to_string(&header).unwrap_or_else(|_| "[]".to_string()));
            let _ = tx.send(metadata_json);

            // Read and broadcast each line as JSON
            let mut count = 0;
            for line in lines {
                match line {
                    Ok(data) => {
                        // Convert CSV line to JSON and broadcast
                        let json_data = Self::csv_line_to_json(&header, &data);
                        let _ = tx.send(json_data);
                        count += 1;

                        // Add a small delay to simulate realistic message flow
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => {
                        eprintln!("Error reading CSV line: {}", e);
                    }
                }
            }

            println!("Finished broadcasting {} JSON messages from CSV file", count);
        });
    }
}

// Handle a single WebSocket connection
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    mut rx: broadcast::Receiver<String>
) {
    // Accept the WebSocket connection
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("Error accepting WebSocket connection from {}: {}", addr, e);
            return;
        }
    };

    println!("WebSocket connection established with: {}", addr);

    // Split the WebSocket stream
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Main client handling loop
    loop {
        // Use select! to handle both broadcast messages and socket events
        select! {
            // Handle incoming broadcast messages (orderbook updates)
            data = rx.recv() => {
                match data {
                    Ok(msg) => {
                        // Send the JSON message to the WebSocket client
                        if ws_sender.send(Message::Text(msg)).await.is_err() {
                            // If sending fails, break out of the loop
                            break;
                        }
                    },
                    Err(e) => {
                        eprintln!("Broadcast channel error: {}", e);
                        break;
                    }
                }
            }

            // Handle incoming WebSocket messages (just for ping/pong)
            ws_msg = ws_receiver.next() => {
                match ws_msg {
                    Some(Ok(msg)) => {
                        // Only handle ping messages
                        if msg.is_ping() {
                            if ws_sender.send(Message::Pong(vec![])).await.is_err() {
                                break;
                            }
                        }
                        // Ignore all other messages from client
                    },
                    Some(Err(e)) => {
                        eprintln!("WebSocket error from {}: {}", addr, e);
                        break;
                    },
                    None => {
                        // WebSocket stream has ended
                        break;
                    }
                }
            }
        }
    }

    println!("Client disconnected: {}", addr);
}