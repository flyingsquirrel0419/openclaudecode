mod config;
mod integration;
mod models;
mod providers;
mod server;

use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use tokio::time::sleep;
use tracing_subscriber::EnvFilter;

use crate::config::{AdapterKind, Config, ProviderConfig, config_path, enrich_provider_metadata};

#[derive(Debug, Parser)]
#[command(
    name = "occ",
    version,
    about = "openclaude: Claude Code provider gateway"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a default ~/.openclaude/config.json.
    Init {
        /// Reset to the built-in default config without prompts.
        #[arg(long)]
        reset: bool,
        /// Alias for --reset.
        #[arg(long)]
        force: bool,
    },
    /// Start the local Claude Code gateway.
    Start {
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    /// Print gateway status and config path.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Stop a running gateway.
    Stop,
    /// Ensure the local gateway is running.
    Ensure,
    /// Install a claude launcher shim that auto-starts/injects openclaude env.
    Enable,
    /// Remove the claude shim and return Claude Code to native behavior.
    Native,
    /// Restore native Claude Code behavior; `restore back` re-enables the shim.
    Restore {
        #[arg(value_name = "back")]
        back: Option<String>,
    },
    /// Alias for restore.
    Eject {
        #[arg(value_name = "back")]
        back: Option<String>,
    },
    /// Remove service/shim/config and restore native Claude Code.
    #[command(alias = "remove")]
    Uninstall,
    /// Run or manage the background service.
    Service {
        #[command(subcommand)]
        command: Option<ServiceCommand>,
    },
    /// OCX-compatible alias for Claude Code shim management.
    #[command(name = "codex-shim", alias = "claude-shim")]
    CodexShim {
        #[command(subcommand)]
        command: ShimCommand,
    },
    /// Print shell exports for using Claude Code through openclaude without a shim.
    Env,
    /// Run local diagnostics.
    Doctor,
    /// OAuth/API-key login placeholder for provider compatibility.
    Login { provider: Option<String> },
    /// Remove stored provider login information.
    Logout { provider: Option<String> },
    /// Sync providers/models into Claude Code gateway config.
    Sync,
    /// OCX-compatible no-op; Claude Code has no writable model cache.
    #[command(name = "sync-cache")]
    SyncCache,
    /// Open the local dashboard URL or print it when no GUI exists yet.
    Gui,
    /// Update instructions for npm-installed openclaude.
    Update {
        #[arg(long)]
        tag: Option<String>,
    },
    /// Print version.
    Version,
    /// Stop and restart the local gateway.
    Restart,
    /// Check gateway health.
    Health {
        #[arg(long)]
        json: bool,
    },
    /// OCX-compatible unsupported history recovery command.
    #[command(name = "recover-history")]
    RecoverHistory {
        #[arg(long)]
        legacy_openai: bool,
    },
    /// Manage providers in ~/.openclaude/config.json.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// List configured model ids as Claude Code sees them.
    Models {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProviderCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        name: String,
        #[arg(long, value_enum)]
        adapter: Option<AdapterArg>,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        default_model: Option<String>,
        #[arg(long, value_delimiter = ',')]
        model: Vec<String>,
        #[arg(long = "set-default", alias = "make-default")]
        make_default: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        sync: bool,
    },
    Remove {
        name: String,
        #[arg(long)]
        json: bool,
    },
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    SetDefault {
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    Install,
    Start,
    Stop,
    Status,
    #[command(alias = "remove")]
    Uninstall,
}

#[derive(Debug, Subcommand)]
enum ShimCommand {
    Install,
    Status,
    #[command(alias = "remove")]
    Uninstall,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AdapterArg {
    Anthropic,
    OpenaiChat,
    AzureOpenai,
    Google,
    Cursor,
    Kiro,
}

impl From<AdapterArg> for AdapterKind {
    fn from(value: AdapterArg) -> Self {
        match value {
            AdapterArg::Anthropic => AdapterKind::Anthropic,
            AdapterArg::OpenaiChat => AdapterKind::OpenAiChat,
            AdapterArg::AzureOpenai => AdapterKind::AzureOpenAi,
            AdapterArg::Google => AdapterKind::Google,
            AdapterArg::Cursor => AdapterKind::Cursor,
            AdapterArg::Kiro => AdapterKind::Kiro,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    if std::env::args().nth(1).as_deref() == Some("-v") {
        println!("openclaude {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Status { json: false }) {
        Command::Init { reset, force } => cmd_init(reset || force).await,
        Command::Start { port, host } => cmd_start(port, host).await,
        Command::Status { json } => cmd_status(json).await,
        Command::Stop => cmd_stop().await,
        Command::Ensure => cmd_ensure().await,
        Command::Enable => integration::install_claude_shim(),
        Command::Native => cmd_native().await,
        Command::Restore { back } | Command::Eject { back } => cmd_restore(back).await,
        Command::Uninstall => cmd_uninstall().await,
        Command::Service { command } => cmd_service(command).await,
        Command::CodexShim { command } => cmd_shim(command),
        Command::Env => cmd_env(),
        Command::Doctor => cmd_doctor(),
        Command::Login { provider } => cmd_login(provider),
        Command::Logout { provider } => cmd_logout(provider),
        Command::Sync => cmd_sync(),
        Command::SyncCache => cmd_sync_cache(),
        Command::Gui => cmd_gui(),
        Command::Update { tag } => cmd_update(tag),
        Command::Version => {
            println!("openclaude {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Restart => cmd_restart().await,
        Command::Health { json } => cmd_health(json).await,
        Command::RecoverHistory { legacy_openai } => cmd_recover_history(legacy_openai),
        Command::Provider { command } => cmd_provider(command),
        Command::Models { provider, json } => cmd_models(provider, json),
    }
}

async fn cmd_ensure() -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    if gateway_healthy(&cfg).await {
        println!(
            "openclaude gateway already running: http://{}:{}",
            cfg.host, cfg.port
        );
        return Ok(());
    }
    if gateway_reachable(&cfg).await {
        stop_local_gateway_fallback(&cfg, "gateway token mismatch or stale config").await?;
    }

    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut command = StdCommand::new(exe);
    command
        .arg("start")
        .env("OPENCLAUDE_HOME", config::config_dir()?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    command.spawn().context("spawn openclaude gateway")?;

    for _ in 0..50 {
        if gateway_healthy(&cfg).await {
            println!(
                "openclaude gateway started: http://{}:{}",
                cfg.host, cfg.port
            );
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("openclaude gateway did not become healthy")
}

async fn gateway_healthy(cfg: &Config) -> bool {
    let url = format!("http://{}:{}/v1/models", cfg.host, cfg.port);
    let Ok(resp) = reqwest::Client::new()
        .get(url)
        .header("x-api-key", &cfg.gateway_token)
        .send()
        .await
    else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(body) = resp.json::<serde_json::Value>().await else {
        return false;
    };
    body.get("data")
        .and_then(|value| value.as_array())
        .is_some()
}

async fn gateway_reachable(cfg: &Config) -> bool {
    let url = format!("http://{}:{}/healthz", cfg.host, cfg.port);
    reqwest::get(url)
        .await
        .map(|resp| resp.status().is_success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum AuthKind {
    Forward,
    Oauth,
    Key,
    Local,
}

#[derive(Debug, Clone, Copy)]
struct InitProvider {
    id: &'static str,
    label: &'static str,
    adapter: AdapterKind,
    base_url: &'static str,
    auth_kind: AuthKind,
    dashboard_url: Option<&'static str>,
    default_model: Option<&'static str>,
    models: &'static [&'static str],
}

const UMANS_MODELS: &[&str] = &[
    "umans-coder",
    "umans-kimi-k2.7",
    "umans-kimi-k2.6",
    "umans-flash",
    "umans-glm-5.2",
    "umans-glm-5.1",
    "umans-qwen3.6-35b-a3b",
];

const ANTHROPIC_MODELS: &[&str] = &[
    "claude-sonnet-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-4-6",
    "claude-haiku-4-5",
];

const OPENROUTER_MODELS: &[&str] = &[
    "anthropic/claude-sonnet-5",
    "openai/gpt-5",
    "google/gemini-2.5-pro",
];

const GOOGLE_MODELS: &[&str] = &["gemini-3-pro", "gemini-2.5-pro", "gemini-2.5-flash"];
const OLLAMA_MODELS: &[&str] = &["llama3.1"];
const LM_STUDIO_MODELS: &[&str] = &["local-model"];

fn init_providers() -> Vec<InitProvider> {
    vec![
        InitProvider {
            id: "openai",
            label: "OpenAI (ChatGPT login)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://chatgpt.com/backend-api/codex",
            auth_kind: AuthKind::Forward,
            dashboard_url: None,
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "cursor",
            label: "Cursor (experimental)",
            adapter: AdapterKind::Cursor,
            base_url: "https://api2.cursor.sh",
            auth_kind: AuthKind::Oauth,
            dashboard_url: None,
            default_model: Some("auto"),
            models: &["auto"],
        },
        InitProvider {
            id: "xai",
            label: "xAI Grok",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.x.ai/v1",
            auth_kind: AuthKind::Oauth,
            dashboard_url: None,
            default_model: Some("grok-4.3"),
            models: &["grok-4.3"],
        },
        InitProvider {
            id: "anthropic",
            label: "Anthropic Claude",
            adapter: AdapterKind::Anthropic,
            base_url: "https://api.anthropic.com",
            auth_kind: AuthKind::Oauth,
            dashboard_url: None,
            default_model: Some("claude-sonnet-4-6"),
            models: ANTHROPIC_MODELS,
        },
        InitProvider {
            id: "kimi",
            label: "Kimi",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.kimi.com/coding/v1",
            auth_kind: AuthKind::Oauth,
            dashboard_url: None,
            default_model: Some("kimi-k2.7-code"),
            models: &[
                "kimi-k2.7-code",
                "kimi-k2.7-code-highspeed",
                "kimi-k2.6",
                "kimi-k2.5",
            ],
        },
        InitProvider {
            id: "kiro",
            label: "Kiro (AWS CodeWhisperer)",
            adapter: AdapterKind::Kiro,
            base_url: "https://runtime.us-east-1.kiro.dev",
            auth_kind: AuthKind::Oauth,
            dashboard_url: None,
            default_model: Some("kiro-auto"),
            models: &["kiro-auto"],
        },
        InitProvider {
            id: "openai-apikey",
            label: "OpenAI (API key)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.openai.com/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://platform.openai.com/api-keys"),
            default_model: Some("gpt-5.5"),
            models: &["gpt-5.5", "gpt-5"],
        },
        InitProvider {
            id: "umans",
            label: "Umans AI Coding Plan",
            adapter: AdapterKind::Anthropic,
            base_url: "https://api.code.umans.ai",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://app.umans.ai/billing"),
            default_model: Some("umans-coder"),
            models: UMANS_MODELS,
        },
        InitProvider {
            id: "openrouter",
            label: "OpenRouter",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://openrouter.ai/api/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://openrouter.ai/settings/keys"),
            default_model: Some("anthropic/claude-sonnet-5"),
            models: OPENROUTER_MODELS,
        },
        InitProvider {
            id: "google",
            label: "Google Gemini API",
            adapter: AdapterKind::Google,
            base_url: "https://generativelanguage.googleapis.com",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://aistudio.google.com/app/apikey"),
            default_model: Some("gemini-3-pro"),
            models: GOOGLE_MODELS,
        },
        InitProvider {
            id: "google-vertex",
            label: "Google Vertex AI",
            adapter: AdapterKind::Google,
            base_url: "https://aiplatform.googleapis.com",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://console.cloud.google.com/vertex-ai"),
            default_model: Some("gemini-3-pro"),
            models: GOOGLE_MODELS,
        },
        InitProvider {
            id: "azure-openai",
            label: "Azure OpenAI",
            adapter: AdapterKind::AzureOpenAi,
            base_url: "https://{resource}.openai.azure.com/openai",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://portal.azure.com"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "deepseek",
            label: "DeepSeek",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.deepseek.com",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://platform.deepseek.com/api_keys"),
            default_model: Some("deepseek-chat"),
            models: &[
                "deepseek-chat",
                "deepseek-reasoner",
                "deepseek-v4-pro",
                "deepseek-v4-flash",
            ],
        },
        InitProvider {
            id: "opencode-go",
            label: "opencode go",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://opencode.ai/zen/go/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://opencode.ai/auth"),
            default_model: Some("kimi-k2.7-code"),
            models: &["kimi-k2.7-code", "glm-5.2", "deepseek-v4-pro"],
        },
        InitProvider {
            id: "neuralwatt",
            label: "Neuralwatt Cloud",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.neuralwatt.com/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://portal.neuralwatt.com"),
            default_model: Some("glm-5.2"),
            models: &["glm-5.2", "kimi-k2.7-code", "qwen3.6-35b"],
        },
        InitProvider {
            id: "groq",
            label: "Groq",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.groq.com/openai/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://console.groq.com/keys"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "cerebras",
            label: "Cerebras",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.cerebras.ai/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://cloud.cerebras.ai/platform/apikeys"),
            default_model: Some("llama-3.3-70b"),
            models: &["llama-3.3-70b"],
        },
        InitProvider {
            id: "together",
            label: "Together",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.together.xyz/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://api.together.xyz/settings/api-keys"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "fireworks",
            label: "Fireworks",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.fireworks.ai/inference/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://fireworks.ai/account/api-keys"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "firepass",
            label: "Fire Pass (Fireworks Kimi)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.fireworks.ai/inference/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://fireworks.ai/account/api-keys"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "moonshot",
            label: "Moonshot (Kimi API)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.moonshot.ai/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://platform.moonshot.ai/console/api-keys"),
            default_model: Some("kimi-k2.7-code"),
            models: &[
                "kimi-k2.7-code",
                "kimi-k2.7-code-highspeed",
                "kimi-k2.6",
                "kimi-k2.5",
            ],
        },
        InitProvider {
            id: "huggingface",
            label: "Hugging Face",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://router.huggingface.co/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://huggingface.co/settings/tokens"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "nvidia",
            label: "NVIDIA NIM",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://integrate.api.nvidia.com/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://build.nvidia.com"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "venice",
            label: "Venice",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.venice.ai/api/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://venice.ai/settings/api"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "zai",
            label: "Z.AI - GLM Coding Plan",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.z.ai/api/coding/paas/v4",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://z.ai/manage-apikey/apikey-list"),
            default_model: Some("glm-5.2"),
            models: &["glm-5.2", "glm-5.2[1m]", "glm-5.1", "glm-5", "glm-4.6"],
        },
        InitProvider {
            id: "nanogpt",
            label: "NanoGPT",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://nano-gpt.com/api/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://nano-gpt.com/api"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "synthetic",
            label: "Synthetic",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.synthetic.new/openai/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://synthetic.new"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "qwen-portal",
            label: "Qwen Portal",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://portal.qwen.ai/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://portal.qwen.ai"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "qianfan",
            label: "Qianfan (Baidu)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://qianfan.baidubce.com/v2",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://console.bce.baidu.com/iam/#/iam/apikey/list"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "alibaba",
            label: "Alibaba Coding Plan",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://coding-intl.dashscope.aliyuncs.com/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://dashscope.console.aliyun.com/apiKey"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "parallel",
            label: "Parallel",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://platform.parallel.ai",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://platform.parallel.ai"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "zenmux",
            label: "ZenMux",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://zenmux.ai/api/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://zenmux.ai"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "litellm",
            label: "LiteLLM (self-hosted)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "http://localhost:4000/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://docs.litellm.ai/docs/proxy/quick_start"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "ollama-cloud",
            label: "Ollama Cloud",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://ollama.com/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://ollama.com/settings/keys"),
            default_model: Some("glm-5.2"),
            models: &[
                "glm-5.2",
                "deepseek-v4-pro",
                "qwen3-coder",
                "gpt-oss:120b",
                "kimi-k2.6",
            ],
        },
        InitProvider {
            id: "mistral",
            label: "Mistral",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.mistral.ai/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://console.mistral.ai/api-keys"),
            default_model: Some("codestral-latest"),
            models: &["codestral-latest"],
        },
        InitProvider {
            id: "minimax",
            label: "MiniMax - Coding Plan",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.minimax.io/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://platform.minimax.io"),
            default_model: Some("MiniMax-M2.5"),
            models: &["MiniMax-M2.5"],
        },
        InitProvider {
            id: "minimax-cn",
            label: "MiniMax - Coding Plan (CN)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.minimaxi.com/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://platform.minimaxi.com"),
            default_model: Some("MiniMax-M2.5"),
            models: &["MiniMax-M2.5"],
        },
        InitProvider {
            id: "kimi-code",
            label: "Kimi (coding)",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.kimi.com/coding/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://platform.moonshot.cn/console/api-keys"),
            default_model: Some("kimi-k2.7-code"),
            models: &[
                "kimi-k2.7-code",
                "kimi-k2.7-code-highspeed",
                "kimi-k2.6",
                "kimi-k2.5",
            ],
        },
        InitProvider {
            id: "opencode-zen",
            label: "opencode zen",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://opencode.ai/zen/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://opencode.ai/auth"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "vercel-ai-gateway",
            label: "Vercel AI Gateway",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://ai-gateway.vercel.sh/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://vercel.com/dashboard"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "xiaomi",
            label: "Xiaomi MiMo",
            adapter: AdapterKind::Anthropic,
            base_url: "https://api.xiaomimimo.com/anthropic",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://xiaomimimo.com"),
            default_model: Some("mimo-v2.5-pro"),
            models: &["mimo-v2.5-pro"],
        },
        InitProvider {
            id: "kilo",
            label: "Kilo",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.kilo.ai/api/gateway",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://kilo.ai"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "cloudflare-ai-gateway",
            label: "Cloudflare AI Gateway",
            adapter: AdapterKind::Anthropic,
            base_url: "https://gateway.ai.cloudflare.com/v1/{account-id}/{gateway}/anthropic",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://dash.cloudflare.com/?to=/:account/ai/ai-gateway"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "github-copilot",
            label: "GitHub Copilot",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://api.githubcopilot.com",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://github.com/settings/copilot"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "gitlab-duo",
            label: "GitLab Duo",
            adapter: AdapterKind::OpenAiChat,
            base_url: "https://cloud.gitlab.com/ai/v1/proxy/openai/v1",
            auth_kind: AuthKind::Key,
            dashboard_url: Some("https://gitlab.com/-/user_settings/personal_access_tokens"),
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "ollama",
            label: "Ollama local",
            adapter: AdapterKind::OpenAiChat,
            base_url: "http://127.0.0.1:11434/v1",
            auth_kind: AuthKind::Local,
            dashboard_url: None,
            default_model: Some("llama3.1"),
            models: OLLAMA_MODELS,
        },
        InitProvider {
            id: "vllm",
            label: "vLLM local",
            adapter: AdapterKind::OpenAiChat,
            base_url: "http://localhost:8000/v1",
            auth_kind: AuthKind::Local,
            dashboard_url: None,
            default_model: None,
            models: &[],
        },
        InitProvider {
            id: "lm-studio",
            label: "LM Studio local",
            adapter: AdapterKind::OpenAiChat,
            base_url: "http://127.0.0.1:1234/v1",
            auth_kind: AuthKind::Local,
            dashboard_url: None,
            default_model: Some("local-model"),
            models: LM_STUDIO_MODELS,
        },
    ]
}

async fn cmd_init(reset: bool) -> anyhow::Result<()> {
    stop_proxy_before_init().await?;
    if reset {
        save_init_config(Config::default_config())?;
        refresh_existing_shim_after_init();
        return Ok(());
    }

    println!();
    println!("openclaude (occ) setup");
    println!();
    println!(
        "Claude Code subscription OAuth is native-only. occ configures API-key or local providers for the gateway."
    );
    println!();

    let providers = init_providers();
    print_init_menu(&providers);
    let choice = prompt("\nSelect provider (number): ")?;
    let idx = choice
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1));

    let (provider_name, provider_config) = if let Some(idx) =
        idx.filter(|idx| *idx < providers.len())
    {
        let provider = providers[idx];
        println!();
        println!("{}", provider.label);
        println!("Base URL: {}", provider.base_url);
        if let Some(url) = provider.dashboard_url {
            println!("Get your key: {url}");
        }

        let api_key = match provider.auth_kind {
            AuthKind::Forward => {
                println!(
                    "ChatGPT/Codex forward login is Codex-native. Claude Code cannot reuse that subscription token."
                );
                String::new()
            }
            AuthKind::Oauth => {
                println!(
                    "OAuth login for this provider is not portable from ocx to Claude Code. Use an API-key provider when available."
                );
                String::new()
            }
            AuthKind::Key => {
                let env = env_key_for(provider.id);
                prompt(&format!("\nAPI key (paste, or env var ${env}): "))?
                    .trim()
                    .to_string()
            }
            AuthKind::Local => prompt("\nAPI key (usually blank, press Enter): ")?
                .trim()
                .to_string(),
        };
        let model_choices = init_model_choices(provider, &api_key).await;
        let model_choice = prompt_model_choice(provider.default_model, &model_choices)?;
        (
            provider.id.to_string(),
            provider_config_from_init(provider, api_key, model_choice, model_choices),
        )
    } else if idx == Some(providers.len()) {
        let provider_name = prompt("Provider name: ")?.trim().to_string();
        validate_provider_name(&provider_name)?;
        let base_url = prompt("Base URL (e.g. http://localhost:11434/v1): ")?
            .trim()
            .to_string();
        if base_url.is_empty() {
            anyhow::bail!("Base URL is required for a custom provider");
        }
        let adapter = prompt("Adapter [openai-chat]: ")?.trim().to_string();
        let adapter = if adapter.is_empty() {
            AdapterKind::OpenAiChat
        } else {
            parse_adapter(&adapter)?
        };
        let api_key = prompt("API key (optional): ")?.trim().to_string();
        let mut provider_config = ProviderConfig {
            adapter,
            base_url,
            api_key: (!api_key.is_empty()).then_some(api_key),
            default_model: None,
            models: Vec::new(),
            context_window: None,
            model_context_windows: Default::default(),
            model_input_modalities: Default::default(),
            reasoning_efforts: None,
            model_reasoning_efforts: Default::default(),
            no_vision_models: Vec::new(),
        };
        let model_choices = fetch_live_models(&provider_name, &provider_config)
            .await
            .unwrap_or_default();
        let default_model = prompt_model_choice(None, &model_choices)?;
        provider_config.default_model = (!default_model.is_empty()).then_some(default_model);
        provider_config.models = model_choices;
        (provider_name, provider_config)
    } else {
        anyhow::bail!("invalid provider selection");
    };

    let port_choice = prompt("\nProxy port [10110]: ")?;
    let port = port_choice.trim().parse::<u16>().unwrap_or(10110);
    let shim_answer = prompt("Install Claude autostart shim? [Y/n]: ")?;
    let install_shim = !matches!(shim_answer.trim().to_ascii_lowercase().as_str(), "n" | "no");

    let mut providers = BTreeMap::new();
    providers.insert(provider_name.clone(), provider_config);
    let cfg = Config {
        host: "127.0.0.1".to_string(),
        port,
        gateway_token: format!("occ_{}", uuid::Uuid::new_v4().simple()),
        default_provider: provider_name.clone(),
        providers,
    };

    save_init_config(cfg)?;
    println!();
    println!("Config saved to {}", config_path()?.display());
    if (install_shim || integration::shim_state_path()?.exists())
        && let Err(err) = integration::install_claude_shim()
    {
        println!("Claude autostart shim skipped: {err:#}");
    }
    println!();
    println!("Setup complete.");
    println!("Run: claude");
    println!("Or without the shim: source <(occ env) && occ ensure");
    Ok(())
}

async fn stop_proxy_before_init() -> anyhow::Result<()> {
    let Ok(cfg) = Config::load() else {
        return Ok(());
    };
    if gateway_reachable(&cfg).await {
        cmd_stop()
            .await
            .context("stop existing proxy before init")?;
    } else if let Ok(runtime) = config::read_runtime()
        && process_exists(runtime.pid)
        && is_openclaude_pid(runtime.pid)
    {
        stop_local_gateway_fallback(&cfg, "stale runtime before init")
            .await
            .context("stop stale proxy before init")?;
    }
    Ok(())
}

fn refresh_existing_shim_after_init() {
    if integration::shim_state_path()
        .map(|path| path.exists())
        .unwrap_or(false)
        && let Err(err) = integration::install_claude_shim()
    {
        println!("Claude autostart shim refresh skipped: {err:#}");
    }
}

fn print_init_menu(providers: &[InitProvider]) {
    println!("Available providers:");
    println!();
    println!("  ChatGPT login:");
    for (idx, provider) in providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| provider.auth_kind == AuthKind::Forward)
    {
        println!("   {:>2}. {}", idx + 1, provider.label);
    }
    println!();
    println!("  Account login (OAuth - ocx-compatible stub in occ):");
    for (idx, provider) in providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| provider.auth_kind == AuthKind::Oauth)
    {
        println!("   {:>2}. {}", idx + 1, provider.label);
    }
    println!();
    println!("  API key providers:");
    for (idx, provider) in providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| provider.auth_kind == AuthKind::Key)
    {
        println!("   {:>2}. {}", idx + 1, provider.label);
    }
    println!();
    println!("  Local servers:");
    for (idx, provider) in providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| provider.auth_kind == AuthKind::Local)
    {
        println!("   {:>2}. {}", idx + 1, provider.label);
    }
    println!();
    println!("   {:>2}. custom (enter URL manually)", providers.len() + 1);
}

