//! WhatsApp Web channel using wa-rs (native Rust implementation)
//!
//! This channel provides direct WhatsApp Web integration with:
//! - QR code and pair code linking
//! - End-to-end encryption via Signal Protocol
//! - Full Baileys parity (groups, media, presence, reactions, editing/deletion)
//!
//! # Feature Flag
//!
//! This channel requires the `whatsapp-web` feature flag:
//! ```sh
//! cargo build --features whatsapp-web
//! ```
//!
//! # Configuration
//!
//! ```toml
//! [channels_config.whatsapp]
//! session_path = "~/.zeroclaw/whatsapp-session.db"  # Required for Web mode
//! pair_phone = "15551234567"  # Optional: for pair code linking
//! allowed_numbers = ["+1234567890", "*"]  # Same as Cloud API
//! ```
//!
//! # Runtime Negotiation
//!
//! This channel is automatically selected when `session_path` is set in the config.
//! The Cloud API channel is used when `phone_number_id` is set.

use super::traits::{Channel, ChannelMessage, SendMessage};
use super::whatsapp_storage::RusqliteStore;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use tokio::select;

#[cfg(feature = "whatsapp-web")]
const WHATSAPP_WEB_CONNECT_GRACE: StdDuration = StdDuration::from_secs(120);
#[cfg(feature = "whatsapp-web")]
const WHATSAPP_WEB_ACTIVITY_STALE_AFTER: StdDuration = StdDuration::from_secs(90);
/// Grace period given to wa-rs internal reconnect logic before declaring the channel unhealthy.
#[cfg(feature = "whatsapp-web")]
const WHATSAPP_WEB_RECONNECT_GRACE: StdDuration = StdDuration::from_secs(60);

/// Number of consecutive *structural* PairError events before auto-deleting the stale session.
/// Transient network errors (timeout/socket/network) use the higher transient threshold.
#[cfg(feature = "whatsapp-web")]
const WHATSAPP_WEB_MAX_PAIR_ERRORS: u32 = 3;
/// Repeated transient PairErrors also indicate a stale session — just more slowly.
#[cfg(feature = "whatsapp-web")]
const WHATSAPP_WEB_MAX_TRANSIENT_PAIR_ERRORS: u32 = 5;

/// WhatsApp Web channel using wa-rs with custom rusqlite storage
///
/// # Status: Functional Implementation
///
/// This implementation uses the wa-rs Bot with our custom RusqliteStore backend.
///
/// # Configuration
///
/// ```toml
/// [channels_config.whatsapp]
/// session_path = "~/.zeroclaw/whatsapp-session.db"
/// pair_phone = "15551234567"  # Optional
/// allowed_numbers = ["+1234567890", "*"]
/// ```
#[cfg(feature = "whatsapp-web")]
pub struct WhatsAppWebChannel {
    /// Session database path
    session_path: String,
    /// Phone number for pair code linking (optional)
    pair_phone: Option<String>,
    /// Custom pair code (optional)
    pair_code: Option<String>,
    /// Allowed phone numbers (E.164 format) or "*" for all
    allowed_numbers: Vec<String>,
    /// Bot handle for shutdown
    bot_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Client handle for sending messages and typing indicators
    client: Arc<Mutex<Option<Arc<wa_rs::Client>>>>,
    /// Message sender channel
    tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<ChannelMessage>>>>,
    /// Last listener start timestamp
    last_started_at: Arc<Mutex<Option<Instant>>>,
    /// Last successful connectivity or inbound activity timestamp
    last_activity_at: Arc<Mutex<Option<Instant>>>,
    /// Most recent disconnect/error reason reported by the listener
    last_unhealthy_reason: Arc<Mutex<Option<String>>>,
    /// Consecutive PairError count across restarts (for circuit-breaker)
    pair_error_count: Arc<Mutex<u32>>,
    /// Timestamp when a Disconnected event was received (wa-rs internal reconnect in progress).
    /// Used to give the library a grace window before the supervisor declares the channel unhealthy.
    reconnect_started_at: Arc<Mutex<Option<Instant>>>,
}

impl WhatsAppWebChannel {
    /// Create a new WhatsApp Web channel
    ///
    /// # Arguments
    ///
    /// * `session_path` - Path to the SQLite session database
    /// * `pair_phone` - Optional phone number for pair code linking (format: "15551234567")
    /// * `pair_code` - Optional custom pair code (leave empty for auto-generated)
    /// * `allowed_numbers` - Phone numbers allowed to interact (E.164 format) or "*" for all
    #[cfg(feature = "whatsapp-web")]
    pub fn new(
        session_path: String,
        pair_phone: Option<String>,
        pair_code: Option<String>,
        allowed_numbers: Vec<String>,
    ) -> Self {
        Self {
            session_path,
            pair_phone,
            pair_code,
            allowed_numbers,
            bot_handle: Arc::new(Mutex::new(None)),
            client: Arc::new(Mutex::new(None)),
            tx: Arc::new(Mutex::new(None)),
            last_started_at: Arc::new(Mutex::new(None)),
            last_activity_at: Arc::new(Mutex::new(None)),
            last_unhealthy_reason: Arc::new(Mutex::new(None)),
            pair_error_count: Arc::new(Mutex::new(0)),
            reconnect_started_at: Arc::new(Mutex::new(None)),
        }
    }

