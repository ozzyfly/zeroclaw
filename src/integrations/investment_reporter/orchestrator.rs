use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use chrono_tz::Tz;
use tracing::{error, info, warn};

use crate::channels::traits::{Channel, SendMessage};
use crate::channels::whatsapp::WhatsAppChannel;
#[cfg(feature = "whatsapp-web")]
use crate::channels::whatsapp_web::WhatsAppWebChannel;
use crate::config::Config;
use crate::integrations::feed_processor::types::{
    FeedSourceConfig, InvestReporterConfig, SourceType,
};
use crate::integrations::investment_reporter::schemas::AnalysisResult;
use crate::integrations::transcription::assemblyai;
use crate::integrations::youtube;
use crate::integrations::youtube::transcript as yt_transcript;
use crate::providers::traits::Provider;

use super::recorder;

pub fn report_date_for_timezone(timezone: &str) -> String {
    let tz_name = timezone.trim();
    match tz_name.parse::<Tz>() {
        Ok(tz) => Utc::now()
            .with_timezone(&tz)
            .date_naive()
            .format("%Y-%m-%d")
            .to_string(),
        Err(err) => {
            warn!(
                timezone = tz_name,
                "Invalid invest_reporter timezone ({err}); falling back to UTC"
            );
            Utc::now().format("%Y-%m-%d").to_string()
        }
    }
}

/// Result of a single source processing step.
struct SourceContent {
    source_type: String,
    source_name: String,
    title: String,
    url: String,
    publish_time: Option<String>,
    transcript: String,
}

/// Run the full daily investment report pipeline:
///
/// 1. Fetch new items from RSS / podcast feeds
/// 2. Fetch recent YouTube videos
/// 3. Transcribe podcast audio (AssemblyAI) + YouTube transcripts (Apify)
/// 4. Analyze each piece of content with LLM
/// 5. Aggregate results
/// 6. Generate final report
/// 7. Send to WhatsApp (unless dry-run)
pub async fn run_daily_report(
    config: &Config,
    reporter_config: &InvestReporterConfig,
    provider: &dyn Provider,
    model: &str,
    temperature: f64,
    whatsapp_recipient: &str,
    dry_run: bool,
) -> Result<super::ReportOutput> {
    let since = Utc::now() - Duration::hours(48);
    let report_date = report_date_for_timezone(&reporter_config.timezone);

    // ── Step 1: Collect content from all sources ──────────────────────
    info!(
        "📡 Fetching content from {} feed(s) and {} YouTube source(s)",
        reporter_config.feeds.len(),
        reporter_config.youtube_channels.len(),
    );

    let mut contents: Vec<SourceContent> = Vec::new();

    // 1a. RSS / Podcast feeds
    for feed_src in &reporter_config.feeds {
        match fetch_feed_content(config, feed_src, reporter_config).await {
            Ok(mut items) => {
                // Limit to 5 most recent items per source to keep LLM costs manageable
                items.truncate(5);
                info!("  ✅ {} — {} new item(s)", feed_src.name, items.len());
                contents.append(&mut items);
            }
            Err(e) => {
                warn!("  ⚠️ {} — failed: {e:#}", feed_src.name);
            }
        }
    }

    // 1b. YouTube channels
    let youtube_api_key = std::env::var("YOUTUBE_API_KEY").ok();
    let apify_token = std::env::var("APIFY_TOKEN").ok();

    if let Some(ref api_key) = youtube_api_key {
        for channel_id in &reporter_config.youtube_channels {
            match fetch_youtube_content(api_key, apify_token.as_deref(), channel_id, since).await {
                Ok(mut items) => {
                    info!("  ✅ YouTube:{channel_id} — {} video(s)", items.len());
                    contents.append(&mut items);
                }
                Err(e) => {
                    warn!("  ⚠️ YouTube:{channel_id} — failed: {e:#}");
                }
            }
        }
    } else if !reporter_config.youtube_channels.is_empty() {
        warn!("YOUTUBE_API_KEY not set — skipping YouTube sources");
    }

    if contents.is_empty() {
        anyhow::bail!("No new content found from any source — skipping report generation");
    }
    info!("📝 Analyzing {} piece(s) of content …", contents.len());

    // ── Step 2: LLM analysis for each piece ──────────────────────────
    let mut analyses: Vec<AnalysisResult> = Vec::new();

    for content in &contents {
        match super::analyze_single_content(
            provider,
            model,
            temperature,
            &content.source_type,
            &content.source_name,
            &content.title,
            &content.url,
            content.publish_time.as_deref(),
            &content.transcript,
        )
        .await
        {
            Ok(result) => {
                info!("  ✅ Analyzed: {}", content.title);
                analyses.push(result);
            }
            Err(e) => {
                error!("  ❌ Analysis failed for '{}': {e:#}", content.title);
            }
        }
    }

    if analyses.is_empty() {
        anyhow::bail!("All content analysis failed — cannot generate report");
    }

    // ── Step 3: Aggregate ────────────────────────────────────────────
    info!("📊 Aggregating {} analysis result(s) …", analyses.len());
    let (aggregated_symbols, macro_data) = super::aggregate_analyses(&analyses);

    // ── Step 4: Generate report ──────────────────────────────────────
    info!("📰 Generating final report …");
    let report_output = super::generate_report(
        provider,
        model,
        temperature,
        &aggregated_symbols,
        &macro_data,
        &report_date,
        contents.len(),
    )
    .await
    .context("failed to generate report")?;

    // ── Step 5: Record report to database + file ────────────────────
    let json_report = serde_json::to_string_pretty(&report_output.report)
        .unwrap_or_else(|_| "(serialization failed)".to_string());

    let report_id = if let Ok(conn) = recorder::open_db(&config.workspace_dir) {
        match recorder::save_report(
            &conn,
            &report_date,
            &json_report,
            &report_output.whatsapp_summary,
            contents.len(),
            report_output.report.symbols.len(),
        ) {
            Ok(id) => {
                info!("💾 Report saved to database (id: {id})");
                Some(id)
            }
            Err(e) => {
                warn!("Failed to save report to database: {e:#}");
                None
            }
        }
    } else {
        None
    };

    // Save JSON file to workspace/reports/
    match recorder::save_report_file(&config.workspace_dir, &report_date, &json_report) {
        Ok(path) => info!("📁 Report file: {}", path.display()),
        Err(e) => warn!("Failed to save report file: {e:#}"),
    }

    // ── Step 6: Deliver via WhatsApp ─────────────────────────────────
    if dry_run {
        info!("🔕 Dry-run mode — skipping WhatsApp delivery");
    } else {
        info!("📲 Sending report to WhatsApp ({whatsapp_recipient}) …");
        match send_whatsapp_report(config, whatsapp_recipient, &report_output.whatsapp_summary)
            .await
        {
            Ok(()) => {
                info!("✅ Report delivered successfully");
                // Update delivery status
                if let (Some(ref rid), Ok(conn)) =
                    (&report_id, recorder::open_db(&config.workspace_dir))
                {
                    let _ = recorder::update_delivery_status(
                        &conn,
                        rid,
                        recorder::DeliveryStatus::Delivered,
                        None,
                    );
                }
            }
            Err(e) => {
                error!("❌ WhatsApp delivery failed: {e:#}");
                if let (Some(ref rid), Ok(conn)) =
                    (&report_id, recorder::open_db(&config.workspace_dir))
                {
                    let _ = recorder::update_delivery_status(
                        &conn,
                        rid,
                        recorder::DeliveryStatus::Failed,
                        Some(&format!("{e:#}")),
                    );
                }
                return Err(e);
            }
        }
    }

    Ok(report_output)
}

