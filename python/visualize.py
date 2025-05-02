import pandas as pd
import matplotlib.pyplot as plt
from matplotlib.animation import FuncAnimation
import numpy as np
import matplotlib.gridspec as gridspec
from datetime import datetime
import pytz
import argparse
import asyncio
import websockets
import json
import signal
import sys
import multiprocessing as mp
from multiprocessing import Process, Queue, Event


# File and parameters
filename = '../orderbooks/AAPL_orderbook.csv'
num_levels = 10
est = pytz.timezone('US/Eastern')
skip_premarket = True

#9:30 AM in nanoseconds
MARKET_OPEN_NS = 34_170_000_000_000

# Generator function to stream CSV row by row using Polars

class WebSocketClient:
    def __init__(self, uri, data_queue, stop_event):
        self.uri = uri
        self.websocket = None
        self.connected = False
        self.data_queue = data_queue
        self.stop_event = stop_event

    async def connect(self):
        """Connect to the WebSocket server"""
        try:
            self.websocket = await websockets.connect(self.uri)
            self.connected = True
            print(f"Connected to WebSocket server at {self.uri}")
            return True
        except Exception as e:
            print(f"Error connecting to WebSocket server: {e}")
            self.connected = False
            return False

    async def disconnect(self):
        """Disconnect from the WebSocket server"""
        if self.websocket:
            await self.websocket.close()
            self.connected = False
            print("Disconnected from WebSocket server")

    async def receive_data(self):
        """Receive data from the WebSocket server"""
        if not self.connected:
            if not await self.connect():
                return None

        try:
            data = await self.websocket.recv()
            return data
        except websockets.exceptions.ConnectionClosed:
            print("Connection to WebSocket server closed")
            self.connected = False
            return None
        except Exception as e:
            print(f"Error receiving data: {e}")
            return None

    def process_json_data(self, json_str):
        """Process a JSON string into a structured format"""
        if not json_str:
            return None

        try:
            # Parse the JSON string
            data_dict = json.loads(json_str)

            # Convert timestamp to numeric if it's a string
            if 'timestamp' in data_dict and isinstance(data_dict['timestamp'], str):
                try:
                    data_dict['timestamp'] = int(data_dict['timestamp'])
                except ValueError:
                    pass

            # Convert numeric values
            for key, value in data_dict.items():
                if key != 'timestamp' and isinstance(value, str):
                    try:
                        data_dict[key] = float(value)
                    except ValueError:
                        pass

            return data_dict
        except json.JSONDecodeError as e:
            print(f"Error decoding JSON: {e}, data: {json_str[:100]}...")
            return None

    async def run(self, skip_premarket=True):
        """Run the WebSocket client and push data to the queue"""
        past_market_open = not skip_premarket

        while not self.stop_event.is_set():
            data = await self.receive_data()
            if data:
                # Process the data
                processed_data = self.process_json_data(data)
                if processed_data:
                    # Check if we should skip pre-market data
                    if 'timestamp' in processed_data:
                        ts = processed_data['timestamp']
                        if skip_premarket and not past_market_open:
                            if ts < MARKET_OPEN_NS:
                                continue
                            else:
                                past_market_open = True

                    # Add to queue for animation to consume
                    try:
                        self.data_queue.put(processed_data, block=False)
                    except:
                        # If queue is full, try to make room
                        try:
                            # Try to get an item first to make room (non-blocking)
                            self.data_queue.get(block=False)
                            # Now try to put our item
                            self.data_queue.put(processed_data, block=False)
                        except:
                            # If still fails, just move on
                            pass

            # Short sleep to prevent CPU spinning
            await asyncio.sleep(0.01)

        await self.disconnect()
        print("WebSocket client stopped")

def websocket_process_main(uri, data_queue, stop_event, skip_premarket):
    """Main function for the WebSocket process"""
    # Set up signal handlers in the child process
    def signal_handler(sig, frame):
        print("\nWebSocket process received signal to terminate")
        stop_event.set()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    # Create and run the WebSocket client
    client = WebSocketClient(uri, data_queue, stop_event)

    try:
        # Need to create a new event loop for the child process
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        loop.run_until_complete(client.run(skip_premarket))
    except Exception as e:
        print(f"Error in WebSocket process: {e}")
    finally:
        print("WebSocket process exiting")