async fn init_model_choices(provider: InitProvider, api_key: &str) -> Vec<String> {
    let mut provider_config =
        provider_config_from_init(provider, api_key.to_string(), String::new(), Vec::new());
    if provider.auth_kind == AuthKind::Key && provider_config.resolve_api_key().is_none() {
        return provider_config.models;
    }
    if matches!(provider.auth_kind, AuthKind::Key | AuthKind::Local)
        && let Ok(models) = fetch_live_models(provider.id, &provider_config).await
        && !models.is_empty()
    {
        provider_config.models = models.clone();
        enrich_provider_metadata(provider.id, &mut provider_config);
        return provider_config.models;
    }
    provider_config.models
}

async fn fetch_live_models(
    _provider_name: &str,
    provider: &ProviderConfig,
) -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build model fetch client")?;
    let base = provider.base_url.trim_end_matches('/');
    let mut request = match provider.adapter {
        AdapterKind::Google => {
            let Some(key) = provider.resolve_api_key() else {
                anyhow::bail!("missing API key for model fetch");
            };
            client.get(format!("{base}/v1beta/models?key={key}"))
        }
        AdapterKind::Anthropic => {
            let mut req = client
                .get(format!("{base}/v1/models"))
                .header("anthropic-version", "2023-06-01");
            if let Some(key) = provider.resolve_api_key() {
                req = req.header("x-api-key", key);
            }
            req
        }
        AdapterKind::OpenAiChat | AdapterKind::AzureOpenAi => {
            let mut req = client.get(format!("{base}/models"));
            if let Some(key) = provider.resolve_api_key() {
                req = req.header("authorization", format!("Bearer {key}"));
                req = req.header("api-key", key);
            }
            req
        }
        AdapterKind::Cursor | AdapterKind::Kiro => {
            anyhow::bail!("adapter does not expose a generic model list");
        }
    };
    request = request.header("accept", "application/json");
    let value: serde_json::Value = request
        .send()
        .await
        .context("fetch provider models")?
        .error_for_status()
        .context("provider models status")?
        .json()
        .await
        .context("parse provider models")?;
    Ok(parse_model_list(value))
}

