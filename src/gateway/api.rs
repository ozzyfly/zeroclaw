//! REST API handlers for the web dashboard.
//!
//! Read-only `/api/*` routes use [`require_auth`]: a bearer token is required
//! only when `gateway.require_pairing` is enabled. Mutating routes use
//! [`require_auth_strict`], which always requires a valid bearer token
//! regardless of the `require_pairing` setting.

use super::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use serde::Deserialize;

// ── Bearer token auth extractor ─────────────────────────────────

/// Extract and validate bearer token from Authorization header.
fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
}

/// Standard error response for an unauthorized `/api/*` request.
fn unauthorized() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
        })),
    )
}

/// Verify bearer token against PairingGuard for a read-only route.
///
/// When `gateway.require_pairing` is false this returns `Ok(())` without a
/// token — read-only access stays convenient in local-trust mode. When pairing
/// is enabled, a valid bearer token is required.
fn require_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if !state.pairing.require_pairing() {
        return Ok(());
    }

    let token = extract_bearer_token(headers).unwrap_or("");
    if state.pairing.is_authenticated(token) {
        Ok(())
    } else {
        Err(unauthorized())
    }
}

/// Verify bearer token for a mutating route.
///
/// Unlike [`require_auth`], this never consults `require_pairing` — a valid
/// bearer token is *always* required. When no token has been paired yet the
/// request still fails closed with `401`; the error directs the caller to
/// `POST /pair` first.
fn require_auth_strict(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let token = extract_bearer_token(headers).unwrap_or("");
    if state.pairing.verify_token_strict(token) {
        Ok(())
    } else {
        Err(unauthorized())
    }
}

// ── Query parameters ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MemoryQuery {
    pub query: Option<String>,
    pub category: Option<String>,
}

#[derive(Deserialize)]
pub struct MemoryStoreBody {
    pub key: String,
    pub content: String,
    pub category: Option<String>,
}

#[derive(Deserialize)]
pub struct CronAddBody {
    pub name: Option<String>,
    pub schedule: String,
    pub command: String,
}

// ── Handlers ────────────────────────────────────────────────────

/// GET /api/status — system status overview
pub async fn handle_api_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let health = crate::health::snapshot();

    let mut channels = serde_json::Map::new();

    for (channel, present) in config.channels_config.channels() {
        channels.insert(channel.name().to_string(), serde_json::Value::Bool(present));
    }

    let body = serde_json::json!({
        "provider": config.default_provider,
        "model": state.model,
        "temperature": state.temperature,
        "uptime_seconds": health.uptime_seconds,
        "gateway_port": config.gateway.port,
        "locale": "en",
        "memory_backend": state.mem.name(),
        "paired": state.pairing.is_paired(),
        "channels": channels,
        "health": health,
    });

    Json(body).into_response()
}

/// GET /api/config — current config (api_key masked)
pub async fn handle_api_config_get(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();

    // Serialize to TOML, then mask sensitive fields
    let toml_str = match toml::to_string_pretty(&config) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to serialize config: {e}")})),
            )
                .into_response();
        }
    };

    // Mask api_key in the TOML output
    let masked = mask_sensitive_fields(&toml_str);

    Json(serde_json::json!({
        "format": "toml",
        "content": masked,
    }))
    .into_response()
}

/// PUT /api/config — update config from TOML body
pub async fn handle_api_config_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if let Err(e) = require_auth_strict(&state, &headers) {
        return e.into_response();
    }

    // Parse the incoming TOML
    let new_config: crate::config::Config = match toml::from_str(&body) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid TOML: {e}")})),
            )
                .into_response();
        }
    };

    // Save to disk
    if let Err(e) = new_config.save().await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to save config: {e}")})),
        )
            .into_response();
    }

    // Update in-memory config
    *state.config.lock() = new_config;

    Json(serde_json::json!({"status": "ok"})).into_response()
}