# Extract bid/ask levels from row
def extract_orderbook_levels(row):
    bids = [(row[f'{i}_bid_price'], row[f'{i}_bid_vol']) for i in range(1, num_levels + 1)]
    asks = [(row[f'{i}_ask_price'], row[f'{i}_ask_vol']) for i in range(1, num_levels + 1)]
    return sorted(bids, key=lambda x: -x[0]), sorted(asks, key=lambda x: x[0])  # Bids descending, Asks ascending

def create_orderbook_animation(data_queue, stop_event):
    """Create and run the orderbook animation, reading data from the queue"""
    fig = plt.figure(figsize=(10, 8))
    gs = gridspec.GridSpec(2, 1, height_ratios=[3, 2])  # More space between plots
    ax_depth = fig.add_subplot(gs[0])
    ax_table = fig.add_subplot(gs[1])
    plt.subplots_adjust(hspace=0.6)  # Increase space between depth plot & table

    # Latest data from the queue
    latest_data = None

    def update(_):
        nonlocal latest_data

        # Check if stop event is set
        if stop_event.is_set():
            plt.close(fig)  # Close the figure when stop event is set
            return

        # Try to get new data from the queue (non-blocking)
        try:
            while not data_queue.empty():
                latest_data = data_queue.get(block=False)
        except:
            # If no new data, use previous data if available
            if latest_data is None:
                return

        # If we still don't have data, wait for it
        if latest_data is None:
            ax_depth.clear()
            ax_depth.set_title("AAPL - Order Book Depth (Waiting for data)")
            return

        row = latest_data
        ax_depth.clear()
        ax_table.clear()

        bids, asks = extract_orderbook_levels(row)

        # Handle empty orderbook
        if not bids or not asks:
            ax_depth.set_title("AAPL - Order Book Depth (Incomplete data)")
            return

        bid_prices, bid_vols = zip(*bids)
        ask_prices, ask_vols = zip(*asks)

        # Cumulative volume
        bid_cumvols = np.cumsum(bid_vols)
        ask_cumvols = np.cumsum(ask_vols)

        ax_depth.step(bid_prices, bid_cumvols, color='green', where='post', label='Bids')
        ax_depth.step(ask_prices, ask_cumvols, color='red', where='post', label='Asks')

        # Mid-price and vertical line
        if 'mid_price' in row:
            mid_price = row['mid_price']
        else:
            # Fall back to calculating it if not available
            mid_price = (ask_prices[0] + bid_prices[0]) / 2

        # Get orderbook imbalance from the data if available
        imbalance = row.get('orderbook_imbalance', 0.0)
        ax_depth.axvline(mid_price, color='black', linestyle='--', linewidth=1)

        ax_depth.set_title("AAPL - Order Book Depth")
        ax_depth.set_xlabel("Price (USD)")
        ax_depth.set_ylabel("Cumulative Volume")
        ax_depth.legend(loc='center left', bbox_to_anchor=(1.0, 0.5), facecolor='white', edgecolor='black')  # Legend outside

        # Flip asks so Ask 1 is just above Bid 1
        num_asks = len(asks)
        num_bids = len(bids)
        ask_table = list(reversed([[f"{p:.2f}", f"{v:.0f}"] for p, v in asks]))
        bid_table = [[f"{p:.2f}", f"{v:.0f}"] for p, v in bids]
        full_table = ask_table + [["", ""]] + bid_table  # Insert blank row for spacing

        col_labels = ["Price", "Volume"]

        # Display table without row labels
        ax_table.axis('off')
        table = ax_table.table(cellText=full_table, colLabels=col_labels, loc='center')

        # Adjust widths and remove borders
        for key, cell in table.get_celld().items():
            r, col = key
            if col in [0, 1]:  # Price and Volume columns
                cell.set_width(0.15)
                cell.set_linewidth(0)  # Remove border
            cell.get_text().set_horizontalalignment('center')  # Center text
            cell.get_text().set_verticalalignment('center')

        # Color-code rows
        for i in range(num_asks):  # Asks
            for j in range(2):
                table[(i+1, j)].get_text().set_color('red')
        for i in range(num_asks + 1, num_asks + num_bids + 1):  # Bids
            for j in range(2):
                table[(i+1, j)].get_text().set_color('green')

        # Insert mid-price into blank row (centered, bold, black)
        mid_str = f"Mid:{mid_price:.2f}"
        imb_str = f"Imb: {imbalance:.4f}"
        table[(num_asks + 1, 0)].get_text().set_text(mid_str)
        table[(num_asks + 1, 0)].get_text().set_color('black')
        table[(num_asks + 1, 0)].get_text().set_fontsize(16)
        table[(num_asks + 1, 0)].get_text().set_weight('bold')

        table[(num_asks + 1, 1)].get_text().set_text(imb_str)
        table[(num_asks + 1, 1)].get_text().set_color('black')
        table[(num_asks + 1, 1)].get_text().set_fontsize(16)
        table[(num_asks + 1, 1)].get_text().set_weight('bold')

        # Color imbalance text based on its value
        if imbalance > 0:
            # Positive imbalance (more bids than asks) - green
            table[(num_asks + 1, 1)].get_text().set_color('green')
        elif imbalance < 0:
            # Negative imbalance (more asks than bids) - red
            table[(num_asks + 1, 1)].get_text().set_color('red')

        # Time formatting
        if 'timestamp' in row:
            # Try to convert timestamp to datetime
            try:
                # Convert to integer if it's not already
                if not isinstance(row['timestamp'], int):
                    row['timestamp'] = int(float(row['timestamp']))

                # Convert to datetime
                dt = pd.to_datetime(row['timestamp'], unit='ns')
                time_str = dt.strftime('%H:%M:%S.%f')[:-3]  # Truncate microseconds to 3 digits
            except:
                time_str = str(row['timestamp'])
        else:
            time_str = datetime.now().strftime('%H:%M:%S.%f')[:-3]

        fig.suptitle(f"Time: {time_str}   |   Mid Price: {mid_price:.2f} USD   |   Imbalance: {imbalance:.4f}", fontsize=14)

    # Connect event handlers
    def on_close(event):
        stop_event.set()
        print("Visualization window closed, stopping WebSocket process...")

    fig.canvas.mpl_connect('close_event', on_close)

    # Create animation
    ani = FuncAnimation(fig, update, interval=200, repeat=True)
    plt.show()

