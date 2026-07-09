use std::{
    collections::BTreeMap,
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::{fs::OpenOptionsExt, fs::PermissionsExt};

use anyhow::Context;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub gateway_token: String,
    pub default_provider: String,
    pub providers: BTreeMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub adapter: AdapterKind,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub model_context_windows: HashMap<String, u64>,
    #[serde(default)]
    pub model_input_modalities: HashMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_efforts: Option<Vec<String>>,
    #[serde(default)]
    pub model_reasoning_efforts: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub no_vision_models: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterKind {
    Anthropic,
    OpenAiChat,
    AzureOpenAi,
    Google,
    Cursor,
    Kiro,
}

impl std::fmt::Display for AdapterKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            AdapterKind::Anthropic => "anthropic",
            AdapterKind::OpenAiChat => "openai-chat",
            AdapterKind::AzureOpenAi => "azure-openai",
            AdapterKind::Google => "google",
            AdapterKind::Cursor => "cursor",
            AdapterKind::Kiro => "kiro",
        };
        f.write_str(value)
    }
}

impl Config {
    pub fn default_config() -> Self {
        let mut providers = BTreeMap::new();
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                adapter: AdapterKind::OpenAiChat,
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: Some("${OPENROUTER_API_KEY}".to_string()),
                default_model: Some("anthropic/claude-sonnet-4.5".to_string()),
                models: vec![
                    "anthropic/claude-sonnet-4.5".to_string(),
                    "openai/gpt-5".to_string(),
                    "google/gemini-2.5-pro".to_string(),
                ],
                context_window: None,
                model_context_windows: HashMap::new(),
                model_input_modalities: HashMap::new(),
                reasoning_efforts: None,
                model_reasoning_efforts: HashMap::new(),
                no_vision_models: Vec::new(),
            },
        );
        providers.insert(
            "anthropic-api".to_string(),
            ProviderConfig {
                adapter: AdapterKind::Anthropic,
                base_url: "https://api.anthropic.com".to_string(),
                api_key: Some("${ANTHROPIC_API_KEY_UPSTREAM}".to_string()),
                default_model: Some("claude-sonnet-4-5".to_string()),
                models: vec![
                    "claude-sonnet-4-5".to_string(),
                    "claude-opus-4-1".to_string(),
                ],
                context_window: Some(200_000),
                model_context_windows: HashMap::new(),
                model_input_modalities: HashMap::new(),
                reasoning_efforts: Some(vec![
                    "low".to_string(),
                    "medium".to_string(),
                    "high".to_string(),
                ]),
                model_reasoning_efforts: HashMap::new(),
                no_vision_models: Vec::new(),
            },
        );
        Self {
            host: "127.0.0.1".to_string(),
            port: 10110,
            gateway_token: format!("occ_{}", Uuid::new_v4().simple()),
            default_provider: "openrouter".to_string(),
            providers,
        }
    }

    pub fn load() -> anyhow::Result<Self> {
        let raw = fs::read_to_string(config_path()?).context("read config")?;
        serde_json::from_str(&raw).context("parse config")
    }

    pub fn load_or_create() -> anyhow::Result<Self> {
        match Self::load() {
            Ok(cfg) => Ok(cfg),
            Err(_) => {
                let cfg = Self::default_config();
                cfg.save()?;
                Ok(cfg)
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path()?;
        let tmp = path.with_extension("json.tmp");
        write_private_file(&tmp, &(serde_json::to_string_pretty(self)? + "\n"))
            .context("write temp config")?;
        fs::rename(&tmp, &path).context("replace config")?;
        set_private_permissions(&path).context("set config permissions")?;
        Ok(())
    }

    pub fn claude_model_ids(&self) -> Vec<String> {
        let mut models = Vec::new();
        if let Some(provider) = self.providers.get(&self.default_provider) {
            add_provider_model_ids(
                &mut models,
                &self.default_provider,
                provider.default_model.iter().chain(provider.models.iter()),
            );
        }
        for (provider_name, provider) in &self.providers {
            if provider_name == &self.default_provider {
                continue;
            }
            add_provider_model_ids(
                &mut models,
                provider_name,
                provider.default_model.iter().chain(provider.models.iter()),
            );
        }
        models
    }

    pub fn claude_model_env(&self) -> Vec<(String, String)> {
        let models = self.claude_model_ids();
        let Some(default_model) = models.first() else {
            return Vec::new();
        };
        let fallback = default_model.clone();
        let mut available = models.clone();
        let sonnet = take_index_or(&mut available, 0, &fallback);
        let opus = take_index_or(&mut available, 0, &fallback);
        let haiku = take_preferred_fast_model(&mut available)
            .unwrap_or_else(|| take_index_or(&mut available, 0, &fallback));
        let fable = take_index_or(&mut available, 0, &fallback);
        let custom = take_index_or(&mut available, 0, &fallback);

        let mut env = vec![
            ("ANTHROPIC_MODEL".to_string(), sonnet.clone()),
            ("ANTHROPIC_SMALL_FAST_MODEL".to_string(), haiku.clone()),
        ];
        env.extend(default_model_slot_env("ANTHROPIC_DEFAULT_SONNET", &sonnet));
        env.extend(default_model_slot_env("ANTHROPIC_DEFAULT_OPUS", &opus));
        env.extend(default_model_slot_env("ANTHROPIC_DEFAULT_HAIKU", &haiku));
        env.extend(default_model_slot_env("ANTHROPIC_DEFAULT_FABLE", &fable));
        env.extend(custom_model_slot_env(&custom));
        env
    }
}

fn add_provider_model_ids<'a>(
    models: &mut Vec<String>,
    provider_name: &str,
    provider_models: impl Iterator<Item = &'a String>,
) {
    for model in provider_models {
        let id = normalize_provider_model_id(provider_name, model);
        if !models.iter().any(|existing| existing == &id) {
            models.push(id);
        }
    }
}

