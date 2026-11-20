//! Extension settings. API keys are stored encrypted with the panel's database key, live only in
//! the backend and leave it masked ([`AdminView`]); nothing here is ever sent to the browser in
//! full.

use crate::provider::Provider;
use serde::{Deserialize, Serialize};
use shared::{
    State,
    extensions::settings::{
        ExtensionSettings, SettingsDeserializeExt, SettingsDeserializer, SettingsSerializeExt,
        SettingsSerializer,
    },
};

pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Any OpenAI-compatible chat completions API: OpenAI, OpenRouter, Groq, Ollama, LM Studio, ...
    Openai,
    Anthropic,
}

#[derive(Clone)]
pub struct Settings {
    pub provider: ProviderKind,
    pub openai_base_url: String,
    pub openai_api_key: String,
    pub openai_model: String,
    pub anthropic_api_key: String,
    pub anthropic_model: String,
    /// Server users get the chat on their servers.
    pub server_chat_enabled: bool,
    /// Users with the `ai-assistant.chat` admin permission get the chat in the admin area.
    pub admin_chat_enabled: bool,
    /// Messages one user may send per hour; 0 means no limit.
    pub requests_per_hour: u32,
    /// Tokens (input plus output) one user may use per day; 0 means no limit.
    pub daily_token_limit: u64,
    /// Model calls per message before the assistant has to stop and report.
    pub max_tool_steps: u32,
    /// The longest answer the model may give in one call.
    pub max_output_tokens: u32,
    /// Tool output (console, files, API responses) is cut to this many characters.
    pub tool_output_limit: u32,
    /// How many recent chat entries are sent to the model.
    pub history_limit: u32,
    /// Conversations untouched for this many days are deleted; 0 keeps them.
    pub retention_days: u32,
    /// Extra instructions appended to the system prompt.
    pub instructions: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Openai,
            openai_base_url: DEFAULT_OPENAI_BASE_URL.into(),
            openai_api_key: String::new(),
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            anthropic_api_key: String::new(),
            anthropic_model: DEFAULT_ANTHROPIC_MODEL.into(),
            server_chat_enabled: true,
            admin_chat_enabled: true,
            requests_per_hour: 30,
            daily_token_limit: 500_000,
            max_tool_steps: 8,
            max_output_tokens: 4096,
            tool_output_limit: 12_000,
            history_limit: 60,
            retention_days: 30,
            instructions: String::new(),
        }
    }
}

impl Settings {
    /// The configured provider, or why the assistant cannot answer yet.
    pub fn provider(&self) -> Result<Provider, String> {
        match self.provider {
            ProviderKind::Openai => {
                if self.openai_base_url.trim().is_empty() || self.openai_model.trim().is_empty() {
                    return Err(
                        "The administrator has not set up the AI provider (base URL and model) yet."
                            .into(),
                    );
                }
                Ok(Provider {
                    kind: ProviderKind::Openai,
                    url: format!(
                        "{}/chat/completions",
                        self.openai_base_url.trim().trim_end_matches('/')
                    ),
                    api_key: self.openai_api_key.clone(),
                    model: self.openai_model.trim().to_string(),
                    max_tokens: self.max_output_tokens,
                })
            }
            ProviderKind::Anthropic => {
                if self.anthropic_api_key.is_empty() || self.anthropic_model.trim().is_empty() {
                    return Err(
                        "The administrator has not set up the Anthropic API key and model yet."
                            .into(),
                    );
                }
                Ok(Provider {
                    kind: ProviderKind::Anthropic,
                    url: crate::provider::anthropic::URL.into(),
                    api_key: self.anthropic_api_key.clone(),
                    model: self.anthropic_model.trim().to_string(),
                    max_tokens: self.max_output_tokens,
                })
            }
        }
    }

    pub fn model(&self) -> &str {
        match self.provider {
            ProviderKind::Openai => &self.openai_model,
            ProviderKind::Anthropic => &self.anthropic_model,
        }
    }