    /// Check if a phone number is allowed (E.164 format: +1234567890)
    #[cfg(feature = "whatsapp-web")]
    fn is_number_allowed(&self, phone: &str) -> bool {
        let normalized_phone = Self::normalize_phone_value(phone);
        self.allowed_numbers
            .iter()
            .any(|n| n == "*" || Self::normalize_phone_value(n) == normalized_phone)
    }

    #[cfg(feature = "whatsapp-web")]
    fn is_any_number_allowed(allowed_numbers: &[String], phones: &[String]) -> bool {
        phones.iter().any(|phone| {
            let normalized_phone = Self::normalize_phone_value(phone);
            allowed_numbers.iter().any(|allowed| {
                allowed == "*" || Self::normalize_phone_value(allowed) == normalized_phone
            })
        })
    }

    #[cfg(feature = "whatsapp-web")]
    fn format_allowlist_context(
        raw_sender: &str,
        canonical_sender: &str,
        candidates: &[String],
        allowed_numbers: &[String],
        chat: &str,
    ) -> String {
        format!(
            "raw_sender={raw_sender} canonical_sender={canonical_sender} chat={chat} candidates={candidates:?} allowed_numbers={allowed_numbers:?}"
        )
    }

    #[cfg(feature = "whatsapp-web")]
    fn mark_listener_started(&self) {
        *self.last_started_at.lock() = Some(Instant::now());
        *self.last_unhealthy_reason.lock() = None;
        *self.reconnect_started_at.lock() = None;
        // Clear stale client/bot_handle from a previous aborted listener so
        // health_check() doesn't see an old dead client and skip the grace window.
        *self.client.lock() = None;
        if let Some(handle) = self.bot_handle.lock().take() {
            handle.abort();
        }
    }

    /// Normalize phone number to E.164 format
    #[cfg(feature = "whatsapp-web")]
    fn normalize_phone(&self, phone: &str) -> String {
        Self::normalize_phone_value(phone)
    }