/// GET /api/tools — list registered tool specs
pub async fn handle_api_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let tools: Vec<serde_json::Value> = state
        .tools_registry
        .iter()
        .map(|spec| {
            serde_json::json!({
                "name": spec.name,
                "description": spec.description,
                "parameters": spec.parameters,
            })
        })
        .collect();

    Json(serde_json::json!({"tools": tools})).into_response()
}

/// GET /api/cron — list cron jobs
pub async fn handle_api_cron_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    match crate::cron::list_jobs(&config) {
        Ok(jobs) => {
            let jobs_json: Vec<serde_json::Value> = jobs
                .iter()
                .map(|job| {
                    serde_json::json!({
                        "id": job.id,
                        "name": job.name,
                        "command": job.command,
                        "next_run": job.next_run.to_rfc3339(),
                        "last_run": job.last_run.map(|t| t.to_rfc3339()),
                        "last_status": job.last_status,
                        "enabled": job.enabled,
                    })
                })
                .collect();
            Json(serde_json::json!({"jobs": jobs_json})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to list cron jobs: {e}")})),
        )
            .into_response(),
    }
}

/// POST /api/cron — add a new cron job
pub async fn handle_api_cron_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CronAddBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth_strict(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let schedule = crate::cron::Schedule::Cron {
        expr: body.schedule,
        tz: None,
    };

    match crate::cron::add_shell_job(&config, body.name, schedule, &body.command) {
        Ok(job) => Json(serde_json::json!({
            "status": "ok",
            "job": {
                "id": job.id,
                "name": job.name,
                "command": job.command,
                "enabled": job.enabled,
            }
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to add cron job: {e}")})),
        )
            .into_response(),
    }
}

/// DELETE /api/cron/:id — remove a cron job
pub async fn handle_api_cron_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth_strict(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    match crate::cron::remove_job(&config, &id) {
        Ok(()) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to remove cron job: {e}")})),
        )
            .into_response(),
    }
}

/// GET /api/integrations — list all integrations with status
pub async fn handle_api_integrations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let entries = crate::integrations::registry::all_integrations();

    let integrations: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            let status = (entry.status_fn)(&config);
            serde_json::json!({
                "name": entry.name,
                "description": entry.description,
                "category": entry.category,
                "status": status,
            })
        })
        .collect();

    Json(serde_json::json!({"integrations": integrations})).into_response()
}

/// POST /api/doctor — run diagnostics
pub async fn handle_api_doctor(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth_strict(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let results = crate::doctor::diagnose(&config);

    let ok_count = results
        .iter()
        .filter(|r| r.severity == crate::doctor::Severity::Ok)
        .count();
    let warn_count = results
        .iter()
        .filter(|r| r.severity == crate::doctor::Severity::Warn)
        .count();
    let error_count = results
        .iter()
        .filter(|r| r.severity == crate::doctor::Severity::Error)
        .count();

    Json(serde_json::json!({
        "results": results,
        "summary": {
            "ok": ok_count,
            "warnings": warn_count,
            "errors": error_count,
        }
    }))
    .into_response()
}

/// GET /api/memory — list or search memory entries
pub async fn handle_api_memory_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<MemoryQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    if let Some(ref query) = params.query {
        // Search mode
        match state.mem.recall(query, 50, None).await {
            Ok(entries) => Json(serde_json::json!({"entries": entries})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Memory recall failed: {e}")})),
            )
                .into_response(),
        }
    } else {
        // List mode
        let category = params.category.as_deref().map(|cat| match cat {
            "core" => crate::memory::MemoryCategory::Core,
            "daily" => crate::memory::MemoryCategory::Daily,
            "conversation" => crate::memory::MemoryCategory::Conversation,
            other => crate::memory::MemoryCategory::Custom(other.to_string()),
        });

        match state.mem.list(category.as_ref(), None).await {
            Ok(entries) => Json(serde_json::json!({"entries": entries})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Memory list failed: {e}")})),
            )
                .into_response(),
        }
    }
}