    pub fn admin_view(&self) -> AdminView {
        AdminView {
            provider: self.provider,
            openai_base_url: self.openai_base_url.clone(),
            openai_api_key: SecretView::of(&self.openai_api_key),
            openai_model: self.openai_model.clone(),
            anthropic_api_key: SecretView::of(&self.anthropic_api_key),
            anthropic_model: self.anthropic_model.clone(),
            server_chat_enabled: self.server_chat_enabled,
            admin_chat_enabled: self.admin_chat_enabled,
            requests_per_hour: self.requests_per_hour,
            daily_token_limit: self.daily_token_limit,
            max_tool_steps: self.max_tool_steps,
            max_output_tokens: self.max_output_tokens,
            tool_output_limit: self.tool_output_limit,
            history_limit: self.history_limit,
            retention_days: self.retention_days,
            instructions: self.instructions.clone(),
        }
    }
}

/// Whether a key is stored, and its last four characters for long keys. Never the key itself.
#[derive(Serialize, Debug, PartialEq)]
pub struct SecretView {
    pub set: bool,
    pub hint: Option<String>,
}

impl SecretView {
    pub fn of(secret: &str) -> Self {
        let chars: Vec<char> = secret.chars().collect();
        Self {
            set: !secret.is_empty(),
            hint: (chars.len() >= 16).then(|| {
                let tail: String = chars[chars.len() - 4..].iter().collect();
                format!("…{tail}")
            }),
        }
    }
}

#[derive(Serialize)]
pub struct AdminView {
    pub provider: ProviderKind,
    pub openai_base_url: String,
    pub openai_api_key: SecretView,
    pub openai_model: String,
    pub anthropic_api_key: SecretView,
    pub anthropic_model: String,
    pub server_chat_enabled: bool,
    pub admin_chat_enabled: bool,
    pub requests_per_hour: u32,
    pub daily_token_limit: u64,
    pub max_tool_steps: u32,
    pub max_output_tokens: u32,
    pub tool_output_limit: u32,
    pub history_limit: u32,
    pub retention_days: u32,
    pub instructions: String,
}

/// A settings change from the admin page. A key that is left out (or `null`) stays as it is,
/// an empty string removes it.
#[derive(Deserialize)]
pub struct Update {
    pub provider: ProviderKind,
    pub openai_base_url: String,
    #[serde(default)]
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    #[serde(default)]
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub server_chat_enabled: bool,
    pub admin_chat_enabled: bool,
    pub requests_per_hour: u32,
    pub daily_token_limit: u64,
    pub max_tool_steps: u32,
    pub max_output_tokens: u32,
    pub tool_output_limit: u32,
    pub history_limit: u32,
    pub retention_days: u32,
    #[serde(default)]
    pub instructions: String,
}

fn check_range<T: PartialOrd + std::fmt::Display>(
    errors: &mut Vec<String>,
    name: &str,
    value: T,
    min: T,
    max: T,
) {
    if value < min || value > max {
        errors.push(format!("{name} must be between {min} and {max}"));
    }
}

fn check_key(errors: &mut Vec<String>, name: &str, key: Option<&str>) {
    if let Some(key) = key
        && (key.len() > 1024 || key.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        errors.push(format!(
            "{name} must be at most 1024 characters without spaces or line breaks"
        ));
    }
}

impl Update {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        let base_url = self.openai_base_url.trim();
        if base_url.is_empty() {
            if self.provider == ProviderKind::Openai {
                errors.push("openai_base_url is required for an OpenAI-compatible provider".into());
            }
        } else {
            match url::Url::parse(base_url) {
                Ok(url) if matches!(url.scheme(), "http" | "https") && url.host().is_some() => {}
                _ => errors.push("openai_base_url must be an http(s) URL".into()),
            }
        }
        for (name, model) in [
            ("openai_model", &self.openai_model),
            ("anthropic_model", &self.anthropic_model),
        ] {
            if model.trim().chars().count() > 255 || model.chars().any(char::is_control) {
                errors.push(format!("{name} must be at most 255 characters"));
            }
        }
        match self.provider {
            ProviderKind::Openai if self.openai_model.trim().is_empty() => {
                errors.push("openai_model is required for an OpenAI-compatible provider".into())
            }
            ProviderKind::Anthropic if self.anthropic_model.trim().is_empty() => {
                errors.push("anthropic_model is required for Anthropic".into())
            }
            _ => {}
        }
        check_key(&mut errors, "openai_api_key", self.openai_api_key.as_deref());
        check_key(
            &mut errors,
            "anthropic_api_key",
            self.anthropic_api_key.as_deref(),
        );

