mod transcript;

use html_escape::decode_html_entities;
use teloxide::{
    dispatching::{UpdateFilterExt, UpdateHandler},
    prelude::*,
};
use transcript::TranscriptService;

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
    let text = match msg.text() {
        Some(text) => text,
        None => {
            bot.send_message(msg.chat.id, "Please provide a valid YouTube video ID.")
                .await?;
            return Ok(());
        }
    };

    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.is_empty() {
        bot.send_message(msg.chat.id, "Please provide a video ID.")
            .await?;
        return Ok(());
    }

    let video_id = parts[0].trim();
    let requested_lang = parts.get(1).copied().unwrap_or("en");

    match TranscriptService::fetch(video_id, requested_lang).await {
        Ok((transcript, info)) => {
            if let Some(info) = info {
                bot.send_message(msg.chat.id, info).await?;
            }
            send_transcript(&bot, &msg, transcript).await?;
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("Error fetching transcript: {}", e))
                .await?;
        }
    }

    Ok(())
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