fn parse_model_list(value: serde_json::Value) -> Vec<String> {
    let mut models = Vec::new();
    if let Some(items) = value.get("data").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                models.push(id.to_string());
            }
        }
    }
    if let Some(items) = value.get("models").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                models.push(name.strip_prefix("models/").unwrap_or(name).to_string());
            }
        }
    }
    models.sort();
    models.dedup();
    models
}

fn prompt_model_choice(default_model: Option<&str>, models: &[String]) -> anyhow::Result<String> {
    if models.is_empty() {
        let model_prompt = match default_model {
            Some(default_model) => format!("Default model [{default_model}]: "),
            None => "Default model (optional): ".to_string(),
        };
        let choice = prompt(&model_prompt)?.trim().to_string();
        return Ok(choice.or_default_model(default_model));
    }

    println!();
    println!("Available models:");
    for (idx, model) in models.iter().enumerate() {
        let marker = if Some(model.as_str()) == default_model {
            " *"
        } else {
            ""
        };
        println!("   {:>2}. {}{}", idx + 1, model, marker);
    }
    let default_idx = default_model
        .and_then(|default| models.iter().position(|model| model == default))
        .map(|idx| idx + 1)
        .unwrap_or(1);
    let answer = prompt(&format!("Select default model [{default_idx}]: "))?;
    if answer.trim().is_empty() {
        return Ok(models[default_idx - 1].clone());
    }
    if let Ok(idx) = answer.trim().parse::<usize>()
        && (1..=models.len()).contains(&idx)
    {
        return Ok(models[idx - 1].clone());
    }
    Ok(answer.trim().to_string())
}