        check_range(&mut errors, "requests_per_hour", self.requests_per_hour, 0, 10_000);
        check_range(
            &mut errors,
            "daily_token_limit",
            self.daily_token_limit,
            0,
            1_000_000_000,
        );
        check_range(&mut errors, "max_tool_steps", self.max_tool_steps, 1, 25);
        check_range(
            &mut errors,
            "max_output_tokens",
            self.max_output_tokens,
            256,
            64_000,
        );
        check_range(
            &mut errors,
            "tool_output_limit",
            self.tool_output_limit,
            1_000,
            100_000,
        );
        check_range(&mut errors, "history_limit", self.history_limit, 4, 200);
        check_range(&mut errors, "retention_days", self.retention_days, 0, 3650);
        if self.instructions.chars().count() > 8000 {
            errors.push("instructions must be at most 8000 characters".into());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn apply(self, settings: &mut Settings) {
        settings.provider = self.provider;
        settings.openai_base_url = self.openai_base_url.trim().to_string();
        if let Some(key) = self.openai_api_key {
            settings.openai_api_key = key;
        }
        settings.openai_model = self.openai_model.trim().to_string();
        if let Some(key) = self.anthropic_api_key {
            settings.anthropic_api_key = key;
        }
        settings.anthropic_model = self.anthropic_model.trim().to_string();
        settings.server_chat_enabled = self.server_chat_enabled;
        settings.admin_chat_enabled = self.admin_chat_enabled;
        settings.requests_per_hour = self.requests_per_hour;
        settings.daily_token_limit = self.daily_token_limit;
        settings.max_tool_steps = self.max_tool_steps;
        settings.max_output_tokens = self.max_output_tokens;
        settings.tool_output_limit = self.tool_output_limit;
        settings.history_limit = self.history_limit;
        settings.retention_days = self.retention_days;
        settings.instructions = self.instructions.trim().to_string();
    }
}

#[async_trait::async_trait]
impl SettingsSerializeExt for Settings {
    async fn serialize(
        &self,
        serializer: SettingsSerializer,
    ) -> Result<SettingsSerializer, anyhow::Error> {
        Ok(serializer
            .write_serde_setting("provider", &self.provider)?
            .write_serde_setting("openai_base_url", &self.openai_base_url)?
            .write_serde_setting("openai_model", &self.openai_model)?
            .write_serde_setting("anthropic_model", &self.anthropic_model)?
            .write_serde_setting("server_chat_enabled", &self.server_chat_enabled)?
            .write_serde_setting("admin_chat_enabled", &self.admin_chat_enabled)?
            .write_serde_setting("requests_per_hour", &self.requests_per_hour)?
            .write_serde_setting("daily_token_limit", &self.daily_token_limit)?
            .write_serde_setting("max_tool_steps", &self.max_tool_steps)?
            .write_serde_setting("max_output_tokens", &self.max_output_tokens)?
            .write_serde_setting("tool_output_limit", &self.tool_output_limit)?
            .write_serde_setting("history_limit", &self.history_limit)?
            .write_serde_setting("retention_days", &self.retention_days)?
            .write_serde_setting("instructions", &self.instructions)?
            .write_raw_encrypted_setting("openai_api_key", self.openai_api_key.as_str())
            .await?
            .write_raw_encrypted_setting("anthropic_api_key", self.anthropic_api_key.as_str())
            .await?)
    }
}

pub struct SettingsDataDeserializer;

#[async_trait::async_trait]
impl SettingsDeserializeExt for SettingsDataDeserializer {
    async fn deserialize_boxed(
        &self,
        deserializer: SettingsDeserializer<'_>,
    ) -> Result<ExtensionSettings, anyhow::Error> {
        let d = Settings::default();
        macro_rules! serde_or {
            ($key:literal, $default:expr) => {
                deserializer.read_serde_setting($key).unwrap_or($default)
            };
        }
        // A key that cannot be decrypted (for example after the panel's encryption key changed)
        // reads as unset instead of breaking every settings load.
        let openai_api_key = match deserializer
            .read_raw_encrypted_setting("openai_api_key")
            .await
        {
            Ok(Some(key)) => key.to_string(),
            _ => String::new(),
        };
        let anthropic_api_key = match deserializer
            .read_raw_encrypted_setting("anthropic_api_key")
            .await
        {
            Ok(Some(key)) => key.to_string(),
            _ => String::new(),
        };

        Ok(Box::new(Settings {
            provider: serde_or!("provider", d.provider),
            openai_base_url: serde_or!("openai_base_url", d.openai_base_url),
            openai_api_key,
            openai_model: serde_or!("openai_model", d.openai_model),
            anthropic_api_key,
            anthropic_model: serde_or!("anthropic_model", d.anthropic_model),
            server_chat_enabled: serde_or!("server_chat_enabled", d.server_chat_enabled),
            admin_chat_enabled: serde_or!("admin_chat_enabled", d.admin_chat_enabled),
            requests_per_hour: serde_or!("requests_per_hour", d.requests_per_hour),
            daily_token_limit: serde_or!("daily_token_limit", d.daily_token_limit),
            max_tool_steps: serde_or!("max_tool_steps", d.max_tool_steps),
            max_output_tokens: serde_or!("max_output_tokens", d.max_output_tokens),
            tool_output_limit: serde_or!("tool_output_limit", d.tool_output_limit),
            history_limit: serde_or!("history_limit", d.history_limit),
            retention_days: serde_or!("retention_days", d.retention_days),
            instructions: serde_or!("instructions", d.instructions),
        }))
    }
}

pub async fn load(state: &State) -> Result<Settings, anyhow::Error> {
    let settings = state.settings.get().await?;
    Ok(settings
        .get_extension_settings::<Settings>(crate::PACKAGE)
        .cloned()
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update() -> Update {
        Update {
            provider: ProviderKind::Openai,
            openai_base_url: "https://openrouter.ai/api/v1/".into(),
            openai_api_key: None,
            openai_model: " openai/gpt-4o ".into(),
            anthropic_api_key: None,
            anthropic_model: DEFAULT_ANTHROPIC_MODEL.into(),
            server_chat_enabled: true,
            admin_chat_enabled: false,
            requests_per_hour: 10,
            daily_token_limit: 1000,
            max_tool_steps: 5,
            max_output_tokens: 2048,
            tool_output_limit: 8000,
            history_limit: 20,
            retention_days: 7,
            instructions: String::new(),
        }
    }

    #[test]
    fn keys_are_masked_to_their_last_four_characters() {
        assert_eq!(
            SecretView::of(""),
            SecretView {
                set: false,
                hint: None
            }
        );
        assert_eq!(
            SecretView::of("short-key"),
            SecretView {
                set: true,
                hint: None
            }
        );
        let view = SecretView::of("sk-proj-1234567890abcdefWXYZ");
        assert_eq!(view.hint.as_deref(), Some("…WXYZ"));
    }

    #[test]
    fn the_admin_view_never_contains_a_key() {
        let settings = Settings {
            openai_api_key: "sk-secret-openai-key-0000".into(),
            anthropic_api_key: "sk-ant-secret-key-1111".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings.admin_view()).unwrap();
        assert!(!json.contains("sk-secret-openai-key"), "{json}");
        assert!(!json.contains("sk-ant-secret-key"), "{json}");
        assert!(json.contains("\"set\":true"), "{json}");
    }

    #[test]
    fn leaving_a_key_out_keeps_it_and_an_empty_string_clears_it() {
        let mut settings = Settings {
            openai_api_key: "old-key".into(),
            anthropic_api_key: "old-anthropic".into(),
            ..Default::default()
        };
        let mut change = update();
        change.anthropic_api_key = Some(String::new());
        assert!(change.validate().is_ok());
        change.apply(&mut settings);

        assert_eq!(settings.openai_api_key, "old-key");
        assert_eq!(settings.anthropic_api_key, "");
        assert_eq!(settings.openai_model, "openai/gpt-4o");
        let provider = settings.provider().unwrap();
        assert_eq!(provider.url, "https://openrouter.ai/api/v1/chat/completions");
    }

    #[test]
    fn invalid_settings_are_refused() {
        let mut change = update();
        change.openai_base_url = "file:///etc/passwd".into();
        change.max_tool_steps = 0;
        change.openai_api_key = Some("two words".into());
        let errors = change.validate().unwrap_err();
        assert_eq!(errors.len(), 3, "{errors:?}");
    }

    #[test]
    fn anthropic_needs_a_key_before_it_counts_as_configured() {
        let settings = Settings {
            provider: ProviderKind::Anthropic,
            ..Default::default()
        };
        assert!(settings.provider().is_err());
        let settings = Settings {
            anthropic_api_key: "sk-ant".into(),
            ..settings
        };
        assert_eq!(settings.provider().unwrap().kind, ProviderKind::Anthropic);
    }
}
