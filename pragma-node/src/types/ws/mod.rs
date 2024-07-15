pub mod metrics;

use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use metrics::{Interaction, Status};
use nonzero_ext::nonzero;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt::Debug;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::AppState;
use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use thiserror::Error;
use tokio::sync::{watch, Mutex};
use tokio::time::{interval, Interval};
use uuid::Uuid;

#[derive(Default, Debug, Serialize, Deserialize)]
pub enum SubscriptionType {
    #[serde(rename = "subscribe")]
    #[default]
    Subscribe,
    #[serde(rename = "unsubscribe")]
    Unsubscribe,
}

#[derive(Debug, Error)]
pub enum WebSocketError {
    #[error("could not create a channel with the client")]
    ChannelInitError,
    #[error("could not decode client message: {0}")]
    MessageDecodeError(String),
    #[error("could not close the channel")]
    ChannelCloseError,
}

/// Subscriber is an actor that handles a single websocket connection.
/// It listens to the store for updates and sends them to the client.
#[allow(dead_code)]
pub struct Subscriber<ChannelState> {
    pub id: Uuid,
    pub ip_address: IpAddr,
    pub closed: bool,
    pub state: Arc<Mutex<ChannelState>>,
    pub app_state: Arc<AppState>,
    pub sender: SplitSink<WebSocket, Message>,
    pub receiver: SplitStream<WebSocket>,
    pub update_interval: Interval,
    pub notify_receiver: Receiver<Message>,
    pub rate_limiter: DefaultKeyedRateLimiter<IpAddr>,
    pub exit: (watch::Sender<bool>, watch::Receiver<bool>),
}

/// The maximum number of bytes that can be sent per second per IP address.
/// If the limit is exceeded, the connection is closed.
const BYTES_LIMIT_PER_IP_PER_SECOND: u32 = 256 * 1024; // 256 KiB

pub trait ChannelHandler<ChannelState, CM, Err> {
    /// Called after a message is received from the client.
    /// The handler should process the message and update the state.
    async fn handle_client_msg(
        &mut self,
        subscriber: &mut Subscriber<ChannelState>,
        message: CM,
    ) -> Result<(), Err>;

    /// Called at a regular interval to update the client with the latest state.
    async fn periodic_interval(
        &mut self,
        subscriber: &mut Subscriber<ChannelState>,
    ) -> Result<(), Err>;
}