trait DefaultModelChoice {
    fn or_default_model(self, default_model: Option<&str>) -> String;
}

impl DefaultModelChoice for String {
    fn or_default_model(self, default_model: Option<&str>) -> String {
        if self.is_empty() {
            default_model.unwrap_or_default().to_string()
        } else {
            self
        }
    }
}

fn provider_config_from_init(
    provider: InitProvider,
    api_key: String,
    model_choice: String,
    models_override: Vec<String>,
) -> ProviderConfig {
    let default_model = if model_choice.is_empty() {
        provider.default_model.map(str::to_string)
    } else {
        Some(model_choice)
    };
    let mut provider_config = ProviderConfig {
        adapter: provider.adapter,
        base_url: provider.base_url.to_string(),
        api_key: match provider.auth_kind {
            AuthKind::Key => Some(if api_key.is_empty() {
                format!("${{{}}}", env_key_for(provider.id))
            } else {
                api_key
            }),
            AuthKind::Forward | AuthKind::Oauth | AuthKind::Local => {
                (!api_key.is_empty()).then_some(api_key)
            }
        },
        default_model,
        models: if models_override.is_empty() {
            provider
                .models
                .iter()
                .map(|model| model.to_string())
                .collect()
        } else {
            models_override
        },
        context_window: None,
        model_context_windows: Default::default(),
        model_input_modalities: Default::default(),
        reasoning_efforts: None,
        model_reasoning_efforts: Default::default(),
        no_vision_models: Vec::new(),
    };
    enrich_provider_metadata(provider.id, &mut provider_config);
    provider_config
}

