use teloxide::prelude::*; // Teloxide provides the fundamental traits and types for bot creation [6]
use ytranscript::YoutubeTranscript; // ytranscript offers an easy way to fetch YouTube transcripts [2]

#[tokio::main]
async fn main() {
    // Initialize logging
    pretty_env_logger::init();
    log::info!("Launching Telegram bot...");

    // Read the bot token from the TELOXIDE_TOKEN environment variable
    // For example, set it like: export TELOXIDE_TOKEN="123456789:ABC-123..."
    let bot = Bot::from_env();

    // Use 'repl' to respond to each incoming message
    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        // Attempt to parse the received text as a YouTube video ID
        if let Some(video_id) = msg.text() {
            // Fetch the transcript with no special config
            match YoutubeTranscript::fetch_transcript(video_id.trim(), None).await {
                Ok(transcript) => {
                    let mut result = String::new();
                    for entry in transcript {
                        result.push_str(&format!("{} ", entry.text));
                    }

                    // If there's no transcript text or it's empty, notify the user
                    if result.trim().is_empty() {
                        bot.send_message(
                            msg.chat.id,
                            "Transcript could not be retrieved or is empty.",
                        )
                        .await?;
                    } else {
                        // Split the transcript into 4096-byte chunks to avoid exceeding Telegram's limit
                        for chunk in result.as_bytes().chunks(4096) {
                            let text_chunk = String::from_utf8_lossy(chunk);
                            bot.send_message(msg.chat.id, text_chunk).await?;
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
    })
    .await;
}
