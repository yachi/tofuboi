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

    // Use 'repl' to respond to each incoming message
    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        // Expect the message to contain the YouTube video ID and an optional language code.
        // Example: "dQw4w9WgXcQ en" or just "dQw4w9WgXcQ"
        if let Some(text) = msg.text() {
            let parts: Vec<&str> = text.trim().split_whitespace().collect();
            if parts.is_empty() {
                bot.send_message(msg.chat.id, "Please provide a video ID.").await?;
                return Ok(());
            }
            let video_id = parts[0];
            let requested_lang = if parts.len() > 1 {
                parts[1]
            } else {
                "en"
            };

            let config = TranscriptConfig {
                lang: Some(requested_lang.to_string()),
            };

            // Try fetching transcript with the user's language choice
            match YoutubeTranscript::fetch_transcript(video_id.trim(), Some(config)).await {
                Ok(transcript) => {
                    send_transcript(&bot, &msg, transcript).await?;
                }
                Err(ytranscript::YoutubeTranscriptError::TranscriptNotAvailableLanguage(_, available_langs, _video)) => {
                    // Try en first, then zh-HK, zh-TW, then anything that starts with zh.
                    let fallback_lang = if available_langs.contains(&"en".to_string()) {
                        "en".to_string()
                    } else if available_langs.contains(&"zh-HK".to_string()) {
                        "zh-HK".to_string()
                    } else if available_langs.contains(&"zh-TW".to_string()) {
                        "zh-TW".to_string()
                    } else if let Some(lang) = available_langs.iter().find(|l| l.starts_with("zh")) {
                        lang.clone()
                    } else {
                        available_langs.get(0).cloned().unwrap_or_else(|| "en".to_string())
                    };

                    let available_langs_str = available_langs.join(", ");
                    let info = format!(
                        "Requested language '{}' not available. Retrying with fallback language '{}'. Available languages: {}",
                        requested_lang, fallback_lang, available_langs_str
                    );
                    bot.send_message(msg.chat.id, info).await?;
                    let new_config = TranscriptConfig {
                        lang: Some(fallback_lang),
                    };
                    match YoutubeTranscript::fetch_transcript(video_id.trim(), Some(new_config)).await {
                        Ok(transcript) => {
                            send_transcript(&bot, &msg, transcript).await?;
                        }
                        Err(e) => {
                            bot.send_message(msg.chat.id, format!("Error fetching transcript: {}", e))
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
            bot.send_message(msg.chat.id, "Please provide a valid YouTube video ID.").await?;
        }

        Ok(())
    })
    .await;
}

/// Helper function to send transcript text in chunks.
async fn send_transcript(
    bot: &Bot,
    msg: &Message,
    transcript: Vec<ytranscript::TranscriptResponse>,
) -> Result<(), teloxide::RequestError> {
    let mut result = String::new();
    for entry in transcript {
        result.push_str(&format!("{} ", entry.text));
    }

    // Decode any HTML entities in the transcript text
    let unescaped = decode_html_entities(&result).replace("&#39;", "'");

    if unescaped.trim().is_empty() {
        bot.send_message(
            msg.chat.id,
            "Transcript could not be retrieved or is empty.",
        )
        .await?;
    } else {
        // Split the transcript into 4096-byte chunks to avoid exceeding Telegram's limit
        for chunk in unescaped.as_bytes().chunks(4096) {
            let text_chunk = String::from_utf8_lossy(chunk);
            bot.send_message(msg.chat.id, text_chunk).await?;
        }
    }
    Ok(())
}