/// POST /api/memory — store a memory entry
pub async fn handle_api_memory_store(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MemoryStoreBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth_strict(&state, &headers) {
        return e.into_response();
    }

    let category = body
        .category
        .as_deref()
        .map(|cat| match cat {
            "core" => crate::memory::MemoryCategory::Core,
            "daily" => crate::memory::MemoryCategory::Daily,
            "conversation" => crate::memory::MemoryCategory::Conversation,
            other => crate::memory::MemoryCategory::Custom(other.to_string()),
        })
        .unwrap_or(crate::memory::MemoryCategory::Core);

    match state
        .mem
        .store(&body.key, &body.content, category, None)
        .await
    {
        Ok(()) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Memory store failed: {e}")})),
        )
            .into_response(),
    }
}

/// DELETE /api/memory/:key — delete a memory entry
pub async fn handle_api_memory_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth_strict(&state, &headers) {
        return e.into_response();
    }

    match state.mem.forget(&key).await {
        Ok(deleted) => {
            Json(serde_json::json!({"status": "ok", "deleted": deleted})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Memory forget failed: {e}")})),
        )
            .into_response(),
    }
}

/// GET /api/cost — cost summary
pub async fn handle_api_cost(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    if let Some(ref tracker) = state.cost_tracker {
        match tracker.get_summary() {
            Ok(summary) => Json(serde_json::json!({"cost": summary})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Cost summary failed: {e}")})),
            )
                .into_response(),
        }
    } else {
        Json(serde_json::json!({
            "cost": {
                "session_cost_usd": 0.0,
                "daily_cost_usd": 0.0,
                "monthly_cost_usd": 0.0,
                "total_tokens": 0,
                "request_count": 0,
                "by_model": {},
            }
        }))
        .into_response()
    }
}

/// GET /api/cli-tools — discovered CLI tools
pub async fn handle_api_cli_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let tools = crate::tools::cli_discovery::discover_cli_tools(&[], &[]);

    Json(serde_json::json!({"cli_tools": tools})).into_response()
}

/// GET /api/health — component health snapshot
pub async fn handle_api_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let snapshot = crate::health::snapshot();
    Json(serde_json::json!({"health": snapshot})).into_response()
}

// ── Helpers ─────────────────────────────────────────────────────

