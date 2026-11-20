//! Users' own provider keys ("bring your own key") and which key pays for a chat.

use crate::{
    provider::Provider,
    settings::{ProviderKind, Settings},
    ssrf::{self, Resolver},
};
use serde::{Deserialize, Serialize};

/// Whether users may (or must) use their own provider key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserKeyMode {
    /// Only the panel's key.
    Off,
    /// A user's own key when they set one, otherwise the panel's key.
    Allowed,
    /// Only users' own keys; the panel's key is never used for chats.
    Required,
}

/// Who pays for a chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    Panel,
    User,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Panel => "panel",
            Self::User => "user",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    Use(KeySource),
    /// The user has to add a key of their own first.
    NeedsUserKey,
    /// Nothing usable; the text says why.
    Unavailable(String),
}

pub const NEEDS_USER_KEY: &str =
    "Add your own AI provider key under Account → AI Assistant to use the assistant.";

/// Picks the key for a chat. `panel` is the panel's provider set-up, or why it is not set up.
pub fn choose(mode: UserKeyMode, has_user_key: bool, panel: Result<(), String>) -> Choice {
    match (mode, has_user_key, panel) {
        (UserKeyMode::Off, _, Ok(())) => Choice::Use(KeySource::Panel),
        (UserKeyMode::Off, _, Err(problem)) => Choice::Unavailable(problem),
        (UserKeyMode::Allowed | UserKeyMode::Required, true, _) => Choice::Use(KeySource::User),
        (UserKeyMode::Allowed, false, Ok(())) => Choice::Use(KeySource::Panel),
        (UserKeyMode::Allowed, false, Err(_)) | (UserKeyMode::Required, false, _) => {
            Choice::NeedsUserKey
        }
    }
}

/// A provider users can pick without typing a URL.
#[derive(Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ProviderKind,
    pub base_url: &'static str,
    pub default_model: &'static str,
}

pub const CUSTOM: &str = "custom";

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "openai",
        name: "OpenAI",
        kind: ProviderKind::Openai,
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
    },
    Preset {
        id: "anthropic",
        name: "Anthropic",
        kind: ProviderKind::Anthropic,
        base_url: "https://api.anthropic.com/v1",
        default_model: "claude-sonnet-4-5",
    },
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        kind: ProviderKind::Openai,
        base_url: "https://openrouter.ai/api/v1",
        default_model: "openai/gpt-4o-mini",
    },
    Preset {
        id: "groq",
        name: "Groq",
        kind: ProviderKind::Openai,
        base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.3-70b-versatile",
    },
    Preset {
        id: "mistral",
        name: "Mistral",
        kind: ProviderKind::Openai,
        base_url: "https://api.mistral.ai/v1",
        default_model: "mistral-small-latest",
    },
    Preset {
        id: "deepseek",
        name: "DeepSeek",
        kind: ProviderKind::Openai,
        base_url: "https://api.deepseek.com/v1",
        default_model: "deepseek-chat",
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.id == id)
}

/// A user's stored key, decrypted.
pub struct UserKey {
    pub preset: String,
    /// Only for the `custom` preset.
    pub base_url: Option<String>,
    pub model: String,
    pub api_key: String,
}

impl UserKey {
    /// The provider for this key. Custom URLs are checked against the SSRF rules and the request
    /// is pinned to the vetted addresses.
    pub async fn provider(
        &self,
        settings: &Settings,
        resolver: &dyn Resolver,
    ) -> Result<Provider, String> {
        if let Some(preset) = preset(&self.preset) {
            return Ok(Provider {
                kind: preset.kind,
                url: match preset.kind {
                    ProviderKind::Openai => format!("{}/chat/completions", preset.base_url),
                    ProviderKind::Anthropic => crate::provider::anthropic::URL.into(),
                },
                api_key: self.api_key.clone(),
                model: self.model.clone(),
                max_tokens: settings.max_output_tokens,
                pin: None,
            });
        }
        if self.preset != CUSTOM {
            return Err(format!(
                "Your key uses the provider \"{}\", which no longer exists. Pick another one under Account → AI Assistant.",
                self.preset
            ));
        }
        if !settings.allow_custom_base_urls {
            return Err("The administrator turned off custom provider URLs. Pick one of the listed providers under Account → AI Assistant.".into());
        }
        let base_url = self
            .base_url
            .as_deref()
            .ok_or("Your key has no base URL. Edit it under Account → AI Assistant.")?;
        let pin = ssrf::vet(base_url, resolver).await?;
        Ok(Provider {
            kind: ProviderKind::Openai,
            url: format!("{}/chat/completions", base_url.trim().trim_end_matches('/')),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            max_tokens: settings.max_output_tokens,
            pin: Some(pin),
        })
    }
}

