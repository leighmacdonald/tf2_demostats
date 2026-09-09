//! Transcription of extracted voice audio via an OpenAI API-compatible
//! speech-to-text server (e.g. [speaches](https://speaches.ai/), a
//! self-hostable server backed by faster-whisper).
//!
//! The client POSTs each speaker's `.opus` file to
//! `{base_url}/v1/audio/transcriptions` as multipart (`response_format:
//! verbose_json`) and parses the returned segments. No audio conversion is
//! needed: these servers decode Ogg Opus and resample internally.

use std::{io, path::Path, time::Duration};

use serde::Deserialize;

/// Default server base URL: a self-hosted speaches instance.
pub const DEFAULT_BASE_URL: &str = "http://localhost:8000/v1";

/// Default model: faster-whisper large-v3 in CTranslate2 format.
pub const DEFAULT_MODEL: &str = "Systran/faster-whisper-large-v3";

/// Connection settings for the transcription server.
#[derive(Debug, Clone)]
pub struct TranscribeConfig {
    /// Server base URL, e.g. `http://localhost:8000/v1`.
    pub base_url: String,
    /// Model ID as known by the server, e.g. `Systran/faster-whisper-large-v3`.
    pub model: String,
    /// Optional bearer token (only needed if the server enforces auth).
    pub api_key: Option<String>,
    /// ISO-639-1 language code, e.g. `en`. Empty = server auto-detects.
    pub language: String,
    /// Per-request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for TranscribeConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
            api_key: None,
            language: "en".into(),
            timeout_secs: 300,
        }
    }
}

/// One transcript segment from a `verbose_json` response.
#[derive(Debug, Clone, Deserialize)]
pub struct VerboseSegment {
    /// Segment index (missing on some servers — defaults to 0).
    #[serde(default)]
    pub id: u64,
    /// Start time in seconds, relative to the submitted audio file.
    pub start: f64,
    /// End time in seconds, relative to the submitted audio file.
    pub end: f64,
    /// Transcribed text.
    #[serde(default)]
    pub text: String,
}

/// Parsed `verbose_json` transcription response.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct VerboseTranscription {
    /// Detected (or requested) language.
    #[serde(default)]
    pub language: String,
    /// Audio duration in seconds as seen by the server.
    #[serde(default)]
    pub duration: f64,
    /// Full transcript text.
    #[serde(default)]
    pub text: String,
    /// Timestamped segments.
    #[serde(default)]
    pub segments: Vec<VerboseSegment>,
}

/// Client for an OpenAI-compatible transcription server.
#[derive(Debug, Clone)]
pub struct Transcriber {
    client: reqwest::Client,
    config: TranscribeConfig,
}

impl Transcriber {
    /// Build a client from `config`. Fails only on invalid timeout/TLS setup,
    /// not on server reachability (checked per request).
    pub fn new(config: TranscribeConfig) -> crate::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()?;
        Ok(Self { client, config })
    }

    /// Transcribe one audio file (e.g. a per-player `.opus`).
    pub async fn transcribe_file(
        &self,
        audio_path: &Path,
    ) -> crate::Result<VerboseTranscription> {
        let bytes = std::fs::read(audio_path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("cannot read {}: {e}", audio_path.display()),
            )
        })?;
        let file_name = audio_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unusable file name: {}", audio_path.display()),
                )
            })?
            .to_owned();

        let mut form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(bytes)
                    .file_name(file_name)
                    .mime_str("audio/ogg")
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?,
            )
            .text("model", self.config.model.clone())
            .text("response_format", "verbose_json");
        if !self.config.language.is_empty() {
            form = form.text("language", self.config.language.clone());
        }

        let url = format!(
            "{}/audio/transcriptions",
            self.config.base_url.trim_end_matches('/')
        );
        let mut request = self.client.post(&url).multipart(form);
        if let Some(key) = self.config.api_key.as_deref().filter(|k| !k.is_empty()) {
            request = request.bearer_auth(key);
        }

        let response = request.send().await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!(
                    "cannot reach transcription server at {url}: {e}. \
                     Is a speaches (or compatible) server running there?"
                ),
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body = body.chars().take(500).collect::<String>();
            return Err(io::Error::other(format!(
                "transcription server returned {status}: {body}"
            ))
            .into());
        }
        response.json::<VerboseTranscription>().await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid transcription response: {e}"),
            )
            .into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "language": "en",
        "duration": 6.54,
        "text": " hello world",
        "segments": [
            {"id": 1, "seek": 572, "start": 0.82, "end": 5.82, "text": " hello world",
             "tokens": [1, 2], "temperature": 0.0, "avg_logprob": -0.3,
             "compression_ratio": 1.2, "no_speech_prob": 0.01}
        ]
    }"#;

    #[test]
    fn verbose_json_fixture_parses() {
        let t: VerboseTranscription = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(t.language, "en");
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].id, 1);
        assert!((t.segments[0].start - 0.82).abs() < 1e-9);
        assert_eq!(t.segments[0].text.trim(), "hello world");
    }

    #[test]
    fn sparse_response_parses_with_defaults() {
        // Some servers omit fields; segments only need start/end.
        let t: VerboseTranscription =
            serde_json::from_str(r#"{"segments": [{"start": 0.0, "end": 1.5}]}"#).unwrap();
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].id, 0);
        assert!(t.language.is_empty());
    }

    #[test]
    fn config_defaults() {
        let c = TranscribeConfig::default();
        assert_eq!(c.base_url, "http://localhost:8000/v1");
        assert_eq!(c.model, "Systran/faster-whisper-large-v3");
        assert_eq!(c.language, "en");
        assert!(c.api_key.is_none());
    }
}