/// Fetch and transcribe content from an RSS or podcast feed.
async fn fetch_feed_content(
    config: &Config,
    feed_src: &FeedSourceConfig,
    reporter_config: &InvestReporterConfig,
) -> Result<Vec<SourceContent>> {
    let sub = crate::integrations::feed_processor::subscribe(config, feed_src)?;
    let items = crate::integrations::feed_processor::fetch_new_items(config, &sub).await?;

    let assemblyai_key = std::env::var("ASSEMBLYAI_API_KEY").ok();
    let mut results = Vec::new();

    for item in &items {
        let transcript = match feed_src.source_type {
            SourceType::Podcast => {
                // Transcribe podcast audio via AssemblyAI
                if let Some(ref audio_url) = item.enclosure_url {
                    if let Some(ref api_key) = assemblyai_key {
                        let timeout =
                            std::time::Duration::from_secs(reporter_config.assemblyai_timeout_secs);
                        match assemblyai::transcribe_audio(api_key, audio_url, None, Some(timeout))
                            .await
                        {
                            Ok(text) => text,
                            Err(e) => {
                                warn!("Transcription failed for '{}': {e:#}", item.title);
                                // Fall back to description
                                item.description.clone().unwrap_or_default()
                            }
                        }
                    } else {
                        warn!(
                            "ASSEMBLYAI_API_KEY not set — using description for '{}'",
                            item.title
                        );
                        item.description.clone().unwrap_or_default()
                    }
                } else {
                    item.description.clone().unwrap_or_default()
                }
            }
            // RSS: use description as content
            _ => item.description.clone().unwrap_or_default(),
        };

        if transcript.is_empty() {
            continue;
        }

        results.push(SourceContent {
            source_type: feed_src.source_type.as_str().to_string(),
            source_name: feed_src.name.clone(),
            title: item.title.clone(),
            url: item.link.clone(),
            publish_time: item.pub_date.map(|d| d.to_rfc3339()),
            transcript,
        });

        // Mark processed only after a non-empty transcript is confirmed —
        // items with empty transcripts are NOT marked so they can be retried.
        if let Err(e) = crate::integrations::feed_processor::mark_item_processed(config, &sub, item)
        {
            warn!("Failed to mark item '{}' as processed: {e:#}", item.title);
        }
    }

    Ok(results)
}

