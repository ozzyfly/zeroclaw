use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use super::types::{FeedItem, SourceType};

/// Parse raw XML/Atom/RSS bytes into a list of `FeedItem`.
pub fn parse_feed(data: &[u8], source_type: SourceType) -> Result<Vec<FeedItem>> {
    let feed = feed_rs::parser::parse(data).context("failed to parse feed XML")?;

    let items: Vec<FeedItem> = feed
        .entries
        .into_iter()
        .map(|entry| {
            let guid = entry.id;

            let title = entry
                .title
                .map(|t| t.content)
                .unwrap_or_else(|| "(untitled)".to_string());

            let link = entry
                .links
                .first()
                .map_or(String::new(), |l| l.href.clone());

            let pub_date: Option<DateTime<Utc>> = entry
                .published
                .or(entry.updated)
                .map(|dt| dt.with_timezone(&Utc));

            let description = entry
                .summary
                .map(|s| s.content)
                .or_else(|| entry.content.and_then(|c| c.body.map(|b| b.to_string())));

            // Podcast enclosure: look for audio/* media type.
            let enclosure_url = entry.media.iter().flat_map(|m| &m.content).find_map(|c| {
                c.content_type
                    .as_ref()
                    .filter(|ct| ct.ty() == "audio")
                    .and_then(|_| c.url.as_ref().map(|u| u.to_string()))
            });

            // Duration from media metadata (seconds).
            let duration_secs = entry
                .media
                .iter()
                .flat_map(|m| &m.content)
                .find_map(|c| c.duration.map(|d| d.as_secs()));

            let _ = source_type; // available for future per-type logic

            FeedItem {
                guid,
                title,
                link,
                pub_date,
                description,
                enclosure_url,
                duration_secs,
            }
        })
        .collect();

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RSS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Test Feed</title>
    <item>
      <guid>item-1</guid>
      <title>First Post</title>
      <link>https://example.com/1</link>
      <pubDate>Mon, 01 Jan 2026 00:00:00 GMT</pubDate>
      <description>Hello world</description>
    </item>
    <item>
      <guid>item-2</guid>
      <title>Second Post</title>
      <link>https://example.com/2</link>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parse_simple_rss() {
        let items = parse_feed(SAMPLE_RSS, SourceType::Rss).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].guid, "item-1");
        assert_eq!(items[0].title, "First Post");
        assert!(items[0].pub_date.is_some());
        assert_eq!(items[1].guid, "item-2");
    }
}
