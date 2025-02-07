mod formatter;
mod transcript;

use formatter::split_safe_utf8;
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

    const MAX_MESSAGE_SIZE: usize = 4096; // Telegram's message size limit
    let mut buffer = String::with_capacity(MAX_MESSAGE_SIZE);

    for entry in transcript {
        // Decode HTML entities and fix specific cases
        let text = decode_html_entities(&entry.text)
            .replace("&#39;", "'")
            .to_string();

        // Safely split text to ensure single chunks won't exceed MAX_MESSAGE_SIZE
        let chunks = match split_safe_utf8(&text, MAX_MESSAGE_SIZE) {
            Ok(chunks) => chunks,
            Err(e) => {
                bot.send_message(msg.chat.id, format!("Error processing transcript: {}", e))
                    .await?;
                return Ok(());
            }
        };

        for chunk in chunks {
            // Determine additional length, including a newline if buffer is not empty
            let additional_len = if buffer.is_empty() {
                chunk.len()
            } else {
                1 + chunk.len()
            };
            if buffer.len() + additional_len > MAX_MESSAGE_SIZE {
                // Flush the current buffer if appending the chunk would exceed Telegram's limit
                bot.send_message(msg.chat.id, &buffer).await?;
                buffer.clear();
            }
            if !buffer.is_empty() {
                buffer.push('\n');
            }
            buffer.push_str(chunk);
        }
    }

    // Send any remaining content in the buffer
    if !buffer.is_empty() {
        bot.send_message(msg.chat.id, &buffer).await?;
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
            .map(|m| m.text().unwrap_or_default().to_string().replace("\n", " "))
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
