# Changelog

All notable changes to claude-occ are documented here.

This project follows a pragmatic changelog: CLI details may evolve, but security boundaries and
native-mode behavior should remain explicit.

## [1.0.0] - 2026-07-09

### Added

- Rust CLI binary `occ`.
- npm wrapper package metadata for the `occ` and `claude-occ` commands.
- Interactive `occ init` with an opencodex-style provider picker.
- Single-file config at `~/.claude-occ/config.json`.
- Local gateway endpoints:
  - `GET /healthz`
  - `GET /v1/models`
  - `POST /v1/messages`
  - `POST /v1/messages/count_tokens`
  - `GET /api/config`
  - `GET /api/providers`
  - `POST /api/stop`
- Provider management commands:
  - `occ provider list`
  - `occ provider add`
  - `occ provider remove`
  - `occ provider show`
  - `occ provider set-default`
- Model listing with `occ models` and `occ models --json`.
- Claude launcher shim with `occ enable`, `occ codex-shim install`, and `occ claude-shim install`.
- Native restore path with `occ native`, `occ restore`, and `occ eject`.
- Local process lifecycle commands: `occ start`, `occ stop`, `occ ensure`, `occ restart`, `occ health`.
- Authenticated gateway health checks through `/v1/models` to avoid stale-token false positives.
- Stale proxy cleanup during `occ init`, `occ start`, and `occ ensure`.
- Claude Code `/model` slot environment overrides backed by configured `provider/model` ids.
- Umans provider metadata for context windows, text-only models, and reasoning effort hints.
- OpenAI-compatible chat translation smoke test with streaming and non-streaming coverage.

### Security

- `occ env` and the Claude shim unset `ANTHROPIC_AUTH_TOKEN` before routing through claude-occ.
- Gateway data-plane and management endpoints require the generated local gateway token.
- Claude Code subscription OAuth is treated as native-only and is not reused by claude-occ.
- Config and runtime files are written with owner-only permissions on Unix.
- Gateway token checks use constant-time comparison.
- Upstream error responses redact configured provider API keys before returning errors to the client.

### Changed

- `occ native` now stops the proxy before restoring native Claude Code.
- `occ init` now stops an existing proxy before writing a new config and refreshes an installed shim
  after config changes.
- Added PR/main CI for fmt, clippy, tests, command-surface, smoke, release build, and npm dry-run.
- Release workflow now fails early with a clear message when the `NPM_TOKEN` repository secret is
  missing.

### Known limitations

- `occ service` is compatibility-mode only; it delegates to `occ ensure` behavior rather than
  installing a full OS service.
- `occ gui` prints the local gateway URL; a web dashboard is not implemented yet.
- `occ sync-cache` is a compatibility no-op because Claude Code has no writable model cache like
  Codex.
