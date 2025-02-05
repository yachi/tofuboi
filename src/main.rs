use html_escape::decode_html_entities;
use teloxide::{
    dispatching::{UpdateFilterExt, UpdateHandler},
    prelude::*,
};
use ytranscript::{TranscriptConfig, YoutubeTranscript};

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log::info!("Launching Telegram bot...");

    let bot = Bot::from_env();

    Dispatcher::builder(bot, handler_tree())
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

fn handler_tree() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    dptree::entry().branch(Update::filter_message().endpoint(handle_message))
}

async fn handle_message(bot: Bot, msg: Message) -> HandlerResult {
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
        bot.send_message(msg.chat.id, "Please provide a valid YouTube video ID.")
            .await?;
    }

    Ok(())
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

    const MAX_CHUNK_SIZE: usize = 4000; // Leave some margin for safety
    let mut current_chunk = String::with_capacity(MAX_CHUNK_SIZE);

    for entry in transcript {
        let text = decode_html_entities(&entry.text).replace("&#39;", "'");

        // Split long text into smaller chunks efficiently
        if text.len() > MAX_CHUNK_SIZE {
            // First send any accumulated text
            if !current_chunk.is_empty() {
                bot.send_message(msg.chat.id, &current_chunk).await?;
                current_chunk.clear();
            }

            // Then split and send the long text
            for chunk in text.as_bytes().chunks(MAX_CHUNK_SIZE) {
                if let Ok(chunk_str) = std::str::from_utf8(chunk) {
                    bot.send_message(msg.chat.id, chunk_str).await?;
                }
            }
            continue;
        }

        // If adding this entry would exceed chunk size, send current chunk
        if current_chunk.len() + text.len() + 1 > MAX_CHUNK_SIZE {
            bot.send_message(msg.chat.id, &current_chunk).await?;
            current_chunk.clear();
        }

        // Add the text to current chunk
        if !current_chunk.is_empty() {
            current_chunk.push(' ');
        }
        current_chunk.push_str(&text);
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
    use std::fs;
    use teloxide_tests::{MockBot, MockMessageText};

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

        // Test empty available languages list
        let available: Vec<String> = vec![];
        assert_eq!(
            select_fallback_language(&available, &["fr", "en", "es"]),
            "en"
        );
    }

    #[tokio::test]
    async fn test_handle_message_happy_path() {
        let video_id = "https://www.youtube.com/watch?v=HQoJMIgNdjo";
        let bot = MockBot::new(MockMessageText::new().text(video_id), handler_tree());

        bot.dispatch().await;

        let messages: Vec<String> = bot
            .get_responses()
            .sent_messages
            .iter()
            .map(|m| m.text().unwrap_or_default().to_string())
            .collect();

        let expected = fs::read_to_string("fixtures/transcript.txt")
            .expect("Failed to read transcript.txt fixture");
        assert_eq!(messages.join(" ").trim(), expected.trim());
    }

    #[tokio::test]
    async fn test_handle_invalid_video_id() {
        let invalid_id = "not_a_valid_video_id";
        let bot = MockBot::new(MockMessageText::new().text(invalid_id), handler_tree());

        bot.dispatch().await;

        let messages: Vec<String> = bot
            .get_responses()
            .sent_messages
            .iter()
            .map(|m| m.text().unwrap_or_default().to_string())
            .collect();

        assert!(!messages.is_empty());
        assert!(messages[0].contains("Error fetching transcript"));
    }

    #[tokio::test]
    async fn test_handle_empty_message() {
        let empty_message = "";
        let bot = MockBot::new(MockMessageText::new().text(empty_message), handler_tree());

        bot.dispatch().await;

        let messages: Vec<String> = bot
            .get_responses()
            .sent_messages
            .iter()
            .map(|m| m.text().unwrap_or_default().to_string())
            .collect();

        assert!(!messages.is_empty());
        assert_eq!(messages[0], "Please provide a video ID.");
    }
}