impl<ChannelState> Subscriber<ChannelState>
where
    ChannelState: Default + Debug,
{
    /// Create a new subscriber tied to a websocket connection.
    pub async fn new(
        socket: WebSocket,
        ip_address: IpAddr,
        app_state: Arc<AppState>,
        state: Option<ChannelState>,
        update_interval_in_ms: u64,
    ) -> Result<(Self, Sender<Message>), WebSocketError> {
        let id = Uuid::new_v4();
        let (sender, receiver) = socket.split();
        let (notify_sender, notify_receiver) = mpsc::channel::<Message>(32);

        let mut subscriber = Subscriber {
            id,
            ip_address,
            closed: false,
            state: Arc::new(Mutex::new(state.unwrap_or_default())),
            app_state,
            sender,
            receiver,
            update_interval: interval(Duration::from_millis(update_interval_in_ms)),
            notify_receiver,
            rate_limiter: RateLimiter::dashmap(Quota::per_second(nonzero!(
                BYTES_LIMIT_PER_IP_PER_SECOND
            ))),
            exit: watch::channel(false),
        };
        subscriber.assert_is_healthy().await?;
        // Retain the recent rate limit data for the IP addresses to
        // prevent the rate limiter size from growing indefinitely.
        subscriber.rate_limiter.retain_recent();
        metrics::record_ws_interaction(Interaction::NewConnection, Status::Success);
        Ok((subscriber, notify_sender))
    }

    /// Perform the initial handshake with the client - ensure the channel is healthy
    async fn assert_is_healthy(&mut self) -> Result<(), WebSocketError> {
        let ping_status = self.sender.send(Message::Ping(vec![1, 2, 3])).await;
        if ping_status.is_err() {
            metrics::record_ws_interaction(Interaction::NewConnection, Status::Error);
            return Err(WebSocketError::ChannelInitError);
        }
        Ok(())
    }

    /// Listen to messages from the client and the server.
    /// The handler is responsible for processing the messages and updating the state.
    pub async fn listen<H, CM, Err>(&mut self, mut handler: H) -> Result<(), Err>
    where
        H: ChannelHandler<ChannelState, CM, Err>,
        CM: for<'a> Deserialize<'a>,
    {
        let tracing_span = tracing::span!(tracing::Level::INFO, "subscriber", id = %self.id);
        let _tracing_guard = tracing_span.enter();
        loop {
            tokio::select! {
                // Messages from the client
                maybe_client_msg = self.receiver.next() => {
                    match maybe_client_msg {
                        Some(Ok(client_msg)) => {
                            tracing::info!("👤 [CLIENT -> SERVER]");
                            tracing::info!("{:?}", client_msg);
                            handler = self.decode_and_handle(handler, client_msg).await?;
                        }
                        Some(Err(_)) => {
                            tracing::info!("😶‍🌫️ Client disconnected/error occurred. Closing the channel.");
                            return Ok(());
                        },
                        None => {}
                    }
                },
                // Periodic updates
                _ = self.update_interval.tick() => {
                    let status = handler.periodic_interval(self).await;
                    match status {
                        Ok(_) => {
                            metrics::record_ws_interaction(Interaction::ChannelUpdate, Status::Success);
                        },
                        Err(e) => {
                            metrics::record_ws_interaction(Interaction::ChannelUpdate, Status::Error);
                            metrics::record_ws_interaction(Interaction::CloseConnection, Status::Success);
                            return Err(e);
                        }
                    }
                },
                // Messages from the server to the client
                maybe_server_msg = self.notify_receiver.recv() => {
                    if let Some(server_msg) = maybe_server_msg {
                        tracing::info!("🥡 [SERVER -> CLIENT]");
                        tracing::info!("{:?}", server_msg);
                        let _ = self.sender.send(server_msg).await;
                    }
                },
                // Exit signal
                _ = self.exit.1.changed() => {
                    if *self.exit.1.borrow() {
                        self.sender.close().await.ok();
                        self.closed = true;
                        metrics::record_ws_interaction(Interaction::CloseConnection, Status::Success);
                        tracing::info!("⛔ [CLOSING SIGNAL]");
                        return Ok(());
                    }
                },
            }
        }
    }

    /// Called after a message is received from the client.
    /// The handler should process the message and update the state.
    /// If the handler returns an error, the connection will be closed.
    async fn decode_and_handle<H, CM, Err>(
        &mut self,
        mut handler: H,
        client_msg: Message,
    ) -> Result<H, Err>
    where
        H: ChannelHandler<ChannelState, CM, Err>,
        CM: for<'a> Deserialize<'a>,
    {
        let status_decoded_msg = self.decode_msg::<CM>(client_msg).await;
        if let Ok(maybe_client_msg) = status_decoded_msg {
            if let Some(client_msg) = maybe_client_msg {
                metrics::record_ws_interaction(Interaction::ClientMessageDecode, Status::Success);
                let status = handler.handle_client_msg(self, client_msg).await;
                match status {
                    Ok(_) => {
                        metrics::record_ws_interaction(
                            Interaction::ClientMessageProcess,
                            Status::Success,
                        );
                    }
                    Err(e) => {
                        metrics::record_ws_interaction(
                            Interaction::ClientMessageProcess,
                            Status::Error,
                        );
                        metrics::record_ws_interaction(
                            Interaction::CloseConnection,
                            Status::Success,
                        );
                        return Err(e);
                    }
                }
            }
        } else {
            metrics::record_ws_interaction(Interaction::ClientMessageDecode, Status::Error);
        }
        Ok(handler)
    }

    /// Decode the message into the expected type.
    /// The message is expected to be in JSON format.
    /// If the message is not in the expected format, it will return None.
    /// If the message is a close signal, it will return None and send a close signal to the client.
    async fn decode_msg<T: for<'a> Deserialize<'a>>(
        &mut self,
        msg: Message,
    ) -> Result<Option<T>, WebSocketError> {
        match msg {
            Message::Close(_) => {
                tracing::info!("📨 [CLOSE]");
                if self.exit.0.send(true).is_ok() {
                    self.sender
                        .close()
                        .await
                        .map_err(|_| WebSocketError::ChannelCloseError)?;
                    self.closed = true;
                } else {
                    metrics::record_ws_interaction(Interaction::CloseConnection, Status::Error);
                }
            }
            Message::Text(text) => {
                tracing::info!("📨 [TEXT]");
                let msg = serde_json::from_str::<T>(&text);
                if let Ok(msg) = msg {
                    return Ok(Some(msg));
                } else {
                    self.send_err("⛔ Incorrect message. Please check the documentation for more information.").await;
                    return Err(WebSocketError::MessageDecodeError(text));
                }
            }
            Message::Binary(payload) => {
                tracing::info!("📨 [BINARY]");
                let maybe_msg = serde_json::from_slice::<T>(&payload);
                if let Ok(msg) = maybe_msg {
                    return Ok(Some(msg));
                } else {
                    self.send_err("⛔ Incorrect message. Please check the documentation for more information.").await;
                    return Err(WebSocketError::MessageDecodeError(format!("{:?}", payload)));
                }
            }
            // Ignore pings and pongs messages
            _ => {}
        }
        Ok(None)
    }

    /// Send a message to the client.
    pub async fn send_msg(&mut self, msg: String) -> Result<(), axum::Error> {
        self.sender.send(Message::Text(msg)).await
    }

    /// Send an error message to the client without closing the channel.
    pub async fn send_err(&mut self, err: &str) {
        let err = json!({"error": err});
        let _ = self.sender.send(Message::Text(err.to_string())).await;
    }
}