fn normalize_provider_model_id(provider_name: &str, model: &str) -> String {
    if model.starts_with(&format!("{provider_name}/")) {
        model.to_string()
    } else {
        format!("{provider_name}/{model}")
    }
}

fn take_index_or(models: &mut Vec<String>, index: usize, fallback: &str) -> String {
    if index < models.len() {
        models.remove(index)
    } else {
        fallback.to_string()
    }
}

fn take_preferred_fast_model(models: &mut Vec<String>) -> Option<String> {
    models
        .iter()
        .position(|model| {
            let lower = model.to_lowercase();
            lower.contains("flash") || lower.contains("haiku") || lower.contains("fast")
        })
        .map(|index| models.remove(index))
}

fn default_model_slot_env(prefix: &str, model: &str) -> Vec<(String, String)> {
    vec![
        (format!("{prefix}_MODEL"), model.to_string()),
        (format!("{prefix}_MODEL_NAME"), model.to_string()),
        (
            format!("{prefix}_MODEL_DESCRIPTION"),
            "openclaude proxy model".to_string(),
        ),
    ]
}

fn custom_model_slot_env(model: &str) -> Vec<(String, String)> {
    vec![
        (
            "ANTHROPIC_CUSTOM_MODEL_OPTION".to_string(),
            model.to_string(),
        ),
        (
            "ANTHROPIC_CUSTOM_MODEL_OPTION_NAME".to_string(),
            model.to_string(),
        ),
        (
            "ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION".to_string(),
            "openclaude proxy model".to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(default_model: &str, models: &[&str]) -> ProviderConfig {
        ProviderConfig {
            adapter: AdapterKind::Anthropic,
            base_url: "https://api.example.test".to_string(),
            api_key: Some("${TEST_API_KEY}".to_string()),
            default_model: Some(default_model.to_string()),
            models: models.iter().map(|model| model.to_string()).collect(),
            context_window: None,
            model_context_windows: HashMap::new(),
            model_input_modalities: HashMap::new(),
            reasoning_efforts: None,
            model_reasoning_efforts: HashMap::new(),
            no_vision_models: Vec::new(),
        }
    }

    #[test]
    fn claude_model_ids_put_default_provider_first_and_deduplicate() {
        let mut cfg = Config::default_config();
        cfg.default_provider = "umans".to_string();
        cfg.providers.insert(
            "umans".to_string(),
            provider(
                "umans-coder",
                &["umans-coder", "umans-flash", "umans/umans-glm-5.2"],
            ),
        );

        let ids = cfg.claude_model_ids();
        assert_eq!(ids[0], "umans/umans-coder");
        assert_eq!(ids[1], "umans/umans-flash");
        assert_eq!(ids[2], "umans/umans-glm-5.2");
        assert_eq!(
            ids.iter()
                .filter(|model| model.as_str() == "umans/umans-coder")
                .count(),
            1
        );
    }

    #[test]
    fn claude_model_env_spreads_proxy_models_across_native_slots() {
        let mut cfg = Config::default_config();
        cfg.default_provider = "umans".to_string();
        cfg.providers.clear();
        cfg.providers.insert(
            "umans".to_string(),
            provider(
                "umans-coder",
                &[
                    "umans-coder",
                    "umans-kimi-k2.7",
                    "umans-kimi-k2.6",
                    "umans-flash",
                    "umans-glm-5.2",
                ],
            ),
        );

        let env = cfg.claude_model_env();
        let value = |key: &str| {
            env.iter()
                .find_map(|(name, value)| (name == key).then_some(value.as_str()))
                .unwrap()
        };
        assert_eq!(value("ANTHROPIC_DEFAULT_SONNET_MODEL"), "umans/umans-coder");
        assert_eq!(
            value("ANTHROPIC_DEFAULT_OPUS_MODEL"),
            "umans/umans-kimi-k2.7"
        );
        assert_eq!(value("ANTHROPIC_DEFAULT_HAIKU_MODEL"), "umans/umans-flash");
        assert_eq!(
            value("ANTHROPIC_DEFAULT_FABLE_MODEL"),
            "umans/umans-kimi-k2.6"
        );
        assert_eq!(
            value("ANTHROPIC_CUSTOM_MODEL_OPTION"),
            "umans/umans-glm-5.2"
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_file_writer_uses_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.json");
        write_private_file(&path, "{\"token\":\"secret\"}\n").unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert_eq!(raw, "{\"token\":\"secret\"}\n");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub pid: u32,
    pub host: String,
    pub port: u16,
}

impl ProviderConfig {
    pub fn resolve_api_key(&self) -> Option<String> {
        self.api_key.as_ref().and_then(|value| {
            if let Some(name) = value.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
                std::env::var(name).ok().filter(|s| !s.is_empty())
            } else if let Some(name) = value.strip_prefix('$') {
                std::env::var(name).ok().filter(|s| !s.is_empty())
            } else if value.is_empty() {
                None
            } else {
                Some(value.clone())
            }
        })
    }
}

pub fn enrich_provider_metadata(name: &str, provider: &mut ProviderConfig) {
    if name != "umans" {
        return;
    }
    if provider.context_window.is_none() {
        provider.context_window = Some(262_144);
    }
    for (model, window) in [
        ("umans-coder", 262_144),
        ("umans-kimi-k2.7", 262_144),
        ("umans-kimi-k2.6", 262_144),
        ("umans-flash", 262_144),
        ("umans-glm-5.2", 405_504),
        ("umans-glm-5.1", 202_752),
        ("umans-qwen3.6-35b-a3b", 262_144),
    ] {
        provider
            .model_context_windows
            .entry(model.to_string())
            .or_insert(window);
    }
    for model in ["umans-glm-5.2", "umans-glm-5.1"] {
        provider
            .model_input_modalities
            .entry(model.to_string())
            .or_insert_with(|| vec!["text".to_string()]);
        if !provider.no_vision_models.iter().any(|m| m == model) {
            provider.no_vision_models.push(model.to_string());
        }
    }
    for model in ["umans-coder", "umans-kimi-k2.7", "umans-kimi-k2.6"] {
        provider
            .model_reasoning_efforts
            .entry(model.to_string())
            .or_insert_with(|| {
                vec![
                    "low".to_string(),
                    "medium".to_string(),
                    "high".to_string(),
                    "xhigh".to_string(),
                ]
            });
    }
    for model in ["umans-glm-5.2", "umans-glm-5.1"] {
        provider
            .model_reasoning_efforts
            .entry(model.to_string())
            .or_insert_with(|| vec!["high".to_string(), "xhigh".to_string()]);
    }
    for model in ["umans-flash", "umans-qwen3.6-35b-a3b"] {
        provider
            .model_reasoning_efforts
            .entry(model.to_string())
            .or_insert_with(|| vec!["low".to_string(), "medium".to_string(), "high".to_string()]);
    }
}

pub fn config_dir() -> anyhow::Result<PathBuf> {
    if let Some(raw) = std::env::var_os("OPENCLAUDE_HOME") {
        return Ok(PathBuf::from(raw));
    }
    let base = BaseDirs::new().context("resolve home directory")?;
    Ok(base.home_dir().join(".openclaude"))
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn runtime_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("runtime.json"))
}

pub fn write_runtime(state: &RuntimeState) -> anyhow::Result<()> {
    let path = runtime_path()?;
    write_private_file(&path, &(serde_json::to_string_pretty(state)? + "\n"))?;
    Ok(())
}

pub fn read_runtime() -> anyhow::Result<RuntimeState> {
    let raw = fs::read_to_string(runtime_path()?)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn remove_runtime() -> anyhow::Result<()> {
    let path = runtime_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn write_private_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    ensure_parent(path)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("open private file {}", path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("write private file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync private file {}", path.display()))?;
    set_private_permissions(path)?;
    Ok(())
}

pub fn set_private_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod 0600 {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}
