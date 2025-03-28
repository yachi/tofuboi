mod transcript;

use html_escape::decode_html_entities;
use reqwest::Client;
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

/// Uploads content to Pastebin and returns the resulting URL
async fn upload_to_pastebin(
    content: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new();

    // Get API key from environment variable
    let api_key = match env::var("PASTEBIN_KEY") {
        Ok(key) => key,
        Err(_) => return Err("PASTEBIN_KEY environment variable not set".into()),
    };

    // Define a user agent, getting it from env var or using a default
    let user_agent = env::var("UPLOAD_USER_AGENT").unwrap_or_else(|_| "tofuboi/1.0".to_string());

    // Use mockito server URL in tests, otherwise use the real Pastebin URL
    #[cfg(test)]
    let upload_url = {
        use mockito;
        mockito::server_url()
    };

    #[cfg(not(test))]
    let upload_url = "https://pastebin.com/api/api_post.php".to_string();

    // Convert the content to a String to satisfy type requirements
    let content_string = content.to_string();
    let paste_option = "paste".to_string();

    // Send request to Pastebin with the required parameters
    let response = client
        .post(upload_url)
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&[
            ("api_dev_key", &api_key),
            ("api_paste_code", &content_string),
            ("api_option", &paste_option),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        // Get the status code and response body for the error message
        let status = response.status();
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read response body".to_string());
        return Err(format!(
            "Upload failed with status: {}, response: {}",
            status, error_body
        )
        .into());
    }

    // Get the URL from the response body
    let url = response.text().await?.trim().to_string();
    Ok(url)
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

    // Upload the transcript to Pastebin
    match upload_to_pastebin(&full_transcript).await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{mock, Matcher};
    use teloxide_tests::{MockBot, MockMessageText};

    #[tokio::test]
    async fn test_handle_message_happy_path() {
        // Setup mock for Pastebin
        let mock_url = "https://pastebin.com/abcdef123";

        // Set the expected user agent for the test environment
        let expected_user_agent = env::var("UPLOAD_USER_AGENT").unwrap_or_else(|_| "tofuboi/1.0".to_string());
        
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

        // Check if any message contains the expected transcript URL
        let transcript_message_found = messages
            .iter()
            .any(|msg| msg.contains("Transcript available at:") && msg.contains(mock_url));

        // This might fail if TranscriptService::fetch is not mocked correctly,
        // but the mocking of Pastebin upload is now fixed
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
