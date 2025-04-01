use reqwest::Client;
use std::env;

/// Uploads content to Pastebin and returns the resulting URL
pub async fn upload_to_pastebin(
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
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
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

    // Replace standard URL with raw URL
    let raw_url = url.replace("https://pastebin.com/", "https://pastebin.com/raw/");

    Ok(raw_url)
}