    #[cfg(feature = "whatsapp-web")]
    fn normalize_phone_value(phone: &str) -> String {
        let trimmed = phone.trim();
        let user_part = trimmed
            .split_once('@')
            .map(|(user, _)| user)
            .unwrap_or(trimmed);
        let digits: String = user_part.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            user_part.to_string()
        } else {
            format!("+{digits}")
        }
    }

    /// Whether the recipient string is a WhatsApp JID (contains a domain suffix).
    #[cfg(feature = "whatsapp-web")]
    fn is_jid(recipient: &str) -> bool {
        recipient.trim().contains('@')
    }

    #[cfg(feature = "whatsapp-web")]
    fn is_group_jid(recipient: &str) -> bool {
        recipient.trim().ends_with("@g.us")
    }

    #[cfg(feature = "whatsapp-web")]
    async fn is_recipient_allowed(&self, recipient: &str) -> bool {
        if Self::is_group_jid(recipient) {
            return false;
        }

        let normalized = self.normalize_phone(recipient);
        if self.is_number_allowed(&normalized) {
            return true;
        }

        if !Self::is_jid(recipient) {
            return false;
        }

        let Ok(store) = RusqliteStore::new(&self.session_path) else {
            return false;
        };
        let candidates = Self::sender_allowlist_candidates(&store, recipient).await;
        Self::is_any_number_allowed(&self.allowed_numbers, &candidates)
    }

    #[cfg(feature = "whatsapp-web")]
    async fn sender_allowlist_candidates(store: &RusqliteStore, sender: &str) -> Vec<String> {
        use wa_rs_core::store::traits::ProtocolStore as _;

        let trimmed = sender.trim();
        let bare = trimmed
            .split_once('@')
            .map(|(user, _)| user)
            .unwrap_or(trimmed)
            .trim_start_matches('+');

        let mut candidates = BTreeSet::new();
        if !bare.is_empty() {
            candidates.insert(Self::normalize_phone_value(bare));
        }

        if let Ok(Some(entry)) = store.get_lid_mapping(bare).await {
            if !entry.phone_number.trim().is_empty() {
                candidates.insert(Self::normalize_phone_value(&entry.phone_number));
            }
            if !entry.lid.trim().is_empty() {
                candidates.insert(Self::normalize_phone_value(&entry.lid));
            }
        }

        if let Ok(Some(entry)) = store.get_pn_mapping(bare).await {
            if !entry.phone_number.trim().is_empty() {
                candidates.insert(Self::normalize_phone_value(&entry.phone_number));
            }
            if !entry.lid.trim().is_empty() {
                candidates.insert(Self::normalize_phone_value(&entry.lid));
            }
        }

        candidates.into_iter().collect()
    }

    #[cfg(feature = "whatsapp-web")]
    fn daemon_state_path(&self) -> Option<PathBuf> {
        let session_path = Path::new(&self.session_path);
        let zeroclaw_dir = session_path.parent()?.parent()?.parent()?;
        Some(zeroclaw_dir.join("daemon_state.json"))
    }

    #[cfg(feature = "whatsapp-web")]
    fn daemon_reports_healthy(&self) -> bool {
        let Some(state_path) = self.daemon_state_path() else {
            return false;
        };

        let Ok(contents) = std::fs::read_to_string(state_path) else {
            return false;
        };
        let Ok(snapshot) = serde_json::from_str::<Value>(&contents) else {
            return false;
        };

        let Some(pid) = snapshot.get("pid").and_then(Value::as_u64) else {
            return false;
        };
        if !Self::pid_is_alive(pid as u32) {
            return false;
        }

        let Some(written_at) = snapshot.get("written_at").and_then(Value::as_str) else {
            return false;
        };
        let Ok(written_at) = chrono::DateTime::parse_from_rfc3339(written_at) else {
            return false;
        };
        if chrono::Utc::now().signed_duration_since(written_at.with_timezone(&chrono::Utc))
            > chrono::Duration::seconds(30)
        {
            return false;
        }

        matches!(
            snapshot
                .pointer("/components/channel:whatsapp/status")
                .and_then(Value::as_str),
            Some("ok")
        )
    }

    #[cfg(feature = "whatsapp-web")]
    fn listener_looks_active(&self) -> bool {
        let client_connected = self.client.lock().is_some();
        let bot_running = self
            .bot_handle
            .lock()
            .as_ref()
            .is_some_and(|handle| !handle.is_finished());

        if !client_connected || !bot_running {
            return false;
        }

        if self.last_unhealthy_reason.lock().is_some() {
            return false;
        }

        let now = Instant::now();
        if let Some(last_activity) = *self.last_activity_at.lock() {
            return now.duration_since(last_activity) <= WHATSAPP_WEB_ACTIVITY_STALE_AFTER;
        }

        self.last_started_at
            .lock()
            .as_ref()
            .is_some_and(|started| now.duration_since(*started) <= WHATSAPP_WEB_CONNECT_GRACE)
    }

    #[cfg(all(feature = "whatsapp-web", unix))]
    fn pid_is_alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(all(feature = "whatsapp-web", not(unix)))]
    fn pid_is_alive(_pid: u32) -> bool {
        true
    }

    /// Convert a recipient to a wa-rs JID.
    ///
    /// Supports:
    /// - Full JIDs (e.g. "12345@s.whatsapp.net")
    /// - E.164-like numbers (e.g. "+1234567890")
    #[cfg(feature = "whatsapp-web")]
    fn recipient_to_jid(&self, recipient: &str) -> Result<wa_rs_binary::jid::Jid> {
        let trimmed = recipient.trim();
        if trimmed.is_empty() {
            anyhow::bail!("Recipient cannot be empty");
        }

        if trimmed.contains('@') {
            return trimmed
                .parse::<wa_rs_binary::jid::Jid>()
                .map_err(|e| anyhow!("Invalid WhatsApp JID `{trimmed}`: {e}"));
        }

        let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            anyhow::bail!("Recipient `{trimmed}` does not contain a valid phone number");
        }

        Ok(wa_rs_binary::jid::Jid::pn(digits))
    }

    /// Connect to WhatsApp Web, send one or more messages, and disconnect.
    ///
    /// This is intended for one-shot CLI usage (e.g. `invest-report run`).
    /// **Do not call while the daemon is running** — only one process should
    /// hold the session at a time.
    #[cfg(feature = "whatsapp-web")]
    pub async fn send_oneshot(&self, messages: Vec<SendMessage>) -> Result<()> {
        use wa_rs::bot::Bot;
        use wa_rs::store::{Device, DeviceStore};
        use wa_rs_core::types::events::Event;
        use wa_rs_tokio_transport::TokioWebSocketTransportFactory;
        use wa_rs_ureq_http::UreqHttpClient;

        for msg in &messages {
            if !self.is_recipient_allowed(&msg.recipient).await {
                tracing::warn!(
                    "WhatsApp Web oneshot: refusing to send to unauthorized recipient: {}",
                    msg.recipient
                );
                anyhow::bail!(
                    "WhatsApp Web recipient {} is not in channels_config.whatsapp.allowed_numbers",
                    msg.recipient
                );
            }
        }

        let storage = RusqliteStore::new(&self.session_path)?;
        let backend = Arc::new(storage);

        let mut device = Device::new(backend.clone());
        if backend.exists().await? {
            if let Some(core_device) = backend.load().await? {
                device.load_from_serializable(core_device);
            } else {
                anyhow::bail!("WhatsApp Web session exists but failed to load");
            }
        } else {
            anyhow::bail!("No WhatsApp Web session found — pair first via `zeroclaw daemon`");
        }

        let mut transport_factory = TokioWebSocketTransportFactory::new();
        if let Ok(ws_url) = std::env::var("WHATSAPP_WS_URL") {
            transport_factory = transport_factory.with_url(ws_url);
        }

        let http_client = UreqHttpClient::new();

        // Notify when connected so we know it's safe to send.
        let (connected_tx, connected_rx) = tokio::sync::oneshot::channel::<()>();
        let connected_tx = Arc::new(Mutex::new(Some(connected_tx)));

        let builder = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .on_event(move |event, _client| {
                let connected_tx = connected_tx.clone();
                async move {
                    if let Event::Connected(_) = event {
                        if let Some(tx) = connected_tx.lock().take() {
                            let _ = tx.send(());
                        }
                    }
                }
            });

        let mut bot = builder.build().await?;
        let client = bot.client();
        let bot_handle = bot.run().await?;

        // Wait for connection (with timeout).
        tokio::time::timeout(std::time::Duration::from_secs(30), connected_rx)
            .await
            .map_err(|_| anyhow!("WhatsApp Web connection timed out (30 s)"))?
            .map_err(|_| anyhow!("WhatsApp Web connection channel dropped"))?;

        tracing::info!(
            "WhatsApp Web oneshot: connected, sending {} message(s)",
            messages.len()
        );

        for msg in &messages {
            let to = self.recipient_to_jid(&msg.recipient)?;
            let outgoing = wa_rs_proto::whatsapp::Message {
                conversation: Some(msg.content.clone()),
                ..Default::default()
            };
            client.send_message(to, outgoing).await?;
        }

        tracing::info!("WhatsApp Web oneshot: all messages sent, disconnecting");
        bot_handle.abort();
        Ok(())
    }
}

