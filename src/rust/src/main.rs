use std::env;
use std::fmt::Write as FmtWrite;
use std::sync::Arc;

use chrono::Utc;
use log::{debug, error, info, warn};
use socketcan::{CanFrame, CanSocket, EmbeddedFrame, Id, Socket};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, RwLock};

// ---------------------------------------------------------------------------
// Yacht Devices RAW Formatting
// ---------------------------------------------------------------------------

/// Return UTC timestamp as hh:mm:ss.sss (per YD RAW format).
fn utc_timestamp() -> String {
    let now = Utc::now();
    now.format("%H:%M:%S%.3f").to_string()
}

/// Format a CAN frame as a RAW line with the given direction tag.
fn encode_raw_line(can_id: u32, data: &[u8], direction: char) -> Vec<u8> {
    let ts = utc_timestamp();
    let mut line = format!("{} {} {:08X}", ts, direction, can_id);
    for b in data {
        write!(line, " {:02X}", b).unwrap();
    }
    line.push_str("\r\n");
    line.into_bytes()
}

/// Parse a RAW text line into (can_id, data).
/// Accepts: `[timestamp] [R|T] <CANID> <DATA...>`
fn parse_raw_line(line: &str) -> Option<(u32, Vec<u8>)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let mut idx = 0;

    // Skip timestamp if present (HH:MM:SS.mmm format)
    if parts[idx].len() >= 11 {
        let bytes = parts[idx].as_bytes();
        if bytes.len() >= 9 && bytes[2] == b':' && bytes[5] == b':' && bytes[8] == b'.' {
            idx += 1;
        }
    }

    if idx >= parts.len() {
        return None;
    }

    // Skip direction if present
    if parts[idx] == "R" || parts[idx] == "T" {
        idx += 1;
    }

    if idx >= parts.len() {
        return None;
    }

    // CAN ID
    let can_id = u32::from_str_radix(parts[idx], 16).ok()?;
    idx += 1;

    // Data bytes
    let mut data = Vec::with_capacity(8);
    for &part in &parts[idx..] {
        data.push(u8::from_str_radix(part, 16).ok()?);
    }

    Some((can_id, data))
}

// ---------------------------------------------------------------------------
// Client tracking
// ---------------------------------------------------------------------------

/// Unique identifier for each connected TCP client.
type ClientId = u64;

/// Message broadcast to TCP clients: (raw_line, source_client_id or 0 for CAN).
#[derive(Clone)]
struct BroadcastMsg {
    data: Arc<Vec<u8>>,
    source: ClientId,
}

// ---------------------------------------------------------------------------
// CAN bus I/O (blocking, run on dedicated threads)
// ---------------------------------------------------------------------------

