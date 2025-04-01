mod pastebin; // Add module declaration
mod transcript;

use html_escape::decode_html_entities;
// reqwest::Client is no longer needed directly here
use std::env; // Import the env module
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


/// Helper function to upload transcript to Pastebin and send the link to the user.
/// Instead of sending the transcript directly, it uploads the text and sends the resulting URL.
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

    // Combine all transcript entries into a single string
    let mut full_transcript = String::new();

    for entry in transcript {
        // Decode HTML entities and fix specific cases
        let text = decode_html_entities(&entry.text)
            .replace("&#39;", "'")
            .to_string();

        if !full_transcript.is_empty() {
            full_transcript.push('\n');
        }
        full_transcript.push_str(&text);
    }

    // Upload the transcript to Pastebin using the new module
    match pastebin::upload_to_pastebin(&full_transcript).await {
        Ok(url) => {
            // Send only the link to the user
            bot.send_message(msg.chat.id, format!("Transcript available at: {}", url))
                .await?;
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("Error uploading transcript: {}", e))
                .await?;
        }
    }

    Ok(())
}

// Remove unused import
// use reqwest::Client;

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{mock, Matcher}; // mockito is still needed for setting up the mock server URL
    use teloxide_tests::{MockBot, MockMessageText};

    #[tokio::test]
    async fn test_handle_message_happy_path() {
        // Setup mock for Pastebin
        let mock_url = "https://pastebin.com/abcdef123";
        // The raw URL that will be sent to the user
        let raw_mock_url = "https://pastebin.com/raw/abcdef123";

        // Set the expected user agent for the test environment
        let expected_user_agent =
            env::var("UPLOAD_USER_AGENT").unwrap_or_else(|_| "tofuboi/1.0".to_string());

        // Set a test API key for the environment
        env::set_var("PASTEBIN_KEY", "test_api_key");

        // Create a mock that matches the form request to the Pastebin API
        let _m = mock("POST", "/")
            .match_header("user-agent", expected_user_agent.as_str())
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body(Matcher::Any) // Match any body since we're sending form data
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(mock_url)
            .create();

        let video_id = "https://www.youtube.com/watch?v=HQoJMIgNdjo";
        let bot = MockBot::new(MockMessageText::new().text(video_id), handler_tree());

        bot.dispatch().await;

        let messages: Vec<String> = bot
            .get_responses()
            .sent_messages
            .iter()
            .map(|m| m.text().unwrap_or_default().to_string())
            .collect();

        // Verify that at least one message was sent
        assert!(!messages.is_empty());

        // Check if any message contains the expected transcript URL (with /raw/ path)
        let transcript_message_found = messages
            .iter()
            .any(|msg| msg.contains("Transcript available at:") && msg.contains(raw_mock_url));

        assert!(transcript_message_found, "Expected to find a message with the transcript URL, but none was found. Messages: {:?}", messages);
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
