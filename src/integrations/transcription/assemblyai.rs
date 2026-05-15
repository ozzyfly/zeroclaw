use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;
use tracing::debug;

const API_BASE: &str = "https://api.assemblyai.com/v2";
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Transcribe a podcast audio URL via AssemblyAI.
///
/// Submits the audio URL, then polls until the transcript is ready (or until
/// `timeout` elapses). Returns the full transcript text.
pub async fn transcribe_audio(
    api_key: &str,
    audio_url: &str,
    language: Option<&str>,
    timeout: Option<Duration>,
) -> Result<String> {
    let timeout = timeout.unwrap_or(DEFAULT_TIMEOUT);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")?;

    // 1. Submit transcription job.
    let mut body = serde_json::json!({ "audio_url": audio_url });
    if let Some(lang) = language {
        body["language_code"] = serde_json::Value::String(lang.to_string());
    }

    let submit_resp: SubmitResponse = client
        .post(format!("{API_BASE}/transcript"))
        .header("Authorization", api_key)
        .json(&body)
        .send()
        .await
        .context("AssemblyAI submit request failed")?
        .error_for_status()
        .context("AssemblyAI submit returned error status")?
        .json()
        .await
        .context("failed to parse AssemblyAI submit response")?;

    let task_id = &submit_resp.id;
    debug!(task_id, "AssemblyAI transcription submitted");

    // 2. Poll for completion.
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        if tokio::time::Instant::now() > deadline {
            anyhow::bail!(
                "AssemblyAI transcription timed out after {}s for task {task_id}",
                timeout.as_secs()
            );
        }

        let poll_resp: PollResponse = client
            .get(format!("{API_BASE}/transcript/{task_id}"))
            .header("Authorization", api_key)
            .send()
            .await
            .with_context(|| format!("AssemblyAI poll failed for task {task_id}"))?
            .error_for_status()
            .with_context(|| format!("AssemblyAI poll returned error for task {task_id}"))?
            .json()
            .await
            .context("failed to parse AssemblyAI poll response")?;

        match poll_resp.status.as_str() {
            "completed" => {
                let text = poll_resp.text.unwrap_or_default();
                debug!(task_id, chars = text.len(), "transcription completed");
                return Ok(text);
            }
            "error" => {
                let err = poll_resp
                    .error
                    .unwrap_or_else(|| "unknown error".to_string());
                anyhow::bail!("AssemblyAI transcription failed for task {task_id}: {err}");
            }
            status => {
                debug!(task_id, status, "transcription still processing");
            }
        }
    }
}

// ── Response types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SubmitResponse {
    id: String,
}

#[derive(Deserialize)]
struct PollResponse {
    status: String,
    text: Option<String>,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_reasonable() {
        assert!(DEFAULT_TIMEOUT.as_secs() >= 60);
        assert!(POLL_INTERVAL.as_secs() >= 2);
    }
}