fn prompt(question: &str) -> anyhow::Result<String> {
    print!("{question}");
    io::stdout().flush().context("flush prompt")?;
    let mut answer = String::new();
    let bytes = io::stdin().read_line(&mut answer).context("read prompt")?;
    if bytes == 0 {
        anyhow::bail!("init cancelled");
    }
    Ok(answer.trim_end_matches(['\r', '\n']).to_string())
}

fn env_key_for(provider_id: &str) -> String {
    let mut key = String::new();
    for ch in provider_id.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_uppercase());
        } else {
            key.push('_');
        }
    }
    key.push_str("_API_KEY");
    key
}

fn validate_provider_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty()
        || matches!(name, "__proto__" | "constructor" | "prototype")
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        anyhow::bail!(
            "Provider name must use letters, numbers, dot, underscore, or hyphen and cannot be a reserved object key."
        );
    }
    Ok(())
}

fn parse_adapter(value: &str) -> anyhow::Result<AdapterKind> {
    match value {
        "anthropic" => Ok(AdapterKind::Anthropic),
        "openai-chat" => Ok(AdapterKind::OpenAiChat),
        "azure-openai" => Ok(AdapterKind::AzureOpenAi),
        "google" => Ok(AdapterKind::Google),
        "cursor" => Ok(AdapterKind::Cursor),
        "kiro" => Ok(AdapterKind::Kiro),
        _ => anyhow::bail!(
            "unsupported adapter `{value}`; expected anthropic, openai-chat, azure-openai, google, cursor, or kiro"
        ),
    }
}

fn save_init_config(cfg: Config) -> anyhow::Result<Option<PathBuf>> {
    let path = config_path()?;
    let backup = backup_existing_config(&path)?;
    if let Err(err) = cfg.save() {
        restore_init_backup(&path, backup.as_deref());
        return Err(err);
    }
    Ok(backup)
}

fn backup_existing_config(path: &Path) -> anyhow::Result<Option<PathBuf>> {
    let Some(parent) = path.parent() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    fs::create_dir_all(parent).context("create config dir")?;
    remove_old_init_backups(parent)?;
    let backup = path.with_extension("json.bak");
    fs::copy(path, &backup).with_context(|| {
        format!(
            "backup existing config {} to {}",
            path.display(),
            backup.display()
        )
    })?;
    Ok(Some(backup))
}

