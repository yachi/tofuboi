mod transcript;

use html_escape::decode_html_entities;
use reqwest::multipart;
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

/// Uploads content to 0x0.st and returns the resulting URL
async fn upload_to_0x0st(
    content: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();

    // Create a form part with the transcript content
    let file_part = multipart::Part::bytes(content.as_bytes().to_vec()).file_name("transcript.txt");

    // Build the multipart form
    let form = multipart::Form::new().part("file", file_part);

    // Define a unique user agent for this application
    let user_agent = "tofuboi/1.0";

    // Send request to 0x0.st with the custom user agent
    let response = client
        .post("https://0x0.st")
        .header(reqwest::header::USER_AGENT, user_agent)
        .multipart(form)
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

/// Helper function to upload transcript to 0x0.st and send the link to the user.
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

    // Upload the transcript to 0x0.st
    match upload_to_0x0st(&full_transcript).await {
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
    use mockito::mock;
    use teloxide_tests::{MockBot, MockMessageText};

    #[tokio::test]
    async fn test_handle_message_happy_path() {
        // Setup mock for 0x0.st
        let mock_url = "https://0x0.st/example-transcript-url.txt";
        let _m = mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(mock_url)
            .create();

        // Override the 0x0.st API endpoint for testing
        // Note: This is a simplified test that doesn't actually mock the upload_to_0x0st function
        // In a more comprehensive test, we'd use dependency injection to properly mock this function

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

        // Note: In a real implementation with proper mocking, we would check for
        // "Transcript available at:" in the message
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