#[cfg(feature = "whatsapp-web")]
#[async_trait]
impl Channel for WhatsAppWebChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    async fn send(&self, message: &SendMessage) -> Result<()> {
        let client = self.client.lock().clone();
        let Some(client) = client else {
            anyhow::bail!("WhatsApp Web client not connected. Initialize the bot first.");
        };

        if !self.is_recipient_allowed(&message.recipient).await {
            tracing::warn!(
                "WhatsApp Web: refusing to send to unauthorized recipient: {}",
                message.recipient
            );
            anyhow::bail!(
                "WhatsApp Web recipient {} is not in channels_config.whatsapp.allowed_numbers",
                message.recipient
            );
        }

        let to = self.recipient_to_jid(&message.recipient)?;
        let outgoing = wa_rs_proto::whatsapp::Message {
            conversation: Some(message.content.clone()),
            ..Default::default()
        };

        // Retry once on transient failure (network hiccup during send).
        match client.send_message(to.clone(), outgoing.clone()).await {
            Ok(message_id) => {
                // Successful send proves the connection is fully bidirectional —
                // reset the PairError counter so the circuit breaker starts fresh.
                *self.pair_error_count.lock() = 0;
                tracing::debug!(
                    "WhatsApp Web: sent message to {} (id: {})",
                    message.recipient,
                    message_id
                );
                Ok(())
            }
            Err(first_err) => {
                tracing::warn!(
                    "WhatsApp Web: send to {} failed ({}), retrying in 2s",
                    message.recipient,
                    first_err
                );
                tokio::time::sleep(StdDuration::from_secs(2)).await;
                let message_id = client.send_message(to, outgoing).await?;
                *self.pair_error_count.lock() = 0;
                tracing::debug!(
                    "WhatsApp Web: sent message to {} on retry (id: {})",
                    message.recipient,
                    message_id
                );
                Ok(())
            }
        }
    }

    async fn listen(&self, tx: tokio::sync::mpsc::Sender<ChannelMessage>) -> Result<()> {
        // Store the sender channel for incoming messages
        *self.tx.lock() = Some(tx.clone());
        self.mark_listener_started();

        use wa_rs::bot::Bot;
        use wa_rs::pair_code::PairCodeOptions;
        use wa_rs::store::{Device, DeviceStore};
        use wa_rs_binary::jid::JidExt as _;
        use wa_rs_core::proto_helpers::MessageExt;
        use wa_rs_core::types::events::Event;
        use wa_rs_tokio_transport::TokioWebSocketTransportFactory;
        use wa_rs_ureq_http::UreqHttpClient;

        tracing::info!(
            "WhatsApp Web channel starting (session: {})",
            self.session_path
        );

        // Initialize storage backend
        let storage = RusqliteStore::new(&self.session_path)?;
        let backend = Arc::new(storage);

        // Check if we have a saved device to load
        let mut device = Device::new(backend.clone());
        if backend.exists().await? {
            tracing::info!("WhatsApp Web: found existing session, loading device");
            if let Some(core_device) = backend.load().await? {
                device.load_from_serializable(core_device);
            } else {
                anyhow::bail!("Device exists but failed to load");
            }
        } else {
            tracing::info!(
                "WhatsApp Web: no existing session, new device will be created during pairing"
            );
        };

        // Create transport factory
        let mut transport_factory = TokioWebSocketTransportFactory::new();
        if let Ok(ws_url) = std::env::var("WHATSAPP_WS_URL") {
            transport_factory = transport_factory.with_url(ws_url);
        }

        // Create HTTP client for media operations
        let http_client = UreqHttpClient::new();

        // Build the bot
        let tx_clone = tx.clone();
        let allowed_numbers = self.allowed_numbers.clone();
        let session_path = self.session_path.clone(); // Clone for use in closure
        let backend_for_events = backend.clone();
        let last_activity_at = Arc::clone(&self.last_activity_at);
        let last_unhealthy_reason = Arc::clone(&self.last_unhealthy_reason);
        let pair_error_count = Arc::clone(&self.pair_error_count);
        let reconnect_started_at = Arc::clone(&self.reconnect_started_at);

        let mut builder = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .on_event(move |event, _client| {
                let tx_inner = tx_clone.clone();
                let allowed_numbers = allowed_numbers.clone();
                let session_path_inner = session_path.clone(); // Clone for this event
                let backend = backend_for_events.clone();
                let last_activity_at = last_activity_at.clone();
                let last_unhealthy_reason = last_unhealthy_reason.clone();
                let pair_error_count = pair_error_count.clone();
                let reconnect_started_at = reconnect_started_at.clone();
                async move {
                    match event {
                        Event::Message(msg, info) => {
                            *last_activity_at.lock() = Some(Instant::now());
                            *last_unhealthy_reason.lock() = None;
                            // NOTE: Do NOT reset pair_error_count here.
                            // Receiving messages does not prove the connection can send.
                            // The counter is reset on Event::Connected instead.

                            // Extract message content
                            let text = msg.text_content().unwrap_or("");
                            let sender = info.source.sender.user().to_string();
                            let chat = info.source.chat.to_string();
                            let sender_candidates =
                                Self::sender_allowlist_candidates(backend.as_ref(), &sender).await;
                            let canonical_sender = sender_candidates
                                .first()
                                .cloned()
                                .unwrap_or_else(|| Self::normalize_phone_value(&sender));

                            tracing::info!(
                                "WhatsApp Web message from {} in {}: {}",
                                sender,
                                chat,
                                text
                            );

                            // Check if sender is allowed
                            if Self::is_any_number_allowed(&allowed_numbers, &sender_candidates) {
                                tracing::debug!(
                                    "WhatsApp Web allowlist matched: {}",
                                    Self::format_allowlist_context(
                                        &sender,
                                        &canonical_sender,
                                        &sender_candidates,
                                        &allowed_numbers,
                                        &chat,
                                    )
                                );

                                // Skip group messages - only respond to direct messages
                                if Self::is_group_jid(&chat) {
                                    tracing::debug!(
                                        "WhatsApp Web: ignoring group message from {} in {}",
                                        canonical_sender,
                                        chat
                                    );
                                    return;
                                }

                                let trimmed = text.trim();
                                if trimmed.is_empty() {
                                    tracing::debug!(
                                        "WhatsApp Web: ignoring empty or non-text message from {}",
                                        canonical_sender
                                    );
                                    return;
                                }

                                // For DMs, prefer a phone-number JID over the LID
                                // JID — LID sends time out on some sessions.
                                let reply_target = if chat.ends_with("@lid") {
                                    let phone = canonical_sender.trim_start_matches('+');
                                    format!("{phone}@s.whatsapp.net")
                                } else {
                                    chat
                                };

                                if let Err(e) = tx_inner
                                    .send(ChannelMessage {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        channel: "whatsapp".to_string(),
                                        sender: canonical_sender.clone(),
                                        reply_target,
                                        content: trimmed.to_string(),
                                        timestamp: chrono::Utc::now().timestamp() as u64,
                                        thread_ts: None,
                                    })
                                    .await
                                {
                                    tracing::error!("Failed to send message to channel: {}", e);
                                }
                            } else {
                                tracing::warn!(
                                    "WhatsApp Web allowlist denied: {}",
                                    Self::format_allowlist_context(
                                        &sender,
                                        &canonical_sender,
                                        &sender_candidates,
                                        &allowed_numbers,
                                        &chat,
                                    )
                                );
                            }
                        }
                        Event::Connected(_) => {
                            *last_activity_at.lock() = Some(Instant::now());
                            *last_unhealthy_reason.lock() = None;
                            // Connected proves the pairing/auth handshake succeeded —
                            // reset PairError counter so stale counts from previous
                            // failed attempts don't immediately delete the new session.
                            *pair_error_count.lock() = 0;
                            *reconnect_started_at.lock() = None;
                            tracing::info!("WhatsApp Web connected successfully");
                        }
                        Event::LoggedOut(_) => {
                            *last_unhealthy_reason.lock() = Some("logged out".to_string());
                            tracing::warn!("WhatsApp Web was logged out");
                        }
                        Event::StreamError(stream_error) => {
                            *last_unhealthy_reason.lock() = Some(format!("stream error: {stream_error:?}"));
                            tracing::error!("WhatsApp Web stream error: {:?}", stream_error);
                        }
                        Event::PairingCode { code, .. } => {
                            tracing::info!("WhatsApp Web pair code received: {}", code);
                            tracing::info!(
                                "Link your phone by entering this code in WhatsApp > Linked Devices"
                            );
                        }
                        Event::PairingQrCode { code, .. } => {
                            tracing::info!(
                                "WhatsApp Web QR code received (scan with WhatsApp > Linked Devices)"
                            );
                            tracing::debug!("QR code: {}", code);

                            // Generate QR code
                            if let Ok(qr) = qrcode::QrCode::new(code.as_bytes()) {
                                // Display QR code in terminal
                                println!("\n\n📱 SCAN THIS QR CODE WITH YOUR PHONE:\n");
                                let string = qr.render()
                                    .quiet_zone(false)
                                    .light_color(' ')
                                    .dark_color('█')
                                    .build();
                                println!("{}\n", string);

                                // Save QR code as PNG image
                                let session_dir = std::path::Path::new(&session_path_inner)
                                    .parent()
                                    .unwrap_or(std::path::Path::new("."));
                                let qr_path = session_dir.join("qrcode.png");

                                let image = qr.render::<image::Luma<u8>>().build();
                                // Scale up the QR code for better visibility (10x)
                                let scaled = image::imageops::resize(
                                    &image,
                                    image.width() * 10,
                                    image.height() * 10,
                                    image::imageops::FilterType::Nearest
                                );

                                if let Err(e) = scaled.save(&qr_path) {
                                    tracing::warn!("Failed to save QR code image to {:?}: {}", qr_path, e);
                                } else {
                                    println!("✅ QR Code 已儲存至: {}\n", qr_path.display());
                                    tracing::info!("QR code image saved to {:?}", qr_path);
                                }
                            } else {
                                println!("\n\n📱 SCAN THIS QR CODE WITH YOUR PHONE:\n");
                                println!("QR Code data: {}\n", code);
                                println!("⚠️  Use a QR code generator to scan this data\n");
                            }
                        }
                        Event::PairError(pair_err) => {
                            // Transient network errors (socket timeout, connection reset, etc.)
                            // use a higher threshold than structural auth failures, but still
                            // count — repeated transient errors indicate a stale session.
                            let error_lower = pair_err.error.to_lowercase();
                            let is_transient = error_lower.contains("timeout")
                                || error_lower.contains("socket")
                                || error_lower.contains("network");

                            let mut c = pair_error_count.lock();
                            *c = c.saturating_add(1);
                            let count = *c;
                            drop(c);

                            let threshold = if is_transient {
                                WHATSAPP_WEB_MAX_TRANSIENT_PAIR_ERRORS
                            } else {
                                WHATSAPP_WEB_MAX_PAIR_ERRORS
                            };
                            let reason = format!("pair error: {}", pair_err.error);
                            *last_unhealthy_reason.lock() = Some(reason.clone());
                            tracing::warn!(
                                "WhatsApp Web PairError ({}/{}): {} — re-pairing may be required",
                                count,
                                threshold,
                                pair_err.error
                            );

                            if count >= threshold {
                                // Session is stale and will never self-heal; delete it so
                                // the next restart triggers a fresh pair-code flow.
                                let db_path = std::path::Path::new(&session_path_inner);
                                if db_path.exists() {
                                    match std::fs::remove_file(db_path) {
                                        Ok(()) => tracing::warn!(
                                            "WhatsApp Web: deleted stale session {:?} after {} consecutive PairErrors — \
                                             will re-pair on next restart",
                                            db_path, count
                                        ),
                                        Err(e) => tracing::error!(
                                            "WhatsApp Web: failed to delete stale session {:?}: {}",
                                            db_path, e
                                        ),
                                    }
                                }

                                // Send Pushover alert so the user knows to re-pair.
                                if let Some(workspace_dir) = db_path
                                    .parent()
                                    .and_then(|p| p.parent())
                                    .and_then(|p| p.parent())
                                    .map(|p| p.join("workspace"))
                                {
                                    let msg = format!(
                                        "WhatsApp session expired after {} consecutive PairErrors. \
                                         Session deleted — will re-pair on next restart. \
                                         Please check your phone for a new pair code in WhatsApp > Linked Devices.",
                                        count
                                    );
                                    tokio::spawn(async move {
                                        crate::tools::pushover::send_pushover_alert(
                                            &workspace_dir,
                                            "⚠️ ZeroClaw: WhatsApp Re-pair Required",
                                            &msg,
                                        ).await;
                                    });
                                }
                            }
                        }
                        Event::ConnectFailure(cf) => {
                            *last_unhealthy_reason.lock() = Some(format!("connect failure: {cf:?}"));
                            tracing::error!("WhatsApp Web connect failure: {:?}", cf);
                        }
                        Event::StreamReplaced(_) => {
                            *last_unhealthy_reason.lock() = Some("stream replaced".to_string());
                            tracing::warn!("WhatsApp Web stream replaced (another session took over)");
                        }
                        Event::TemporaryBan(ban) => {
                            *last_unhealthy_reason.lock() = Some(format!("temporary ban: {ban:?}"));
                            tracing::error!("WhatsApp Web temporary ban: {:?}", ban);
                        }
                        Event::Disconnected(_) => {
                            *reconnect_started_at.lock() = Some(Instant::now());
                            *last_unhealthy_reason.lock() = Some("disconnected".to_string());
                            tracing::warn!("WhatsApp Web disconnected (wa-rs internal reconnect in progress)");
                        }
                        // Maintenance events that prove the connection is alive.
                        Event::Receipt(_)
                        | Event::OfflineSyncCompleted(_)
                        | Event::OfflineSyncPreview(_)
                        | Event::HistorySync(_)
                        | Event::Notification(_)
                        | Event::ChatPresence(_)
                        | Event::Presence(_)
                        | Event::PushNameUpdate(_) => {
                            *last_activity_at.lock() = Some(Instant::now());
                        }
                        other => {
                            tracing::debug!("WhatsApp Web unhandled event: {:?}", other);
                        }
                    }
                }
            })
            ;

        // Configure pair-code flow when a phone number is provided.
        if let Some(ref phone) = self.pair_phone {
            tracing::info!("WhatsApp Web: pair-code flow enabled for configured phone number");
            builder = builder.with_pair_code(PairCodeOptions {
                phone_number: phone.clone(),
                custom_code: self.pair_code.clone(),
                ..Default::default()
            });
        } else if self.pair_code.is_some() {
            tracing::warn!(
                "WhatsApp Web: pair_code is set but pair_phone is missing; pair code config is ignored"
            );
        }

        let mut bot = builder.build().await?;
        *self.client.lock() = Some(bot.client());

        // Run the bot
        let bot_handle = bot.run().await?;

        // Store the bot handle for later shutdown
        *self.bot_handle.lock() = Some(bot_handle);

        // Wait for bot task to finish OR Ctrl+C.
        // The bot_handle completing means the websocket connection died
        // (logged out, stream error, or silent disconnect).
        // The old code used a broadcast channel that was never sent to,
        // meaning listen() would block forever even after connection death.
        let bot_exit_reason = {
            // Take the handle out of the mutex so we can await it without
            // holding the lock across an await point.
            let handle = self.bot_handle.lock().take();
            if let Some(handle) = handle {
                select! {
                    join_result = handle => {
                        match join_result {
                            Ok(()) => "bot task exited cleanly".to_string(),
                            Err(e) if e.is_cancelled() => "bot task was cancelled".to_string(),
                            Err(e) => format!("bot task panicked: {e}"),
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("WhatsApp Web channel received Ctrl+C");
                        "ctrl-c".to_string()
                    }
                }
            } else {
                "bot handle was not stored".to_string()
            }
        };

        tracing::warn!("WhatsApp Web listen() ending: {bot_exit_reason}");

        *self.client.lock() = None;
        // bot_handle already taken above; abort just in case a new one was stored
        if let Some(handle) = self.bot_handle.lock().take() {
            handle.abort();
        }

        anyhow::bail!("WhatsApp Web connection lost: {bot_exit_reason}");
    }

    async fn health_check(&self) -> bool {
        // Grace window: if the listener was recently started, give it time
        // for QR scan / pair code completion before declaring unhealthy.
        // This must be checked FIRST because during setup the client and
        // bot_handle may be in transient states (set at different times).
        if let Some(started_at) = *self.last_started_at.lock() {
            if Instant::now().duration_since(started_at) <= WHATSAPP_WEB_CONNECT_GRACE {
                return true;
            }
        }

        // Primary: probe the actual socket state via the wa-rs Client.
        if let Some(client) = self.client.lock().clone() {
            if client.is_connected() && client.is_logged_in() {
                *self.last_activity_at.lock() = Some(Instant::now());
                return true;
            }
            // Socket is down but bot task may still be alive (wa-rs internal reconnect).
            let bot_alive = self
                .bot_handle
                .lock()
                .as_ref()
                .is_some_and(|h| !h.is_finished());

            if bot_alive {
                if let Some(reconnect_at) = *self.reconnect_started_at.lock() {
                    if Instant::now().duration_since(reconnect_at) <= WHATSAPP_WEB_RECONNECT_GRACE {
                        return true;
                    }
                }
            }
            return false;
        }

        self.daemon_reports_healthy()
    }

    async fn start_typing(&self, recipient: &str) -> Result<()> {
        let client = self.client.lock().clone();
        let Some(client) = client else {
            anyhow::bail!("WhatsApp Web client not connected. Initialize the bot first.");
        };

        if !self.is_recipient_allowed(recipient).await {
            tracing::warn!(
                "WhatsApp Web: typing target {} not in allowed list",
                recipient
            );
            return Ok(());
        }

        let to = self.recipient_to_jid(recipient)?;
        client
            .chatstate()
            .send_composing(&to)
            .await
            .map_err(|e| anyhow!("Failed to send typing state (composing): {e}"))?;

        tracing::debug!("WhatsApp Web: start typing for {}", recipient);
        Ok(())
    }

    async fn stop_typing(&self, recipient: &str) -> Result<()> {
        let client = self.client.lock().clone();
        let Some(client) = client else {
            anyhow::bail!("WhatsApp Web client not connected. Initialize the bot first.");
        };

        if !self.is_recipient_allowed(recipient).await {
            tracing::warn!(
                "WhatsApp Web: typing target {} not in allowed list",
                recipient
            );
            return Ok(());
        }

        let to = self.recipient_to_jid(recipient)?;
        client
            .chatstate()
            .send_paused(&to)
            .await
            .map_err(|e| anyhow!("Failed to send typing state (paused): {e}"))?;

        tracing::debug!("WhatsApp Web: stop typing for {}", recipient);
        Ok(())
    }
}