fn remove_old_init_backups(parent: &Path) -> anyhow::Result<()> {
    if !parent.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(parent).with_context(|| format!("read {}", parent.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("config.json.bak") {
            fs::remove_file(entry.path())
                .with_context(|| format!("remove old backup {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn restore_init_backup(path: &Path, backup: Option<&Path>) {
    if let Some(backup) = backup {
        let _ = fs::copy(backup, path);
    } else if path.exists() {
        let _ = fs::remove_file(path);
    }
}

async fn cmd_start(port: Option<u16>, host: String) -> anyhow::Result<()> {
    let mut cfg = Config::load_or_create()?;
    if let Some(port) = port {
        cfg.port = port;
    }
    cfg.host = host;
    cfg.save()?;
    if gateway_healthy(&cfg).await {
        println!(
            "openclaude gateway already running: http://{}:{}",
            cfg.host, cfg.port
        );
        return Ok(());
    }
    if gateway_reachable(&cfg).await {
        stop_local_gateway_fallback(&cfg, "gateway token mismatch or stale config").await?;
    }
    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port)
        .parse()
        .with_context(|| format!("invalid listen address {}:{}", cfg.host, cfg.port))?;
    server::serve(addr, cfg).await
}

async fn cmd_stop() -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    let url = format!("http://{}:{}/api/stop", cfg.host, cfg.port);
    let client = reqwest::Client::new();
    match client
        .post(url)
        .header("x-api-key", &cfg.gateway_token)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            for _ in 0..30 {
                if !gateway_reachable(&cfg).await {
                    let _ = config::remove_runtime();
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
            println!("stop requested");
            Ok(())
        }
        Ok(resp) if resp.status() == reqwest::StatusCode::UNAUTHORIZED => {
            stop_local_gateway_fallback(&cfg, "gateway token mismatch").await
        }
        Ok(resp) => anyhow::bail!("stop failed: {}", resp.status()),
        Err(err) => {
            stop_local_gateway_fallback(&cfg, &format!("gateway not reachable: {err}")).await
        }
    }
}

async fn stop_local_gateway_fallback(cfg: &Config, reason: &str) -> anyhow::Result<()> {
    let mut pids = std::collections::BTreeSet::new();
    if let Ok(runtime) = config::read_runtime() {
        pids.insert(runtime.pid);
    }
    if let Some(pid) = listener_pid(cfg.port) {
        pids.insert(pid);
    }

    let mut stopped = Vec::new();
    for pid in pids {
        if pid == std::process::id() || !is_openclaude_pid(pid) {
            continue;
        }
        terminate_pid(pid, false)?;
        stopped.push(pid);
    }

    if stopped.is_empty() {
        let _ = config::remove_runtime();
        anyhow::bail!("stop failed ({reason}); no local openclaude process found to stop");
    }

    for _ in 0..30 {
        if !gateway_reachable(cfg).await {
            let _ = config::remove_runtime();
            println!(
                "stop requested via local fallback ({reason}); stopped pid(s): {}",
                stopped
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }

    for pid in &stopped {
        if process_exists(*pid) {
            let _ = terminate_pid(*pid, true);
        }
    }
    let _ = config::remove_runtime();
    println!(
        "stop forced via local fallback ({reason}); stopped pid(s): {}",
        stopped
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

fn listener_pid(port: u16) -> Option<u32> {
    let output = StdCommand::new("ss").args(["-ltnp"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_listener_pid_from_ss(&String::from_utf8_lossy(&output.stdout), port)
}

fn parse_listener_pid_from_ss(output: &str, port: u16) -> Option<u32> {
    let port_marker = format!(":{port}");
    for line in output.lines() {
        if !line.contains("LISTEN") || !line.contains(&port_marker) {
            continue;
        }
        let Some(pid_start) = line.find("pid=") else {
            continue;
        };
        let pid = line[pid_start + 4..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if let Ok(pid) = pid.parse::<u32>() {
            return Some(pid);
        }
    }
    None
}

fn is_openclaude_pid(pid: u32) -> bool {
    let exe = fs::read_link(format!("/proc/{pid}/exe")).ok();
    let exe_match = exe
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(|name| matches!(name, "occ" | "openclaude"))
        .unwrap_or(false)
        || exe
            .as_ref()
            .map(|path| path.to_string_lossy().contains("/openclaude/"))
            .unwrap_or(false);
    if exe_match {
        return true;
    }
    fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .map(|cmdline| {
            cmdline.contains("openclaude") || cmdline.contains("/occ") || cmdline.contains(" occ ")
        })
        .unwrap_or(false)
}

fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn terminate_pid(pid: u32, force: bool) -> anyhow::Result<()> {
    let signal = if force { "-KILL" } else { "-TERM" };
    let status = StdCommand::new("kill")
        .args([signal, &pid.to_string()])
        .status()
        .with_context(|| format!("send {signal} to pid {pid}"))?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("kill {signal} {pid} failed with {status}");
    }
}

#[cfg(test)]
mod cli_tests {
    use super::parse_listener_pid_from_ss;

    #[test]
    fn parses_ss_listener_pid() {
        let output = r#"State Recv-Q Send-Q Local Address:Port Peer Address:PortProcess
LISTEN 0      128      127.0.0.1:10110   0.0.0.0:*    users:(("occ",pid=41251,fd=9))
"#;
        assert_eq!(parse_listener_pid_from_ss(output, 10110), Some(41251));
    }
}

async fn cmd_restore(back: Option<String>) -> anyhow::Result<()> {
    match back.as_deref() {
        Some("back") => {
            integration::install_claude_shim()?;
            println!("Claude Code now routes through openclaude again (undo with: occ restore).");
            Ok(())
        }
        Some(value) => anyhow::bail!("unknown restore argument: {value}; expected `back`"),
        None => cmd_native().await,
    }
}

async fn cmd_native() -> anyhow::Result<()> {
    let _ = cmd_stop().await;
    integration::uninstall_claude_shim()?;
    println!("openclaude proxy stopped; Claude Code restored to native mode.");
    Ok(())
}

async fn cmd_uninstall() -> anyhow::Result<()> {
    let _ = cmd_stop().await;
    let _ = integration::uninstall_claude_shim();
    let dir = config::config_dir()?;
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
    }
    println!(
        "openclaude uninstalled: removed shim/runtime/config at {}",
        dir.display()
    );
    Ok(())
}

async fn cmd_service(command: Option<ServiceCommand>) -> anyhow::Result<()> {
    match command.unwrap_or(ServiceCommand::Install) {
        ServiceCommand::Install | ServiceCommand::Start => {
            cmd_ensure().await?;
            println!("openclaude service compatibility mode: gateway ensured via occ ensure");
        }
        ServiceCommand::Stop => {
            let _ = cmd_stop().await;
            println!("openclaude service compatibility mode: gateway stop requested");
        }
        ServiceCommand::Status => {
            cmd_status(false).await?;
            println!(
                "service manager: not installed; use `occ ensure` or `occ codex-shim install` for autostart"
            );
        }
        ServiceCommand::Uninstall => {
            let _ = cmd_stop().await;
            println!("openclaude service compatibility mode: no OS service to uninstall");
        }
    }
    Ok(())
}

fn cmd_shim(command: ShimCommand) -> anyhow::Result<()> {
    match command {
        ShimCommand::Install => integration::install_claude_shim(),
        ShimCommand::Status => {
            println!(
                "{}",
                if integration::shim_state_path()?.exists() {
                    "Claude shim installed"
                } else {
                    "Claude shim not installed"
                }
            );
            Ok(())
        }
        ShimCommand::Uninstall => integration::uninstall_claude_shim(),
    }
}

async fn cmd_status(json_output: bool) -> anyhow::Result<()> {
    let path = config_path()?;
    match Config::load() {
        Ok(cfg) => {
            let healthy = gateway_healthy(&cfg).await;
            let runtime = config::read_runtime().ok();
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "proxy": {
                            "health": { "ok": healthy },
                            "pid": runtime.as_ref().map(|state| state.pid),
                            "host": cfg.host,
                            "port": cfg.port,
                        },
                        "dashboard": {
                            "url": format!("http://{}:{}", cfg.host, cfg.port),
                        },
                        "paths": {
                            "config": path,
                            "runtime": config::runtime_path()?,
                        },
                        "runtime": {
                            "source": if runtime.is_some() { "file" } else { "none" },
                        },
                        "defaultProvider": cfg.default_provider,
                        "providers": cfg.providers.len(),
                        "claudeShim": {
                            "installed": integration::shim_state_path()?.exists(),
                            "summary": if integration::shim_state_path()?.exists() { "Claude shim installed" } else { "Claude shim not installed" },
                        },
                    }))?
                );
                return Ok(());
            }
            println!("config: {}", path.display());
            println!("listen: http://{}:{}", cfg.host, cfg.port);
            println!("default provider: {}", cfg.default_provider);
            println!("providers: {}", cfg.providers.len());
            if healthy {
                if let Some(runtime) = runtime {
                    println!(
                        "runtime: pid {} at http://{}:{}",
                        runtime.pid, runtime.host, runtime.port
                    );
                } else {
                    println!("runtime: running, no runtime record");
                }
            } else if config::read_runtime().is_ok() {
                let _ = config::remove_runtime();
                println!("runtime: stale record removed");
            } else {
                println!("runtime: not running");
            }
            println!(
                "claude mode: {}",
                if integration::shim_state_path()?.exists() {
                    "openclaude shim"
                } else {
                    "native/no shim"
                }
            );
        }
        Err(err) if path.exists() => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": false,
                        "error": format!("{err:#}"),
                        "paths": { "config": path },
                    }))?
                );
            } else {
                println!("config error: {err:#}");
            }
        }
        Err(_) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": false,
                        "error": "config missing",
                        "paths": { "config": path },
                    }))?
                );
            } else {
                println!("config missing; run: occ init");
            }
        }
    }
    Ok(())
}

fn cmd_env() -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    println!("export ANTHROPIC_BASE_URL=http://{}:{}", cfg.host, cfg.port);
    println!("unset ANTHROPIC_AUTH_TOKEN");
    println!("export ANTHROPIC_API_KEY={}", cfg.gateway_token);
    println!("export CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1");
    for (name, value) in cfg.claude_model_env() {
        println!("export {name}={}", shell_export_quote(&value));
    }
    Ok(())
}

fn shell_export_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn cmd_doctor() -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    println!("openclaude doctor");
    println!("config: {}", config_path()?.display());
    println!("gateway: http://{}:{}", cfg.host, cfg.port);
    println!("providers:");
    for (name, provider) in &cfg.providers {
        println!("  - {name}: {} {}", provider.adapter, provider.base_url);
    }
    println!();
    println!("Claude subscription OAuth is native-only. Use `occ native` for subscription Claude.");
    println!(
        "Use openclaude with provider API keys, Bedrock/Vertex, local models, or OpenAI-compatible endpoints."
    );
    Ok(())
}

fn cmd_login(provider: Option<String>) -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    let Some(provider) = provider else {
        anyhow::bail!("Usage: occ login <provider>");
    };
    let Some(p) = cfg.providers.get(&provider) else {
        anyhow::bail!("unknown provider: {provider}");
    };
    if let Some(entry) = registry_provider(&provider) {
        match entry.auth_kind {
            AuthKind::Forward => {
                println!(
                    "`{provider}` uses Codex/ChatGPT forward login in ocx. Claude Code subscription OAuth is native-only; use `occ native` for subscription Claude or choose an API-key provider."
                );
                return Ok(());
            }
            AuthKind::Oauth => {
                println!(
                    "`{provider}` uses ocx OAuth. openclaude does not store or reuse those OAuth tokens for Claude Code; choose an API-key variant when available."
                );
                return Ok(());
            }
            AuthKind::Key | AuthKind::Local => {}
        }
    }
    match p.resolve_api_key() {
        Some(_) => println!("provider `{provider}` has a usable API key or env reference"),
        None => println!(
            "provider `{provider}` has no resolved API key; set api_key or the referenced environment variable"
        ),
    }
    Ok(())
}

