use html_escape::decode_html_entities;
use teloxide::prelude::*;
use ytranscript::{TranscriptConfig, YoutubeTranscript};

#[tokio::main]
async fn main() {
    // Initialize logging
    pretty_env_logger::init();
    log::info!("Launching Telegram bot...");

    // Read the bot token from the TELOXIDE_TOKEN environment variable
    let bot = Bot::from_env();

    // Define the message handler
    let message_handler = move |bot: Bot, msg: Message| async move {
        // Expect the message to contain the YouTube video ID and an optional language code.
        // Example: "dQw4w9WgXcQ en" or just "dQw4w9WgXcQ"
        if let Some(text) = msg.text() {
            let parts: Vec<&str> = text.split_whitespace().collect();
            if parts.is_empty() {
                bot.send_message(msg.chat.id, "Please provide a video ID.")
                    .await?;
                return Ok(());
            }
            let video_id = parts[0].trim();
            let requested_lang = if parts.len() > 1 { parts[1] } else { "en" };

            let config = TranscriptConfig {
                lang: Some(requested_lang.to_string()),
            };

            match YoutubeTranscript::fetch_transcript(video_id, Some(config)).await {
                Ok(transcript) => {
                    send_transcript(&bot, &msg, transcript).await?;
                }
                Err(ytranscript::YoutubeTranscriptError::TranscriptNotAvailableLanguage(
                    _,
                    available_langs,
                    _video,
                )) => {
                    // Refactored fallback language selection:
                    let fallback_lang =
                        select_fallback_language(&available_langs, &["en", "zh-HK", "zh-TW"]);
                    let available_langs_str = available_langs.join(", ");
                    let info = format!(
                        "Requested language '{}' not available. Retrying with fallback language '{}'. Available languages: {}",
                        requested_lang, fallback_lang, available_langs_str,
                    );
                    bot.send_message(msg.chat.id, info).await?;
                    let new_config = TranscriptConfig {
                        lang: Some(fallback_lang),
                    };
                    match YoutubeTranscript::fetch_transcript(video_id, Some(new_config)).await {
                        Ok(transcript) => {
                            send_transcript(&bot, &msg, transcript).await?;
                        }
                        Err(e) => {
                            bot.send_message(
                                msg.chat.id,
                                format!("Error fetching transcript: {}", e),
                            )
                            .await?;
                        }
                    }
                }
                Err(e) => {
                    bot.send_message(msg.chat.id, format!("Error fetching transcript: {}", e))
                        .await?;
                }
            }
        } else {
            bot.send_message(msg.chat.id, "Please provide a valid YouTube video ID.")
                .await?;
        }

        Ok(())
    };

    // Use 'repl' to respond to each incoming message
    teloxide::repl(bot, message_handler).await;
}

/// Selects a fallback language from the list of available languages.
/// It first checks a preferred order (passed in via `preferred`). If none of the preferred codes
/// are available, it then picks the first language starting with "zh", or falls back to the first available language.
fn select_fallback_language(available_langs: &[String], preferred: &[&str]) -> String {
    for &lang in preferred {
        if available_langs.contains(&lang.to_string()) {
            return lang.to_string();
        }
    }
    if let Some(lang) = available_langs.iter().find(|l| l.starts_with("zh")) {
        return lang.clone();
    }
    available_langs
        .first()
        .cloned()
        .unwrap_or_else(|| "en".to_string())
}

/// Helper function to send transcript text in chunks.
/// Streams the transcript entries directly without accumulating the entire text first.
async fn send_transcript(
    bot: &Bot,
    msg: &Message,
    transcript: Vec<ytranscript::TranscriptResponse>,
) -> Result<(), teloxide::RequestError> {
    if transcript.is_empty() {
        bot.send_message(
            msg.chat.id,
            "Transcript could not be retrieved or is empty.",
        )
        .await?;
        return Ok(());
    }

    let mut current_chunk = String::with_capacity(4096);

    for entry in transcript {
        let text = decode_html_entities(&entry.text).replace("&#39;", "'");

        // If adding this entry would exceed chunk size, send current chunk first
        if current_chunk.len() + text.len() + 1 > 4096 && !current_chunk.is_empty() {
            bot.send_message(msg.chat.id, &current_chunk).await?;
            current_chunk.clear();
        }

        // Handle case where single entry is longer than 4096
        if text.len() > 4096 {
            for chunk in text.chars().collect::<Vec<char>>().chunks(4096) {
                let chunk_str: String = chunk.iter().collect();
                bot.send_message(msg.chat.id, chunk_str).await?;
            }
        } else {
            current_chunk.push_str(&text);
            current_chunk.push(' ');
        }
    }

    // Send any remaining text
    if !current_chunk.is_empty() {
        bot.send_message(msg.chat.id, &current_chunk).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_fallback_language() {
        let available = vec!["en".to_string(), "es".to_string(), "zh-HK".to_string()];
        assert_eq!(
            select_fallback_language(&available, &["fr", "en", "es"]),
            "en"
        );

        let available = vec!["es".to_string(), "zh-HK".to_string()];
        assert_eq!(
            select_fallback_language(&available, &["fr", "en", "zh-HK"]),
            "zh-HK"
        );
    }
}