def main():
    parser = argparse.ArgumentParser(description='Visualize ITCH orderbook data')
    parser.add_argument('--skip-premarket', action = 'store_true', help='Skip premarket data')
    parser.add_argument('--uri', '-u', type=str, default='ws://localhost:8473', help='WebSocket server URI')
    args = parser.parse_args()

    # Set the start method for multiprocessing
    if sys.platform == 'darwin':  # macOS
        mp.set_start_method('spawn')

    # Create shared objects for inter-process communication
    data_queue = Queue(maxsize=100)
    stop_event = Event()

    # Set up signal handlers in the main process
    def signal_handler(sig, frame):
        print("\nReceived signal to terminate. Stopping...")
        stop_event.set()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    # Start WebSocket client in a separate process
    ws_process = Process(
        target=websocket_process_main,
        args=(args.uri, data_queue, stop_event, args.skip_premarket),
        daemon=True
    )
    ws_process.start()
    print(f"WebSocket process started. Connecting to {args.uri}...")

    try:
        # Start animation in main process
        create_orderbook_animation(data_queue, stop_event)
    except KeyboardInterrupt:
        print("Interrupted by user")
    finally:
        # Clean up
        stop_event.set()
        if ws_process.is_alive():
            print("Waiting for WebSocket process to terminate...")
            ws_process.join(timeout=2.0)
            if ws_process.is_alive():
                print("WebSocket process still running, terminating...")
                ws_process.terminate()

        print("Application closed.")

if __name__ == "__main__":
    main()

