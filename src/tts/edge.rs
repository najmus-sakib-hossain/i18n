//! Microsoft Edge TTS implementation

use crate::error::{I18nError, Result};
use crate::tts::base::{TTSConfig, TextToSpeech};
use crate::tts::constants::{DEFAULT_VOICE, EDGE_TTS_WSS_URL};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Voice {
    pub name: String,
    pub short_name: String,
    pub gender: String,
    pub locale: String,
}

/// Microsoft Edge TTS
pub struct EdgeTTS {
    config: TTSConfig,
}

impl EdgeTTS {
    /// Create a new Edge TTS instance
    ///
    /// # Arguments
    /// * `voice` - Voice name (e.g., "en-US-AriaNeural")
    ///
    /// # Example
    /// ```no_run
    /// use i18n::tts::EdgeTTS;
    ///
    /// let tts = EdgeTTS::new("en-US-AriaNeural");
    /// ```
    pub fn new(voice: &str) -> Self {
        Self {
            config: TTSConfig {
                voice: voice.to_string(),
                ..Default::default()
            },
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: TTSConfig) -> Self {
        Self { config }
    }

    fn create_ssml(&self, text: &str) -> String {
        let request_id = Uuid::new_v4().to_string().replace("-", "");
        let timestamp = chrono::Utc::now().format("%+").to_string();

        format!(
            "X-RequestId:{}\r\n\
             Content-Type:application/ssml+xml\r\n\
             X-Timestamp:{}Z\r\n\
             Path:ssml\r\n\r\n\
             <speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>\
             <voice name='{}'>\
             <prosody pitch='{}' rate='{}' volume='{}'>{}</prosody>\
             </voice>\
             </speak>",
            request_id,
            timestamp,
            self.config.voice,
            self.config.pitch,
            self.config.rate,
            self.config.volume,
            html_escape::encode_text(text)
        )
    }

    fn create_config_message(&self) -> String {
        let timestamp = chrono::Utc::now().format("%+").to_string();
        format!(
            "X-Timestamp:{}\r\n\
             Content-Type:application/json; charset=utf-8\r\n\
             Path:speech.config\r\n\r\n\
             {{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"}},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}}}\r\n",
            timestamp
        )
    }
}

#[async_trait]
impl TextToSpeech for EdgeTTS {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>> {
        let text = text.trim();
        if text.is_empty() {
            return Err(I18nError::Other("No text to speak".to_string()));
        }

        let connect_id = Uuid::new_v4().to_string().replace("-", "");
        let url = format!("{}?ConnectionId={}", EDGE_TTS_WSS_URL, connect_id);

        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| I18nError::WebSocketError(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        // Send configuration
        write
            .send(Message::Text(self.create_config_message()))
            .await
            .map_err(|e| I18nError::WebSocketError(e.to_string()))?;

        // Send SSML
        write
            .send(Message::Text(self.create_ssml(text)))
            .await
            .map_err(|e| I18nError::WebSocketError(e.to_string()))?;

        let mut audio_data = Vec::new();
        let mut audio_received = false;

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if data.len() < 2 {
                        continue;
                    }

                    let header_length = u16::from_be_bytes([data[0], data[1]]) as usize;
                    if header_length + 2 <= data.len() {
                        let audio = &data[header_length + 2..];
                        if !audio.is_empty() {
                            audio_data.extend_from_slice(audio);
                            audio_received = true;
                        }
                    }
                }
                Ok(Message::Text(text)) => {
                    if text.contains("Path:turn.end") {
                        break;
                    }
                }
                Err(e) => {
                    return Err(I18nError::WebSocketError(e.to_string()));
                }
                _ => {}
            }
        }

        if !audio_received {
            return Err(I18nError::NoAudioReceived);
        }

        Ok(audio_data)
    }

    fn get_supported_languages(&self) -> Vec<&'static str> {
        vec![
            "en-US", "en-GB", "en-AU", "en-CA", "en-IN",
            "es-ES", "es-MX", "fr-FR", "fr-CA", "de-DE",
            "it-IT", "pt-BR", "pt-PT", "ru-RU", "ja-JP",
            "ko-KR", "zh-CN", "zh-TW", "ar-SA", "hi-IN",
        ]
    }
}
