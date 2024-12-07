use chrono::Utc;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{SinkExt as _, StreamExt as _};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use serde::{Deserialize, Serialize};
use starknet::core::utils::parse_cairo_short_string;
use std::env;
use std::{io, sync::mpsc, thread, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

const TEST_PAIRS: &[&str] = &[
    "BTC/USD", "ETH/USD", "SOL/USD", "DOGE/USD",
    "BNB/USD",
    // "XRP/USD",
    // "1000PEPE/USD",
    // "WIF/USD",
    // "NEAR/USD",
    // "AVAX/USD",
    // "LTC/USD",
    // "TRX/USD",
    // "ADA/USD",
    // "LINK/USD",
    // "BCH/USD",
    // "ARB/USD",
    // "WLD/USD",
    // "OP/USD",
    // "ONDO/USD",
    // "SUI/USD",
    // "ATOM/USD",
    // "FIL/USD",
    // "TIA/USD",
    // "POPCAT/USD",
    // "SEI/USD",
    // "FTM/USD",
    // "GOAT/USD",
    // "1000BONK/USD",
    // "1000SHIB/USD",
    // "MOODENG/USD",
    // "LDO/USD",
    // "APE/USD",
    // "JUP/USD",
    // "CRV/USD",
];

const TEST_MARK_PAIRS: &[&str] = &[
    // "BTC/USD",
    // "ETH/USD",
    // "SOL/USD",
    // "DOGE/USD",
    // "BNB/USD",
    // "XRP/USD",
    // "1000PEPE/USD",
    // "WIF/USD",
    // "NEAR/USD",
    // "AVAX/USD",
    // "LTC/USD",
    // "TRX/USD",
    // "ADA/USD",
    // "LINK/USD",
    // "BCH/USD",
    // "ARB/USD",
    // "WLD/USD",
    // "OP/USD",
    // "ONDO/USD",
    // "SUI/USD",
    // "ATOM/USD",
    // "FIL/USD",
    // "TIA/USD",
    // "POPCAT/USD",
    // "SEI/USD",
    // "FTM/USD",
    // "GOAT/USD",
    // "1000BONK/USD",
    // "1000SHIB/USD",
    // "MOODENG/USD",
    // "LDO/USD",
    // "APE/USD",
    // "JUP/USD",
    // "CRV/USD",
];

#[derive(Serialize, Deserialize, Debug)]
struct SubscribeMessage {
    msg_type: String,
    pairs: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct SignedPublisherPrice {
    oracle_asset_id: String,
    oracle_price: String,
    signing_key: String,
    signature: String,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct AssetOraclePrice {
    global_asset_id: String,
    median_price: String,
    signature: String,
    signed_prices: Vec<SignedPublisherPrice>,
}

#[derive(Debug, Deserialize)]
struct SubscribeToEntryResponse {
    oracle_prices: Vec<AssetOraclePrice>,
    timestamp: i64,
}

#[derive(Debug)]
struct Environment {
    ws_url: String,
}

impl Environment {
    fn new() -> Self {
        // Default to 'dev' if not specified
        let env_type = env::var("PRAGMA_ENV").unwrap_or_else(|_| "dev".to_string());

        let ws_url = match env_type.as_str() {
            "prod" => "wss://ws.pragma.build/node/v1/data/subscribe",
            "dev" => "wss://ws.dev.pragma.build/node/v1/data/subscribe",
            "local" => "ws://0.0.0.0:3000/node/v1/data/subscribe",
            _ => panic!(
                "Invalid environment: {}. Use 'prod', 'dev', or 'local'",
                env_type
            ),
        }
        .to_string();

        Environment { ws_url }
    }
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct SubscriptionAck {
    msg_type: String,
    pairs: Vec<String>,
}

struct App {
    subscription_pairs: Vec<String>,
    latest_update: Option<SubscribeToEntryResponse>,
    should_quit: bool,
    current_time: i64,
    last_timestamps: std::collections::HashMap<String, i64>,
    scroll_offset: usize,
}

impl App {
    fn new() -> App {
        App {
            subscription_pairs: Vec::new(),
            latest_update: None,
            should_quit: false,
            current_time: Utc::now().timestamp(),
            last_timestamps: std::collections::HashMap::new(),
            scroll_offset: 0,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Terminal initialization
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channels for WebSocket messages
    let (tx, rx) = mpsc::channel();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let mut app = App::new();

    // Spawn WebSocket thread
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let env = Environment::new();

            // Convert spot pairs
            let index_pairs: Vec<String> = TEST_PAIRS.iter().map(|&p| p.to_string()).collect();

            // Convert mark pairs and add :MARK suffix
            let mark_pairs: Vec<String> = TEST_MARK_PAIRS
                .iter()
                .map(|&p| format!("{}:MARK", p))
                .collect();

            // Combine both sets of pairs
            let mut all_pairs = index_pairs.clone();
            all_pairs.extend(mark_pairs.clone());

            let url = Url::parse(&env.ws_url).unwrap();
            let (mut socket, _) = connect_async(url).await.unwrap();

            let subscribe_msg = SubscribeMessage {
                msg_type: "subscribe".to_string(),
                pairs: all_pairs.clone(),
            };

            let msg_str = serde_json::to_string(&subscribe_msg).unwrap();
            socket.send(Message::text(msg_str)).await.unwrap();

            loop {
                tokio::select! {
                    Some(message) = socket.next() => {
                        match message {
                            Ok(msg) => {
                                if let Message::Text(text) = msg {
                                    if tx.send(text).is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("WebSocket error: {}", e);
                                break;
                            }
                        }
                    }
                    Ok(_) = &mut shutdown_rx => {
                        // Clean shutdown
                        let _ = socket.close(None).await;
                        break;
                    }
                }
            }
        });
    });

    // Main loop
    loop {
        app.current_time = Utc::now().timestamp();
        terminal.draw(|f| ui(f, &app))?;

        // Handle events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Up => {
                        if app.scroll_offset > 0 {
                            app.scroll_offset -= 1;
                        }
                    }
                    KeyCode::Down => {
                        app.scroll_offset += 1;
                    }
                    _ => {}
                }
            }
        }

        // Check for WebSocket messages
        if let Ok(text) = rx.try_recv() {
            if let Ok(ack) = serde_json::from_str::<SubscriptionAck>(&text) {
                app.subscription_pairs = ack.pairs;
            } else if let Ok(response) = serde_json::from_str::<SubscribeToEntryResponse>(&text) {
                // Update timestamps only for changed prices
                for price in &response.oracle_prices {
                    let key = price.global_asset_id.clone();
                    if app.latest_update.as_ref().map_or(true, |last| {
                        last.oracle_prices
                            .iter()
                            .find(|p| p.global_asset_id == key)
                            .map_or(true, |p| p.median_price != price.median_price)
                    }) {
                        app.last_timestamps.insert(key, response.timestamp);
                    }
                }
                app.latest_update = Some(response);
            }
        }

        if app.should_quit {
            let _ = shutdown_tx.send(()); // Signal WebSocket thread to shutdown
            thread::sleep(Duration::from_millis(100)); // Brief pause to allow cleanup
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Converts a hexadecimal asset ID to a human-readable string format.
/// If the input starts with "0x", attempts to parse it as a Cairo short string.
/// Otherwise, returns the original string.
///
/// # Arguments
/// * `hex_id` - The hexadecimal asset ID to parse
///
/// # Returns
/// A String containing either the parsed asset name or the original hex ID
fn parse_hex_asset_id(hex_id: &str) -> String {
    if !hex_id.starts_with("0x") {
        return hex_id.to_string();
    }

    let hex_str = &hex_id[2..];
    u128::from_str_radix(hex_str, 16)
        .ok()
        .and_then(|felt| parse_cairo_short_string(&felt.into()).ok())
        .unwrap_or_else(|| hex_id.to_string())
        .replace("/", "")
}

/// Extracts and formats all received pairs from oracle prices.
///
/// # Arguments
/// * `oracle_prices` - Slice of AssetOraclePrice containing the received price updates
///
/// # Returns
/// A Vec<String> containing all formatted asset pairs (e.g., "ETHUSD")
fn get_received_pairs(oracle_prices: &[AssetOraclePrice]) -> Vec<String> {
    oracle_prices
        .iter()
        .map(|p| parse_hex_asset_id(&p.global_asset_id))
        .collect()
}

/// Identifies which subscribed pairs are missing from the received pairs.
/// Handles the format difference between subscribed pairs (ETH/USD) and received pairs (ETHUSD).
///
/// # Arguments
/// * `subscribed` - Slice of subscribed pair strings (format: "ETH/USD")
/// * `received` - Slice of received pair strings (format: "ETHUSD")
///
/// # Returns
/// A Vec<String> containing all subscribed pairs that weren't received
fn get_missing_pairs(subscribed: &[String], received: &[String]) -> Vec<String> {
    subscribed
        .iter()
        .filter(|p| !received.contains(&p.replace("/", "")))
        .cloned()
        .collect()
}

/// Formats the missing pairs into a user-friendly status message.
///
/// # Arguments
/// * `missing_pairs` - Slice of strings containing the missing pairs
///
/// # Returns
/// A formatted string either confirming all pairs were received or listing missing pairs
fn format_missing_pairs_text(missing_pairs: &[String]) -> String {
    if missing_pairs.is_empty() {
        "✅ All pairs received".to_string()
    } else {
        format!("⚠️  Missing: {}", missing_pairs.join(", "))
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Subscription status
            Constraint::Length(3), // Missing pairs
            Constraint::Min(0),    // Price updates - takes remaining space
        ])
        .split(f.size());

    // Add latency display with milliseconds
    if let Some(update) = &app.latest_update {
        let latency_ms = (app.current_time - update.timestamp) * 1000; // Convert to milliseconds
        let latency_text = Paragraph::new(format!("⏱ Latency: {}ms", latency_ms))
            .alignment(Alignment::Right)
            .block(Block::default().borders(Borders::ALL));

        let latency_area = Rect {
            x: chunks[0].width - 25,
            y: 0,
            width: 25,
            height: 3,
        };
        f.render_widget(latency_text, latency_area);
    }

    // Subscription header
    let subscription_text = format!("Subscribed Pairs: {}", app.subscription_pairs.join(", "));
    let subscribed_pairs = Paragraph::new(subscription_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Subscription Status"),
        )
        .wrap(ratatui::widgets::Wrap { trim: true }); // Add text wrapping
    f.render_widget(subscribed_pairs, chunks[0]);

    // Missing pairs section
    if let Some(update) = &app.latest_update {
        let received_pairs = get_received_pairs(&update.oracle_prices);
        let missing_pairs = get_missing_pairs(&app.subscription_pairs, &received_pairs);
        let missing_text = format_missing_pairs_text(&missing_pairs);

        let missing_widget = Paragraph::new(missing_text).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Missing Pairs"),
        );
        f.render_widget(missing_widget, chunks[1]);

        // Price updates list
        let mut items = vec![];
        for price in &update.oracle_prices {
            let asset_display = if price.global_asset_id.starts_with("0x") {
                let hex_str = &price.global_asset_id[2..];
                if let Ok(felt) = u128::from_str_radix(hex_str, 16) {
                    parse_cairo_short_string(&felt.into())
                        .unwrap_or_else(|_| price.global_asset_id.clone())
                } else {
                    price.global_asset_id.clone()
                }
            } else {
                price.global_asset_id.clone()
            };

            let latency_ms = (app.current_time
                - app
                    .last_timestamps
                    .get(&price.global_asset_id)
                    .copied()
                    .unwrap_or(update.timestamp))
                * 1000;

            items.push(ListItem::new(vec![
                Line::from(format!(
                    "🔸 Asset: {} ({}) ⏱ {}ms",
                    asset_display, price.global_asset_id, latency_ms
                )),
                Line::from(format!("  ├─ Median Price: {}", price.median_price)),
                Line::from(format!("  ├─ Publishers: {}", price.signed_prices.len())),
            ]));

            for (idx, pub_price) in price.signed_prices.iter().enumerate() {
                let truncated_key = format!(
                    "{}...{}",
                    &pub_price.signing_key[..10],
                    &pub_price.signing_key[pub_price.signing_key.len() - 8..]
                );
                items.push(ListItem::new(vec![
                    Line::from(format!(
                        "     {}. Price: {}",
                        idx + 1,
                        pub_price.oracle_price
                    )),
                    Line::from(format!("        Key: {}", truncated_key)),
                ]));
            }
        }

        let prices_list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Price Updates (Timestamp: {})", update.timestamp)),
            )
            .style(Style::default())
            .start_corner(ratatui::layout::Corner::TopLeft)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        f.render_stateful_widget(
            prices_list,
            chunks[2],
            &mut ListState::default().with_offset(app.scroll_offset),
        );
    }
}