/// Mask the value of every secret-bearing field in a serialized TOML config.
///
/// A line is masked only when the text before its first `=` trims to an
/// *exact* key name in [`crate::config::SECRET_FIELD_NAMES`]. Exact-key
/// matching (rather than a prefix denylist) means a key like `my_api_key`
/// is left untouched, and — critically — every newly registered secret field
/// is covered automatically once added to the registry.
fn mask_sensitive_fields(toml_str: &str) -> String {
    let mut output = String::with_capacity(toml_str.len());
    for line in toml_str.lines() {
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            if crate::config::schema::is_secret_field(key) {
                output.push_str(&line[..=eq_pos]);
                output.push_str(" \"***MASKED***\"");
            } else {
                output.push_str(line);
            }
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// Sentinel value planted into every secret field; must never survive masking.
    const SENTINEL: &str = "PLAINTEXT_SECRET_SENTINEL";

    #[test]
    fn mask_replaces_each_registered_key() {
        // Exact-key match against the registry, with TOML-style indentation.
        for key in crate::config::SECRET_FIELD_NAMES {
            let toml = format!("  {key} = \"{SENTINEL}\"\n");
            let masked = mask_sensitive_fields(&toml);
            assert!(
                !masked.contains(SENTINEL),
                "registry key `{key}` was not masked: {masked}"
            );
            assert!(
                masked.contains("***MASKED***"),
                "registry key `{key}` did not receive the mask placeholder"
            );
        }
    }

    #[test]
    fn mask_leaves_non_secret_keys_untouched() {
        let toml = "model = \"gpt-4o-mini\"\nport = 8080\n";
        assert_eq!(mask_sensitive_fields(toml), toml);
    }

    #[test]
    fn mask_does_not_match_substring_keys() {
        // `my_api_key` / `api_key_extra` are not the exact key `api_key`.
        let toml = "my_api_key = \"keep\"\napi_key_extra = \"keep\"\n";
        let masked = mask_sensitive_fields(toml);
        assert!(
            masked.contains("\"keep\""),
            "exact-key match must not fire on substring keys: {masked}"
        );
    }

    #[test]
    fn populated_config_serializes_with_no_unmasked_secret() {
        // Build a Config with every reachable secret field set to the sentinel,
        // serialize to TOML, mask it, and assert nothing leaked.
        let mut config = Config::default();
        config.api_key = Some(SENTINEL.into());
        config.composio.api_key = Some(SENTINEL.into());
        config.browser.computer_use.api_key = Some(SENTINEL.into());
        config.web_search.brave_api_key = Some(SENTINEL.into());

        config.channels_config.telegram = Some(crate::config::TelegramConfig {
            bot_token: SENTINEL.into(),
            allowed_users: vec![],
            stream_mode: crate::config::StreamMode::default(),
            draft_update_interval_ms: 1000,
            interrupt_on_new_message: false,
            mention_only: false,
        });
        config.channels_config.slack = Some(crate::config::SlackConfig {
            bot_token: SENTINEL.into(),
            app_token: Some(SENTINEL.into()),
            channel_id: None,
            allowed_users: vec![],
        });
        config.channels_config.matrix = Some(crate::config::MatrixConfig {
            homeserver: "https://matrix.example.com".into(),
            access_token: SENTINEL.into(),
            user_id: None,
            device_id: None,
            room_id: "!room:example.com".into(),
            allowed_users: vec![],
        });
        config.channels_config.whatsapp = Some(crate::config::WhatsAppConfig {
            access_token: Some(SENTINEL.into()),
            phone_number_id: None,
            verify_token: None,
            app_secret: Some(SENTINEL.into()),
            session_path: None,
            pair_phone: None,
            pair_code: None,
            allowed_numbers: vec![],
        });
        config.channels_config.linq = Some(crate::config::LinqConfig {
            api_token: SENTINEL.into(),
            from_phone: "+15551234567".into(),
            signing_secret: Some(SENTINEL.into()),
            allowed_senders: vec![],
        });
        config.channels_config.nextcloud_talk = Some(crate::config::NextcloudTalkConfig {
            base_url: "https://cloud.example.com".into(),
            app_token: SENTINEL.into(),
            webhook_secret: None,
            allowed_users: vec![],
        });
        config.channels_config.webhook = Some(crate::config::WebhookConfig {
            port: 8080,
            secret: Some(SENTINEL.into()),
        });
        config.channels_config.lark = Some(crate::config::LarkConfig {
            app_id: "cli_app".into(),
            app_secret: SENTINEL.into(),
            encrypt_key: Some(SENTINEL.into()),
            verification_token: Some(SENTINEL.into()),
            allowed_users: vec![],
            use_feishu: false,
            receive_mode: crate::config::schema::LarkReceiveMode::default(),
            port: None,
        });
        config.channels_config.irc = Some(crate::config::IrcConfig {
            server: "irc.example.com".into(),
            port: 6697,
            nickname: "zeroclaw_bot".into(),
            username: None,
            channels: vec![],
            allowed_users: vec![],
            server_password: Some(SENTINEL.into()),
            nickserv_password: Some(SENTINEL.into()),
            sasl_password: Some(SENTINEL.into()),
            verify_tls: None,
        });

        let toml = toml::to_string_pretty(&config).expect("config serializes to TOML");
        assert!(
            toml.contains(SENTINEL),
            "test setup sanity: serialized TOML should contain the sentinels before masking"
        );

        let masked = mask_sensitive_fields(&toml);
        assert!(
            !masked.contains(SENTINEL),
            "a secret value survived masking:\n{masked}"
        );
    }
}