/// Read CAN frames in a blocking thread, forward to async broadcast channel.
fn can_reader_thread(socket: Arc<CanSocket>, tx: mpsc::Sender<(u32, Vec<u8>)>) {
    info!("CAN reader thread started");
    loop {
        match socket.read_frame() {
            Ok(frame) => {
                let can_id = match frame.id() {
                    Id::Standard(id) => id.as_raw() as u32,
                    Id::Extended(id) => id.as_raw(),
                };
                let data = frame.data().to_vec();
                if tx.blocking_send((can_id, data)).is_err() {
                    error!("CAN reader: broadcast channel closed");
                    break;
                }
            }
            Err(e) => {
                error!("CAN read error: {}", e);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

/// Write CAN frames from an async channel to the bus (blocking thread).
fn can_writer_thread(socket: Arc<CanSocket>, mut rx: mpsc::Receiver<(u32, Vec<u8>)>) {
    info!("CAN writer thread started");
    while let Some((can_id, data)) = rx.blocking_recv() {
        // Build a CAN frame — use extended ID (29-bit) as per NMEA 2000
        let frame = CanFrame::new(
            socketcan::ExtendedId::new(can_id).unwrap_or_else(|| {
                warn!("Invalid extended CAN ID: {:08X}, masking", can_id);
                socketcan::ExtendedId::new(can_id & 0x1FFFFFFF).unwrap()
            }),
            &data,
        );
        match frame {
            Some(f) => {
                if let Err(e) = socket.write_frame(&f) {
                    error!("CAN write error: {}", e);
                }
            }
            None => {
                warn!("Failed to construct CAN frame for ID {:08X}", can_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TCP ↔ CAN RAW Gateway
// ---------------------------------------------------------------------------

struct Gateway {
    can_interface: String,
    host: String,
    port: u16,
    next_client_id: Arc<RwLock<ClientId>>,
}

impl Gateway {
    fn new() -> Self {
        let can_interface = env::var("CAN_INTERFACE").unwrap_or_else(|_| "can0".to_string());
        let host = env::var("LISTEN_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = env::var("LISTEN_PORT")
            .unwrap_or_else(|_| "2598".to_string())
            .parse()
            .unwrap_or(2598);

        Gateway {
            can_interface,
            host,
            port,
            next_client_id: Arc::new(RwLock::new(1)),
        }
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Starting CAN RAW gateway on {} — TCP {}:{}",
            self.can_interface, self.host, self.port
        );

        // Open SocketCAN
        let socket = Arc::new(CanSocket::open(&self.can_interface)?);
        info!("CAN socket opened on {}", self.can_interface);

        // Channels
        let (can_rx_tx, mut can_rx_rx) = mpsc::channel::<(u32, Vec<u8>)>(4096);
        let (can_tx_tx, can_tx_rx) = mpsc::channel::<(u32, Vec<u8>)>(4096);
        let (broadcast_tx, _) = broadcast::channel::<BroadcastMsg>(4096);

        // Spawn blocking CAN I/O threads
        let reader_socket = Arc::clone(&socket);
        std::thread::Builder::new()
            .name("can-reader".to_string())
            .spawn(move || can_reader_thread(reader_socket, can_rx_tx))?;

        let writer_socket = Arc::clone(&socket);
        std::thread::Builder::new()
            .name("can-writer".to_string())
            .spawn(move || can_writer_thread(writer_socket, can_tx_rx))?;

        // CAN → broadcast task: read frames from CAN, format as RAW, broadcast
        let bcast_tx = broadcast_tx.clone();
        tokio::spawn(async move {
            while let Some((can_id, data)) = can_rx_rx.recv().await {
                let raw = encode_raw_line(can_id, &data, 'R');
                let msg = BroadcastMsg {
                    data: Arc::new(raw),
                    source: 0, // from CAN bus
                };
                // Ignore errors (no subscribers)
                let _ = bcast_tx.send(msg);
            }
        });

        // TCP server
        let bind_addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&bind_addr).await?;
        info!("TCP server listening on {}", bind_addr);

        loop {
            let (stream, peer) = listener.accept().await?;
            info!("Client connected: {}", peer);

            let client_id = {
                let mut id = self.next_client_id.write().await;
                let cid = *id;
                *id += 1;
                cid
            };

            let bcast_rx = broadcast_tx.subscribe();
            let bcast_tx = broadcast_tx.clone();
            let can_writer = can_tx_tx.clone();

            tokio::spawn(async move {
                Self::handle_client(stream, peer, client_id, bcast_rx, bcast_tx, can_writer).await;
                info!("Client disconnected: {}", peer);
            });
        }
    }

    async fn handle_client(
        stream: tokio::net::TcpStream,
        peer: std::net::SocketAddr,
        client_id: ClientId,
        mut bcast_rx: broadcast::Receiver<BroadcastMsg>,
        bcast_tx: broadcast::Sender<BroadcastMsg>,
        can_writer: mpsc::Sender<(u32, Vec<u8>)>,
    ) {
        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);

        // Per-client write channel so both broadcast and direct echo can send
        let (client_tx, mut client_rx) = mpsc::channel::<Vec<u8>>(1024);

        // Task: merge broadcast + direct writes → TCP writer
        let client_tx_for_bcast = client_tx.clone();
        let bcast_handle = tokio::spawn(async move {
            loop {
                match bcast_rx.recv().await {
                    Ok(msg) => {
                        // Don't echo broadcast back to source (they get a direct T echo)
                        if msg.source == client_id {
                            continue;
                        }
                        if client_tx_for_bcast.send(msg.data.to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Client {} lagged, dropped {} messages", peer, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // Task: drain per-client channel → TCP socket
        let write_handle = tokio::spawn(async move {
            while let Some(data) = client_rx.recv().await {
                if writer.write_all(&data).await.is_err() {
                    break;
                }
                if writer.flush().await.is_err() {
                    break;
                }
            }
        });

        // Read lines from client → parse → send to CAN bus → echo T → broadcast T
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            match buf_reader.read_line(&mut line_buf).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let text = line_buf.trim();
                    if text.is_empty() {
                        continue;
                    }
                    debug!("Client {} sent: {}", peer, text);

                    if let Some((can_id, data)) = parse_raw_line(text) {
                        // Send to CAN bus
                        if can_writer.send((can_id, data.clone())).await.is_err() {
                            error!("CAN writer channel closed");
                            break;
                        }

                        // Build T (transmit) echo line
                        let echo = encode_raw_line(can_id, &data, 'T');

                        // Echo T back to the sending client
                        if client_tx.send(echo.clone()).await.is_err() {
                            break;
                        }

                        // Broadcast T to other clients
                        let msg = BroadcastMsg {
                            data: Arc::new(echo),
                            source: client_id,
                        };
                        let _ = bcast_tx.send(msg);
                    }
                }
                Err(e) => {
                    error!("Client {} read error: {}", peer, e);
                    break;
                }
            }
        }

        bcast_handle.abort();
        write_handle.abort();
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Init logging — set RUST_LOG before any threads start
    let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    // SAFETY: called before any threads are spawned
    unsafe { env::set_var("RUST_LOG", &log_level) };
    env_logger::init();

    let gateway = Gateway::new();
    gateway.run().await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- utc_timestamp -------------------------------------------------------

    #[test]
    fn test_utc_timestamp_format() {
        let ts = utc_timestamp();
        // HH:MM:SS.mmm  — exactly 12 chars
        assert_eq!(ts.len(), 12, "timestamp length should be 12: {}", ts);
        assert_eq!(&ts.as_bytes()[2], &b':', "ts[2] should be ':'");
        assert_eq!(&ts.as_bytes()[5], &b':', "ts[5] should be ':'");
        assert_eq!(&ts.as_bytes()[8], &b'.', "ts[8] should be '.'");
    }

    // -- encode_raw_line -----------------------------------------------------

    #[test]
    fn test_encode_raw_received() {
        let line = encode_raw_line(0x19F51323, &[0x01, 0x02, 0x03, 0x04], 'R');
        let text = String::from_utf8(line).unwrap();
        assert!(text.contains(" R "), "should contain ' R ': {}", text);
        assert!(text.contains("19F51323"), "should contain CAN ID: {}", text);
        assert!(
            text.contains("01 02 03 04"),
            "should contain data: {}",
            text
        );
        assert!(text.ends_with("\r\n"), "should end with CRLF");
    }

    #[test]
    fn test_encode_raw_transmit() {
        let line = encode_raw_line(0x09F805FD, &[0xFF], 'T');
        let text = String::from_utf8(line).unwrap();
        assert!(text.contains(" T "), "should contain ' T ': {}", text);
        assert!(text.contains("09F805FD"));
        assert!(text.contains("FF"));
    }

    #[test]
    fn test_encode_raw_empty_data() {
        let line = encode_raw_line(0x00000001, &[], 'R');
        let text = String::from_utf8(line).unwrap();
        assert!(text.contains("00000001"));
        // timestamp + R + CANID = 3 tokens, no data bytes
        let parts: Vec<&str> = text.trim().split_whitespace().collect();
        assert_eq!(parts.len(), 3, "expected 3 parts: {:?}", parts);
    }

    #[test]
    fn test_encode_raw_full_8_bytes() {
        let data: Vec<u8> = (0..8).collect();
        let line = encode_raw_line(0x1FFFFFFF, &data, 'R');
        let text = String::from_utf8(line).unwrap();
        assert!(text.contains("00 01 02 03 04 05 06 07"));
    }

    // -- parse_raw_line ------------------------------------------------------

    #[test]
    fn test_parse_bare_id_and_data() {
        let result = parse_raw_line("19F51323 01 02 03 04");
        assert!(result.is_some());
        let (id, data) = result.unwrap();
        assert_eq!(id, 0x19F51323);
        assert_eq!(data, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_parse_with_timestamp_and_direction() {
        let result = parse_raw_line("12:30:15.482 R 19F51323 01 02 03 04");
        assert!(result.is_some());
        let (id, data) = result.unwrap();
        assert_eq!(id, 0x19F51323);
        assert_eq!(data, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_parse_with_timestamp_and_t() {
        let result = parse_raw_line("00:00:00.000 T 09F805FD FF");
        let (id, data) = result.unwrap();
        assert_eq!(id, 0x09F805FD);
        assert_eq!(data, vec![0xFF]);
    }

    #[test]
    fn test_parse_direction_only_prefix() {
        let result = parse_raw_line("R 19F51323 AA BB");
        let (id, data) = result.unwrap();
        assert_eq!(id, 0x19F51323);
        assert_eq!(data, vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_parse_empty_string() {
        assert!(parse_raw_line("").is_none());
    }

    #[test]
    fn test_parse_whitespace() {
        assert!(parse_raw_line("   ").is_none());
    }

    #[test]
    fn test_parse_invalid_hex() {
        assert!(parse_raw_line("ZZZZZZZZ 01").is_none());
    }

    #[test]
    fn test_parse_no_data_bytes() {
        let result = parse_raw_line("19F51323");
        let (id, data) = result.unwrap();
        assert_eq!(id, 0x19F51323);
        assert!(data.is_empty());
    }

    #[test]
    fn test_parse_timestamp_direction_no_id_returns_none() {
        assert!(parse_raw_line("12:30:15.482 R").is_none());
    }

    // -- roundtrip -----------------------------------------------------------

    #[test]
    fn test_roundtrip_received() {
        let can_id: u32 = 0x19F51323;
        let data = vec![0x01, 0x02, 0x03];
        let encoded = String::from_utf8(encode_raw_line(can_id, &data, 'R')).unwrap();
        let (parsed_id, parsed_data) = parse_raw_line(&encoded).unwrap();
        assert_eq!(parsed_id, can_id);
        assert_eq!(parsed_data, data);
    }

    #[test]
    fn test_roundtrip_transmit() {
        let can_id: u32 = 0x09F805FD;
        let data = vec![0xFF, 0x00];
        let encoded = String::from_utf8(encode_raw_line(can_id, &data, 'T')).unwrap();
        let (parsed_id, parsed_data) = parse_raw_line(&encoded).unwrap();
        assert_eq!(parsed_id, can_id);
        assert_eq!(parsed_data, data);
    }

    // -- Gateway construction ------------------------------------------------

    #[test]
    fn test_gateway_defaults() {
        // Clear relevant env vars to test defaults
        std::env::remove_var("CAN_INTERFACE");
        std::env::remove_var("LISTEN_HOST");
        std::env::remove_var("LISTEN_PORT");

        let gw = Gateway::new();
        assert_eq!(gw.can_interface, "can0");
        assert_eq!(gw.host, "0.0.0.0");
        assert_eq!(gw.port, 2598);
    }

    #[test]
    fn test_gateway_from_env() {
        std::env::set_var("CAN_INTERFACE", "vcan0");
        std::env::set_var("LISTEN_HOST", "127.0.0.1");
        std::env::set_var("LISTEN_PORT", "3000");

        let gw = Gateway::new();
        assert_eq!(gw.can_interface, "vcan0");
        assert_eq!(gw.host, "127.0.0.1");
        assert_eq!(gw.port, 3000);

        // Clean up
        std::env::remove_var("CAN_INTERFACE");
        std::env::remove_var("LISTEN_HOST");
        std::env::remove_var("LISTEN_PORT");
    }
}