fn cmd_logout(provider: Option<String>) -> anyhow::Result<()> {
    let Some(provider) = provider else {
        anyhow::bail!("Usage: occ logout <provider>");
    };
    println!(
        "No stored OAuth token exists for `{provider}`. Remove or edit provider api_key with `occ provider remove/show/add`."
    );
    Ok(())
}

fn cmd_sync() -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    cfg.save()?;
    println!(
        "openclaude config synced for Claude Code gateway: {}",
        config_path()?.display()
    );
    Ok(())
}

fn cmd_sync_cache() -> anyhow::Result<()> {
    println!(
        "Claude Code model discovery is live through /v1/models; no local model cache to refresh."
    );
    Ok(())
}

fn cmd_gui() -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    let url = format!("http://{}:{}", cfg.host, cfg.port);
    println!("OpenClaude dashboard is not built yet. Gateway URL: {url}");
    Ok(())
}

fn cmd_update(tag: Option<String>) -> anyhow::Result<()> {
    let tag = tag.unwrap_or_else(|| "latest".to_string());
    println!(
        "Update openclaude with npm: npm install -g openclaudecode{}",
        if tag == "latest" { "" } else { "@preview" }
    );
    Ok(())
}

async fn cmd_restart() -> anyhow::Result<()> {
    let _ = cmd_stop().await;
    cmd_ensure().await
}

async fn cmd_health(json_output: bool) -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    let ok = gateway_healthy(&cfg).await;
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&json!({
                "ok": ok,
                "host": cfg.host,
                "port": cfg.port
            }))?
        );
    } else if ok {
        println!("Gateway healthy at http://{}:{}", cfg.host, cfg.port);
    } else {
        println!("Gateway not healthy at http://{}:{}", cfg.host, cfg.port);
    }
    if ok {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn cmd_recover_history(legacy_openai: bool) -> anyhow::Result<()> {
    if !legacy_openai {
        anyhow::bail!("Usage: occ recover-history --legacy-openai");
    }
    println!("Claude Code history is native-owned; openclaude does not remap history rows.");
    Ok(())
}

fn registry_provider(name: &str) -> Option<InitProvider> {
    init_providers()
        .into_iter()
        .find(|provider| provider.id == name)
}

fn auth_kind_label(kind: AuthKind) -> &'static str {
    match kind {
        AuthKind::Forward => "forward",
        AuthKind::Oauth => "oauth",
        AuthKind::Key => "key",
        AuthKind::Local => "local",
    }
}

fn provider_source(name: &str) -> &'static str {
    if registry_provider(name).is_some() {
        "registry"
    } else {
        "custom"
    }
}

fn masked_provider_json(
    name: &str,
    provider: &ProviderConfig,
    is_default: bool,
) -> anyhow::Result<serde_json::Value> {
    let mut value = serde_json::to_value(provider)?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("name".to_string(), json!(name));
        obj.insert("isDefault".to_string(), json!(is_default));
        if let Some(api_key) = obj.get("api_key").and_then(|value| value.as_str()) {
            obj.insert("api_key".to_string(), json!(mask_secret(api_key)));
        }
    }
    Ok(value)
}

fn mask_secret(value: &str) -> String {
    if value.starts_with("${") && value.ends_with('}') {
        return value.to_string();
    }
    if value.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}****{}", &value[..4], &value[value.len() - 4..])
    }
}

fn validate_config_for_save(cfg: &Config) -> anyhow::Result<()> {
    if cfg.providers.is_empty() {
        anyhow::bail!("config would have no providers. Aborting.");
    }
    if !cfg.providers.contains_key(&cfg.default_provider) {
        anyhow::bail!(
            "default provider `{}` does not exist in providers. Aborting.",
            cfg.default_provider
        );
    }
    Ok(())
}