/// Checks a key a user typed: at most 1024 characters, no spaces or line breaks.
pub fn valid_api_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 1024
        && !key.chars().any(|c| c.is_control() || c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    struct Private;

    #[async_trait::async_trait]
    impl Resolver for Private {
        async fn resolve(&self, _host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
            Ok(vec![SocketAddr::from(([10, 0, 0, 7], port))])
        }
    }

    struct Public;

    #[async_trait::async_trait]
    impl Resolver for Public {
        async fn resolve(&self, _host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
            Ok(vec![SocketAddr::from(([104, 18, 32, 47], port))])
        }
    }

    fn key(preset: &str, base_url: Option<&str>) -> UserKey {
        UserKey {
            preset: preset.into(),
            base_url: base_url.map(str::to_string),
            model: "m".into(),
            api_key: "sk-user".into(),
        }
    }

    #[test]
    fn the_mode_decides_which_key_pays() {
        let set_up = || Ok(());
        let missing = || Err("no panel key".to_string());

        assert_eq!(choose(UserKeyMode::Off, true, set_up()), Choice::Use(KeySource::Panel));
        assert_eq!(
            choose(UserKeyMode::Off, true, missing()),
            Choice::Unavailable("no panel key".into())
        );

        assert_eq!(choose(UserKeyMode::Allowed, true, set_up()), Choice::Use(KeySource::User));
        assert_eq!(choose(UserKeyMode::Allowed, false, set_up()), Choice::Use(KeySource::Panel));
        assert_eq!(choose(UserKeyMode::Allowed, false, missing()), Choice::NeedsUserKey);

        // Required never falls back to the panel's key, even when it is set up.
        assert_eq!(choose(UserKeyMode::Required, false, set_up()), Choice::NeedsUserKey);
        assert_eq!(choose(UserKeyMode::Required, true, missing()), Choice::Use(KeySource::User));
    }

    #[tokio::test]
    async fn presets_need_no_url_and_custom_urls_are_vetted() {
        let mut settings = Settings::default();
        let openrouter = key("openrouter", Some("http://127.0.0.1"))
            .provider(&settings, &Private)
            .await
            .unwrap();
        // The stored URL is ignored for presets.
        assert_eq!(openrouter.url, "https://openrouter.ai/api/v1/chat/completions");
        assert!(openrouter.pin.is_none());
        let anthropic = key("anthropic", None).provider(&settings, &Private).await.unwrap();
        assert_eq!(anthropic.kind, ProviderKind::Anthropic);

        let custom = key("custom", Some("https://llm.example.com/v1/"));
        let err = custom.provider(&settings, &Public).await.err().unwrap();
        assert!(err.contains("turned off"), "{err}");

        settings.allow_custom_base_urls = true;
        let provider = custom.provider(&settings, &Public).await.unwrap();
        assert_eq!(provider.url, "https://llm.example.com/v1/chat/completions");
        assert_eq!(provider.pin.unwrap().host, "llm.example.com");
        assert!(custom.provider(&settings, &Private).await.is_err());
        assert!(
            key("custom", Some("http://localhost:11434/v1"))
                .provider(&settings, &Public)
                .await
                .is_err()
        );
        assert!(key("gone", None).provider(&settings, &Public).await.is_err());
    }

    #[test]
    fn typed_keys_are_checked() {
        assert!(valid_api_key("sk-proj-abc_123"));
        assert!(!valid_api_key(""));
        assert!(!valid_api_key("sk two"));
        assert!(!valid_api_key(&"x".repeat(1025)));
    }
}
