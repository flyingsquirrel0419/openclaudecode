#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Config, config_dir, ensure_parent};

const SHIM_MARKER: &str = "claude-occ claude autostart shim";

#[derive(Debug, Serialize, Deserialize)]
struct ShimState {
    wrapper_path: PathBuf,
    original_path: PathBuf,
    backup_path: PathBuf,
}

pub fn shim_state_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("claude-shim.json"))
}

pub fn install_claude_shim() -> anyhow::Result<()> {
    let cfg = Config::load_or_create()?;
    let claude = find_on_path("claude").context("could not find claude on PATH")?;
    if is_shim(&claude) {
        if let Ok(raw) = fs::read_to_string(shim_state_path()?)
            && let Ok(state) = serde_json::from_str::<ShimState>(&raw)
        {
            let occ = env::current_exe().context("resolve current occ executable")?;
            write_unix_shim(&claude, &state.backup_path, &occ, &cfg)?;
            println!("refreshed claude shim: {}", claude.display());
            return Ok(());
        }
        println!("claude shim already installed: {}", claude.display());
        return Ok(());
    }
    let backup = backup_path_for(&claude);
    if backup.exists() {
        bail!(
            "backup already exists at {}; refusing to overwrite",
            backup.display()
        );
    }
    fs::rename(&claude, &backup).with_context(|| format!("backup {}", claude.display()))?;
    let occ = env::current_exe().context("resolve current occ executable")?;
    write_unix_shim(&claude, &backup, &occ, &cfg)?;
    let state = ShimState {
        wrapper_path: claude.clone(),
        original_path: claude,
        backup_path: backup,
    };
    let state_path = shim_state_path()?;
    ensure_parent(&state_path)?;
    fs::write(&state_path, serde_json::to_string_pretty(&state)? + "\n")?;
    println!("installed claude shim. Use `occ native` to restore native Claude Code.");
    Ok(())
}

pub fn uninstall_claude_shim() -> anyhow::Result<()> {
    let state_path = shim_state_path()?;
    if !state_path.exists() {
        println!("no claude-occ claude shim state found; native mode already active");
        return Ok(());
    }
    let state: ShimState = serde_json::from_str(&fs::read_to_string(&state_path)?)?;
    if state.wrapper_path.exists() && is_shim(&state.wrapper_path) {
        fs::remove_file(&state.wrapper_path)?;
    }
    if state.backup_path.exists() {
        fs::rename(&state.backup_path, &state.original_path)?;
    }
    fs::remove_file(state_path)?;
    println!("restored native Claude Code launcher.");
    Ok(())
}

fn find_on_path(command: &str) -> Option<PathBuf> {
    for dir in env::split_paths(&env::var_os("PATH")?) {
        let path = dir.join(command);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn backup_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("claude");
    path.with_file_name(format!("{file_name}.claude-occ-real"))
}

fn is_shim(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|s| s.contains(SHIM_MARKER))
        .unwrap_or(false)
}

fn shell_quote(value: &Path) -> String {
    let raw = value.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\\''"))
}

fn shell_quote_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn write_unix_shim(
    wrapper: &Path,
    real_claude: &Path,
    occ: &Path,
    cfg: &Config,
) -> anyhow::Result<()> {
    #[cfg(not(unix))]
    {
        let _ = (wrapper, real_claude, occ, cfg);
        anyhow::bail!("claude shim is currently implemented for Unix-like systems only");
    }
    #[cfg(unix)]
    {
        let model_env = cfg
            .claude_model_env()
            .into_iter()
            .map(|(name, value)| format!("export {name}={}\n", shell_quote_str(&value)))
            .collect::<String>();
        let body = format!(
            r#"#!/usr/bin/env sh
# {SHIM_MARKER}
{occ} ensure >/dev/null 2>&1 || true
export ANTHROPIC_BASE_URL="http://{host}:{port}"
unset ANTHROPIC_AUTH_TOKEN
export ANTHROPIC_API_KEY="{token}"
export CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1
{model_env}
exec {real} "$@"
"#,
            occ = shell_quote(occ),
            host = cfg.host,
            port = cfg.port,
            token = cfg.gateway_token,
            model_env = model_env,
            real = shell_quote(real_claude),
        );
        fs::write(wrapper, body)?;
        let mut perms = fs::metadata(wrapper)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(wrapper, perms)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AdapterKind, ProviderConfig};
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn shell_quote_str_escapes_single_quotes() {
        assert_eq!(shell_quote_str("abc'def"), "'abc'\\''def'");
    }

    #[test]
    fn backup_path_uses_claude_occ_real_suffix() {
        assert_eq!(
            backup_path_for(Path::new("/usr/bin/claude")),
            PathBuf::from("/usr/bin/claude.claude-occ-real")
        );
    }

    #[test]
    fn detects_existing_shim_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude");
        fs::write(&path, format!("#!/bin/sh\n# {SHIM_MARKER}\n")).unwrap();
        assert!(is_shim(&path));
    }

    #[cfg(unix)]
    #[test]
    fn writes_unix_shim_with_gateway_env_and_executable_mode() {
        let dir = tempfile::tempdir().unwrap();
        let wrapper = dir.path().join("claude");
        let real = dir.path().join("claude.real");
        let occ = dir.path().join("occ");
        fs::write(&real, "#!/bin/sh\n").unwrap();
        fs::write(&occ, "#!/bin/sh\n").unwrap();

        let mut providers = BTreeMap::new();
        providers.insert(
            "test".to_string(),
            ProviderConfig {
                adapter: AdapterKind::OpenAiChat,
                base_url: "https://example.test/v1".to_string(),
                api_key: Some("${TEST_API_KEY}".to_string()),
                default_model: Some("model-a".to_string()),
                models: vec!["model-a".to_string(), "model-b".to_string()],
                context_window: None,
                model_context_windows: HashMap::new(),
                model_input_modalities: HashMap::new(),
                reasoning_efforts: None,
                model_reasoning_efforts: HashMap::new(),
                no_vision_models: Vec::new(),
            },
        );
        let cfg = Config {
            host: "127.0.0.1".to_string(),
            port: 10110,
            gateway_token: "occ_test".to_string(),
            default_provider: "test".to_string(),
            providers,
        };

        write_unix_shim(&wrapper, &real, &occ, &cfg).unwrap();
        let body = fs::read_to_string(&wrapper).unwrap();
        assert!(body.contains("unset ANTHROPIC_AUTH_TOKEN"));
        assert!(body.contains("export ANTHROPIC_API_KEY=\"occ_test\""));
        assert!(body.contains("ANTHROPIC_DEFAULT_SONNET_MODEL='test/model-a'"));
        assert!(body.contains("exec "));

        let mode = fs::metadata(&wrapper).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }
}