/// Fetch recent YouTube videos and their transcripts.
async fn fetch_youtube_content(
    api_key: &str,
    apify_token: Option<&str>,
    channel_id: &str,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<SourceContent>> {
    let videos = youtube::search_recent_videos(api_key, channel_id, Some(since), 5).await?;

    let mut results = Vec::new();

    for video in &videos {
        let transcript = if let Some(token) = apify_token {
            match yt_transcript::fetch_transcript(token, &video.video_id).await {
                Ok(text) => text,
                Err(e) => {
                    warn!("YouTube transcript failed for '{}': {e:#}", video.title);
                    video.description.clone()
                }
            }
        } else {
            // No Apify token — fall back to description
            video.description.clone()
        };

        if transcript.is_empty() {
            continue;
        }

        results.push(SourceContent {
            source_type: "youtube".to_string(),
            source_name: video.channel_title.clone(),
            title: video.title.clone(),
            url: format!("https://www.youtube.com/watch?v={}", video.video_id),
            publish_time: Some(video.published_at.to_rfc3339()),
            transcript,
        });
    }

    Ok(results)
}

/// Send report text via WhatsApp (Cloud API or Web, auto-detected).
async fn send_whatsapp_report(config: &Config, recipient: &str, summary: &str) -> Result<()> {
    let wa_config = config
        .channels_config
        .whatsapp
        .as_ref()
        .context("WhatsApp not configured in config.toml")?;

    // WhatsApp message limit — split if needed
    let chunks = split_message(summary, 4000);
    let messages: Vec<SendMessage> = chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            if chunks.len() > 1 {
                SendMessage::new(
                    format!("📊 每日投資報告 ({}/{}) \n\n{chunk}", i + 1, chunks.len()),
                    recipient,
                )
            } else {
                SendMessage::new(chunk.as_str(), recipient)
            }
        })
        .collect();

    // Detect backend: session_path → Web mode, otherwise Cloud API
    #[cfg(feature = "whatsapp-web")]
    if let Some(ref session_path) = wa_config.session_path {
        info!("📲 Using WhatsApp Web to deliver report");
        let channel = WhatsAppWebChannel::new(
            session_path.clone(),
            wa_config.pair_phone.clone(),
            wa_config.pair_code.clone(),
            vec!["*".to_string()],
        );
        channel.send_oneshot(messages).await?;
        return Ok(());
    }
    {
        let access_token = wa_config
            .access_token
            .as_ref()
            .context("WhatsApp access_token not set")?;
        let phone_number_id = wa_config
            .phone_number_id
            .as_ref()
            .context("WhatsApp phone_number_id not set")?;

        let channel = WhatsAppChannel::new(
            access_token.clone(),
            phone_number_id.clone(),
            wa_config.verify_token.clone().unwrap_or_default(),
            vec!["*".to_string()],
        );

        for msg in &messages {
            channel
                .send(msg)
                .await
                .with_context(|| "failed to send WhatsApp Cloud API message")?;
        }
    }

    Ok(())
}

/// Split a message into chunks of at most `max_len` characters,
/// breaking at newline boundaries when possible.
fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        // Try to break at a newline within the limit
        let split_at = remaining[..max_len]
            .rfind('\n')
            .map(|pos| pos + 1) // include the newline in current chunk
            .unwrap_or(max_len);

        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..];
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_message_short_text() {
        let chunks = split_message("hello world", 100);
        assert_eq!(chunks, vec!["hello world"]);
    }

    #[test]
    fn split_message_breaks_at_newline() {
        let text = "line1\nline2\nline3\nline4";
        let chunks = split_message(text, 12);
        // "line1\nline2\n" = 12 chars exactly
        assert_eq!(chunks[0], "line1\nline2\n");
        assert_eq!(chunks[1], "line3\nline4");
    }

    #[test]
    fn split_message_no_newline_hard_break() {
        let text = "a".repeat(150);
        let chunks = split_message(&text, 100);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 50);
    }
}