// Stub implementation when feature is not enabled
#[cfg(not(feature = "whatsapp-web"))]
pub struct WhatsAppWebChannel {
    _private: (),
}

#[cfg(not(feature = "whatsapp-web"))]
impl WhatsAppWebChannel {
    pub fn new(
        _session_path: String,
        _pair_phone: Option<String>,
        _pair_code: Option<String>,
        _allowed_numbers: Vec<String>,
    ) -> Self {
        Self { _private: () }
    }
}

#[cfg(not(feature = "whatsapp-web"))]
#[async_trait]
impl Channel for WhatsAppWebChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    async fn send(&self, _message: &SendMessage) -> Result<()> {
        anyhow::bail!(
            "WhatsApp Web channel requires the 'whatsapp-web' feature. \
            Enable with: cargo build --features whatsapp-web"
        );
    }

    async fn listen(&self, _tx: tokio::sync::mpsc::Sender<ChannelMessage>) -> Result<()> {
        anyhow::bail!(
            "WhatsApp Web channel requires the 'whatsapp-web' feature. \
            Enable with: cargo build --features whatsapp-web"
        );
    }

    async fn health_check(&self) -> bool {
        false
    }

    async fn start_typing(&self, _recipient: &str) -> Result<()> {
        anyhow::bail!(
            "WhatsApp Web channel requires the 'whatsapp-web' feature. \
            Enable with: cargo build --features whatsapp-web"
        );
    }

    async fn stop_typing(&self, _recipient: &str) -> Result<()> {
        anyhow::bail!(
            "WhatsApp Web channel requires the 'whatsapp-web' feature. \
            Enable with: cargo build --features whatsapp-web"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "whatsapp-web")]
    use wa_rs_core::store::traits::{LidPnMappingEntry, ProtocolStore};

    #[cfg(feature = "whatsapp-web")]
    fn make_channel() -> WhatsAppWebChannel {
        WhatsAppWebChannel::new(
            "/tmp/test-whatsapp.db".into(),
            None,
            None,
            vec!["+1234567890".into()],
        )
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_channel_name() {
        let ch = make_channel();
        assert_eq!(ch.name(), "whatsapp");
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_daemon_state_path_uses_zeroclaw_root() {
        let ch = WhatsAppWebChannel::new(
            "/tmp/zeroclaw/state/whatsapp-web/session.db".into(),
            None,
            None,
            vec![],
        );

        assert_eq!(
            ch.daemon_state_path().unwrap(),
            PathBuf::from("/tmp/zeroclaw/daemon_state.json")
        );
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_number_allowed_exact() {
        let ch = make_channel();
        assert!(ch.is_number_allowed("+1234567890"));
        assert!(!ch.is_number_allowed("+9876543210"));
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_number_allowed_wildcard() {
        let ch = WhatsAppWebChannel::new("/tmp/test.db".into(), None, None, vec!["*".into()]);
        assert!(ch.is_number_allowed("+1234567890"));
        assert!(ch.is_number_allowed("+9999999999"));
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_number_denied_empty() {
        let ch = WhatsAppWebChannel::new("/tmp/test.db".into(), None, None, vec![]);
        // Empty allowlist means "deny all" (matches channel-wide allowlist policy).
        assert!(!ch.is_number_allowed("+1234567890"));
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_number_allowed_normalizes_human_format() {
        let ch = WhatsAppWebChannel::new(
            "/tmp/test.db".into(),
            None,
            None,
            vec!["+1 (236) 965-1383".into()],
        );
        assert!(ch.is_number_allowed("+12369651383"));
        assert!(ch.is_number_allowed("12369651383@s.whatsapp.net"));
        assert!(!ch.is_number_allowed("+12369651384"));
    }

    #[tokio::test]
    #[cfg(feature = "whatsapp-web")]
    async fn whatsapp_web_recipient_denies_group_jid_even_with_digits() {
        let ch = WhatsAppWebChannel::new(
            "/tmp/test.db".into(),
            None,
            None,
            vec!["+12369651383".into()],
        );
        assert!(!ch.is_recipient_allowed("12369651383-1@g.us").await);
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_normalize_phone_adds_plus() {
        let ch = make_channel();
        assert_eq!(ch.normalize_phone("1234567890"), "+1234567890");
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_normalize_phone_preserves_plus() {
        let ch = make_channel();
        assert_eq!(ch.normalize_phone("+1234567890"), "+1234567890");
    }

    #[test]
    #[cfg(feature = "whatsapp-web")]
    fn whatsapp_web_normalize_phone_from_jid() {
        let ch = make_channel();
        assert_eq!(
            ch.normalize_phone("1234567890@s.whatsapp.net"),
            "+1234567890"
        );
    }

    #[tokio::test]
    #[cfg(feature = "whatsapp-web")]
    async fn whatsapp_web_sender_candidates_include_mapped_phone_number() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let session_path = temp_dir.path().join("session.db");
        let store = RusqliteStore::new(session_path.to_str().expect("utf8 path"))
            .expect("store should initialize");

        ProtocolStore::put_lid_mapping(
            &store,
            &LidPnMappingEntry {
                lid: "900000000000001".to_string(),
                phone_number: "12369651383".to_string(),
                created_at: 1_700_000_000,
                learning_source: "test".to_string(),
                updated_at: 1_700_000_001,
            },
        )
        .await
        .expect("lid mapping should save");

        let candidates =
            WhatsAppWebChannel::sender_allowlist_candidates(&store, "900000000000001@lid").await;

        assert!(candidates.contains(&"+12369651383".to_string()));
        assert!(candidates.contains(&"+900000000000001".to_string()));
    }

    #[tokio::test]
    #[cfg(feature = "whatsapp-web")]
    async fn whatsapp_web_health_check_disconnected() {
        let ch = make_channel();
        assert!(!ch.health_check().await);
    }
}