fn cmd_provider(command: ProviderCommand) -> anyhow::Result<()> {
    let mut cfg = Config::load_or_create()?;
    match command {
        ProviderCommand::List { json } => {
            if json {
                let configured: Vec<_> = cfg
                    .providers
                    .iter()
                    .map(|(name, provider)| {
                        let registry = registry_provider(name);
                        json!({
                            "name": name,
                            "adapter": provider.adapter,
                            "baseUrl": provider.base_url,
                            "authMode": registry.map(|entry| auth_kind_label(entry.auth_kind)).unwrap_or("key"),
                            "defaultModel": provider.default_model,
                            "isDefault": cfg.default_provider == *name,
                            "source": provider_source(name),
                            "models": provider.models,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "configured": configured,
                        "registryCount": init_providers().len()
                    }))?
                );
                return Ok(());
            }
            println!("Configured providers:\n");
            for (name, provider) in &cfg.providers {
                let default = if cfg.default_provider == *name {
                    " (default)"
                } else {
                    ""
                };
                let custom = if provider_source(name) == "custom" {
                    " [custom]"
                } else {
                    ""
                };
                let model = provider
                    .default_model
                    .as_ref()
                    .map(|model| format!(" model={model}"))
                    .unwrap_or_default();
                println!(
                    "  {name}{default}{custom}  adapter={}{}",
                    provider.adapter, model
                );
            }
            let available: Vec<_> = init_providers()
                .into_iter()
                .filter(|provider| !cfg.providers.contains_key(provider.id))
                .collect();
            if !available.is_empty() {
                println!("\nAvailable from registry ({}):\n", available.len());
                for provider in available {
                    println!(
                        "  {:<24} {}  ({})",
                        provider.id,
                        provider.label,
                        auth_kind_label(provider.auth_kind)
                    );
                }
                println!("\nAdd with: occ provider add <name> [--api-key <key>]");
            }
        }
        ProviderCommand::Add {
            name,
            adapter,
            base_url,
            api_key,
            default_model,
            model,
            make_default,
            force,
            json,
            sync,
        } => {
            validate_provider_name(&name)?;
            if cfg.providers.contains_key(&name) && !force {
                anyhow::bail!("Provider `{name}` already exists. Use --force to overwrite.");
            }

            let registry = registry_provider(&name);
            let mut provider = if let Some(entry) = registry {
                let key = match entry.auth_kind {
                    AuthKind::Key => {
                        api_key.unwrap_or_else(|| format!("${{{}}}", env_key_for(entry.id)))
                    }
                    AuthKind::Forward | AuthKind::Oauth | AuthKind::Local => {
                        api_key.unwrap_or_default()
                    }
                };
                let mut provider = provider_config_from_init(
                    entry,
                    key,
                    default_model.clone().unwrap_or_default(),
                    Vec::new(),
                );
                if let Some(adapter) = adapter {
                    provider.adapter = adapter.into();
                }
                if let Some(base_url) = base_url {
                    provider.base_url = base_url;
                }
                provider
            } else {
                let Some(adapter) = adapter else {
                    anyhow::bail!(
                        "Provider `{name}` is not in the registry. --adapter and --base-url are required."
                    );
                };
                let Some(base_url) = base_url else {
                    anyhow::bail!(
                        "Provider `{name}` is not in the registry. --adapter and --base-url are required."
                    );
                };
                ProviderConfig {
                    adapter: adapter.into(),
                    base_url,
                    api_key,
                    default_model,
                    models: Vec::new(),
                    context_window: None,
                    model_context_windows: Default::default(),
                    model_input_modalities: Default::default(),
                    reasoning_efforts: None,
                    model_reasoning_efforts: Default::default(),
                    no_vision_models: Vec::new(),
                }
            };
            if !model.is_empty() {
                provider.models = model;
            }
            enrich_provider_metadata(&name, &mut provider);
            cfg.providers.insert(name.clone(), provider);
            if make_default {
                cfg.default_provider = name.clone();
            }
            validate_config_for_save(&cfg)?;
            cfg.save()?;
            if json {
                let provider = cfg.providers.get(&name).expect("provider inserted");
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "action": "added",
                        "provider": name,
                        "adapter": provider.adapter,
                        "baseUrl": provider.base_url,
                        "defaultModel": provider.default_model,
                        "isDefault": cfg.default_provider == name,
                        "source": registry.map(|_| "registry").unwrap_or("custom"),
                        "needsSync": true,
                    }))?
                );
                return Ok(());
            }
            let registry_label = registry
                .map(|entry| format!(" ({})", entry.label))
                .unwrap_or_default();
            println!("Provider `{name}`{registry_label} added.");
            if make_default {
                println!("Set as default provider.");
            }
            if let Some(entry) = registry {
                match entry.auth_kind {
                    AuthKind::Forward | AuthKind::Oauth => {
                        println!(
                            "Authentication note: `{}` is {} in ocx; Claude Code cannot reuse that OAuth/subscription token.",
                            entry.id,
                            auth_kind_label(entry.auth_kind)
                        );
                    }
                    AuthKind::Key if cfg.providers[&name].api_key.is_none() => {
                        println!(
                            "Set API key with: occ provider add {name} --api-key <key> --force"
                        );
                    }
                    _ => {}
                }
            }
            if sync {
                println!("Models synced to Claude Code gateway config.");
            } else {
                println!("Apply to a running gateway: occ sync");
            }
        }
        ProviderCommand::Remove { name, json } => {
            if !cfg.providers.contains_key(&name) {
                anyhow::bail!("Provider `{name}` is not configured.");
            }
            if cfg.default_provider == name {
                anyhow::bail!(
                    "Cannot remove `{name}` because it is the default provider. Change the default first: occ provider set-default <other>"
                );
            }
            if cfg.providers.len() <= 1 {
                anyhow::bail!("Cannot remove the last provider.");
            }
            cfg.providers.remove(&name);
            validate_config_for_save(&cfg)?;
            cfg.save()?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "action": "removed",
                        "provider": name,
                        "remainingProviders": cfg.providers.keys().collect::<Vec<_>>(),
                        "defaultProvider": cfg.default_provider,
                        "needsSync": true,
                    }))?
                );
            } else {
                println!("Provider `{name}` removed.");
            }
        }
        ProviderCommand::Show { name, json } => {
            let Some(provider) = cfg.providers.get(&name) else {
                anyhow::bail!("Provider `{name}` is not configured.");
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&masked_provider_json(
                        &name,
                        provider,
                        cfg.default_provider == name
                    )?)?
                );
            } else {
                println!(
                    "Provider: {name}{}",
                    if cfg.default_provider == name {
                        " (default)"
                    } else {
                        ""
                    }
                );
                println!("  adapter:      {}", provider.adapter);
                println!("  baseUrl:      {}", provider.base_url);
                if let Some(entry) = registry_provider(&name) {
                    println!("  authMode:     {}", auth_kind_label(entry.auth_kind));
                }
                if let Some(api_key) = &provider.api_key {
                    println!("  apiKey:       {}", mask_secret(api_key));
                }
                if let Some(default_model) = &provider.default_model {
                    println!("  defaultModel: {default_model}");
                }
                if !provider.models.is_empty() {
                    println!("  models:       {}", provider.models.join(", "));
                }
            }
        }
        ProviderCommand::SetDefault { name, json } => {
            if !cfg.providers.contains_key(&name) {
                anyhow::bail!(
                    "Provider `{name}` is not configured. Add it first: occ provider add {name}"
                );
            }
            if cfg.default_provider == name {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "action": "noop",
                            "provider": name,
                            "defaultProvider": name,
                            "needsSync": false,
                        }))?
                    );
                } else {
                    println!("`{name}` is already the default provider.");
                }
                return Ok(());
            }
            cfg.default_provider = name.clone();
            validate_config_for_save(&cfg)?;
            cfg.save()?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "action": "set-default",
                        "provider": name,
                        "defaultProvider": name,
                        "needsSync": true,
                    }))?
                );
            } else {
                println!("Default provider set to `{name}`.");
            }
        }
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelEntry {
    provider: String,
    model: String,
    is_default: bool,
    context_window: Option<u64>,
    input_modalities: Option<Vec<String>>,
    reasoning_efforts: Option<Vec<String>>,
}

fn collect_model_entries(
    cfg: &Config,
    provider_filter: Option<&str>,
) -> anyhow::Result<Vec<ModelEntry>> {
    if let Some(provider) = provider_filter
        && !cfg.providers.contains_key(provider)
    {
        anyhow::bail!("Provider \"{provider}\" is not configured. See: occ provider list");
    }
    let mut entries = Vec::new();
    for (provider_name, provider) in &cfg.providers {
        if provider_filter
            .map(|filter| filter != provider_name)
            .unwrap_or(false)
        {
            continue;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut add = |model: &str, is_default: bool| {
            if !seen.insert(model.to_string()) {
                return;
            }
            let no_vision = provider.no_vision_models.iter().any(|m| m == model);
            let modalities = provider
                .model_input_modalities
                .get(model)
                .cloned()
                .or_else(|| no_vision.then(|| vec!["text".to_string()]));
            let efforts = provider
                .model_reasoning_efforts
                .get(model)
                .cloned()
                .or_else(|| provider.reasoning_efforts.clone());
            entries.push(ModelEntry {
                provider: provider_name.clone(),
                model: model.to_string(),
                is_default,
                context_window: provider
                    .model_context_windows
                    .get(model)
                    .copied()
                    .or(provider.context_window),
                input_modalities: modalities,
                reasoning_efforts: efforts,
            });
        };

        if let Some(default_model) = &provider.default_model {
            add(default_model, true);
        }
        for model in &provider.models {
            add(
                model,
                provider.default_model.as_deref() == Some(model.as_str()),
            );
        }
    }
    Ok(entries)
}

fn cmd_models(provider_filter: Option<String>, json_output: bool) -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    let models = collect_model_entries(&cfg, provider_filter.as_deref())?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "models": models,
                "note": "Static config models only. Providers may have additional models at runtime."
            }))?
        );
        return Ok(());
    }

    if models.is_empty() {
        println!("No models found in configured providers.");
        if provider_filter.is_none() {
            println!("Providers may discover models dynamically at runtime.");
        }
        return Ok(());
    }

    let mut grouped: std::collections::BTreeMap<String, Vec<ModelEntry>> =
        std::collections::BTreeMap::new();
    for entry in models {
        grouped
            .entry(entry.provider.clone())
            .or_default()
            .push(entry);
    }

    for (provider, entries) in grouped {
        let default_provider = if cfg.default_provider == provider {
            " (default provider)"
        } else {
            ""
        };
        println!("{provider}{default_provider}:");
        for entry in entries {
            let marker = if entry.is_default { " *" } else { "" };
            let ctx = entry
                .context_window
                .map(|value| format!(" ({}k)", value / 1000))
                .unwrap_or_default();
            println!("  {}{}{}", entry.model, marker, ctx);
        }
        println!();
    }
    println!("* = default model for provider");
    println!("Note: providers may have additional models at runtime.");
    Ok(())
}
