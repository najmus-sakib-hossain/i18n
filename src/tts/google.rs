//! Google TTS implementation

use crate::error::{I18nError, Result};
use crate::tts::base::TextToSpeech;
use crate::tts::constants::{GOOGLE_TTS_MAX_CHARS, GOOGLE_TTS_RPC, GOOGLE_TTS_URL};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use regex::Regex;
use reqwest::Client;
use serde_json::json;

/// Google Text-to-Speech
pub struct GoogleTTS {
    client: Client,
    lang: String,
    tld: String,
    slow: bool,
}

impl GoogleTTS {
    /// Create a new Google TTS instance
    ///
    /// # Arguments
    /// * `lang` - Language code (e.g., "en", "es", "fr")
    /// * `tld` - Top-level domain (default: "com")
    /// * `slow` - Speak slowly (default: false)
    ///
    /// # Example
    /// ```no_run
    /// use i18n::tts::GoogleTTS;
    ///
    /// let tts = GoogleTTS::new("en", "com", false);
    /// ```
    pub fn new(lang: &str, tld: &str, slow: bool) -> Self {
        Self {
            client: Client::new(),
            lang: lang.to_string(),
            tld: tld.to_string(),
            slow,
        }
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        let text = text.trim();
        
        if text.len() <= GOOGLE_TTS_MAX_CHARS {
            return vec![text.to_string()];
        }

        // Simple tokenization by sentence
        let mut tokens = Vec::new();
        let mut current = String::new();

        for sentence in text.split(&['.', '!', '?'][..]) {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }

            if current.len() + sentence.len() + 1 > GOOGLE_TTS_MAX_CHARS {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }

            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(sentence);
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        tokens
    }

    fn package_rpc(&self, text: &str) -> String {
        let speed = if self.slow { "true" } else { "null" };
        let parameter = json!([text, self.lang, speed, "null"]);
        let escaped_parameter = serde_json::to_string(&parameter).unwrap();

        let rpc = json!([[[GOOGLE_TTS_RPC, escaped_parameter, serde_json::Value::Null, "generic"]]]);
        let escaped_rpc = serde_json::to_string(&rpc).unwrap();

        format!("f.req={}&", urlencoding::encode(&escaped_rpc))
    }

    async fn synthesize_part(&self, text: &str) -> Result<Vec<u8>> {
        let url = format!("https://translate.google.{}/{}", self.tld, "_/TranslateWebserverUi/data/batchexecute");
        let data = self.package_rpc(text);

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded;charset=utf-8")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36")
            .header("Referer", "http://translate.google.com/")
            .body(data)
            .send()
            .await?;

        let text = response.text().await?;
        
        // Extract audio data from response
        let re = Regex::new(r#"jQ1olc","\\["(.*)\\"]"#).unwrap();
        if let Some(caps) = re.captures(&text) {
            if let Some(audio_b64) = caps.get(1) {
                let audio_bytes = general_purpose::STANDARD
                    .decode(audio_b64.as_str())
                    .map_err(|e| I18nError::Other(format!("Base64 decode error: {}", e)))?;
                return Ok(audio_bytes);
            }
        }

        Err(I18nError::NoAudioReceived)
    }
}

#[async_trait]
impl TextToSpeech for GoogleTTS {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>> {
        if text.trim().is_empty() {
            return Err(I18nError::Other("No text to speak".to_string()));
        }

        let parts = self.tokenize(text);
        let mut audio_data = Vec::new();

        for part in parts {
            let part_audio = self.synthesize_part(&part).await?;
            audio_data.extend(part_audio);
        }

        Ok(audio_data)
    }

    fn get_supported_languages(&self) -> Vec<&'static str> {
        vec![
            "en", "es", "fr", "de", "it", "pt", "ru", "ja", "ko", "zh",
            "ar", "hi", "nl", "pl", "tr", "sv", "da", "no", "fi", "cs",
        ]
    }
}
