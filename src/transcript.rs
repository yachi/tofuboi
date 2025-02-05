use ytranscript::{TranscriptConfig, YoutubeTranscript, YoutubeTranscriptError};

pub struct TranscriptService;

impl TranscriptService {
    pub async fn fetch(
        video_id: &str,
        lang: &str,
    ) -> Result<(Vec<ytranscript::TranscriptResponse>, Option<String>), YoutubeTranscriptError> {
        let config = TranscriptConfig {
            lang: Some(lang.to_string()),
        };

        match YoutubeTranscript::fetch_transcript(video_id, Some(config)).await {
            Ok(transcript) => Ok((transcript, None)),
            Err(YoutubeTranscriptError::TranscriptNotAvailableLanguage(
                _,
                available_langs,
                video,
            )) => {
                let fallback_lang = Self::select_fallback_language(&available_langs, &["en", "zh-HK", "zh-TW"]);
                let new_config = TranscriptConfig {
                    lang: Some(fallback_lang.clone()),
                };
                let transcript = YoutubeTranscript::fetch_transcript(&video, Some(new_config)).await?;
                Ok((transcript, Some(format!("Requested language '{}' not available. Using fallback language '{}'. Available languages: {}", 
                    lang, fallback_lang, available_langs.join(", ")))))
            }
            Err(e) => Err(e),
        }
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_fallback_language() {
        let available = vec!["en".to_string(), "es".to_string(), "zh-HK".to_string()];
        assert_eq!(
            TranscriptService::select_fallback_language(&available, &["fr", "en", "es"]),
            "en"
        );

        let available = vec!["es".to_string(), "zh-HK".to_string()];
        assert_eq!(
            TranscriptService::select_fallback_language(&available, &["fr", "en", "zh-HK"]),
            "zh-HK"
        );

        let available: Vec<String> = vec![];
        assert_eq!(
            TranscriptService::select_fallback_language(&available, &["fr", "en", "es"]),
            "en"
        );
    }
}
