# Contributing to openclaude

openclaude is a Rust CLI and local gateway for Claude Code. Keep changes small, verified, and tied to
observable Claude Code behavior.

## Development Setup

Prerequisites:

- Rust stable
- Node.js 18+ when touching the npm wrapper
- Claude Code installed if you are testing the launcher shim manually

```bash
cd openclaude
cargo test
cargo build --release
```

For npm wrapper checks:

```bash
cd npm
node ./bin/postinstall.js
node ./bin/occ.js --version
npm pack --dry-run
```

## Verification Commands

Run the narrowest command that proves your change:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
./scripts/command-surface.sh
./scripts/smoke.sh
cargo build --release
```

`scripts/command-surface.sh` checks the CLI contract, config init behavior, model-slot environment
exports, and provider command shapes. `scripts/smoke.sh` starts a fake OpenAI-compatible upstream and
verifies gateway translation through `/v1/messages`.

## Repository Layout

```text
src/main.rs          CLI commands, init flow, provider registry, process lifecycle
src/config.rs        config format and Claude Code model-slot mapping
src/integration.rs   Claude launcher shim
src/server.rs        Axum gateway endpoints
src/providers.rs     route resolution and provider adapters
src/models.rs        Anthropic Messages wire structures
npm/                 npm launcher and release-binary installer
scripts/             command-surface and smoke tests
```

## Coding Conventions

- Prefer the current Rust style and `cargo fmt`.
- Keep config changes backward-compatible where possible.
- Use structured JSON parsing and serde types instead of ad-hoc string parsing.
- Do not log provider API keys, gateway tokens, OAuth tokens, or raw auth headers.
- Preserve native Claude Code behavior behind `occ native`.
- Treat Claude Code subscription OAuth as native-only; do not add token extraction or token reuse.

## Adding a Provider

Most provider catalog changes are entries in `init_providers()` in `src/main.rs`.

Choose the adapter that matches the upstream wire format:

| Adapter | Use for |
|---|---|
| `anthropic` | Anthropic Messages-compatible endpoints |
| `openai-chat` | OpenAI Chat Completions-compatible endpoints |
| `azure-openai` | Azure OpenAI-compatible deployments |
| `google` | Gemini API-style model endpoints |
| `cursor`, `kiro` | declared compatibility surfaces; keep experimental behavior explicit |

Provider entries should include:

- stable provider id;
- human-readable label;
- base URL;
- auth style;
- dashboard/key URL when available;
- default model and static model fallback when known.

If a provider exposes `/models`, prefer live discovery during `occ init`, but keep a static fallback
for offline setup.

## Adding or Changing an Adapter

Adapter behavior lives in `src/providers.rs`. A safe adapter change should cover:

- non-streaming `/v1/messages`;
- streaming response mapping when supported;
- usage token mapping when upstream provides it;
- tool call or tool result behavior if the upstream model supports tools;
- clear errors when a provider cannot support a Claude Code request.

Add or update focused tests in the existing Rust test modules and run `./scripts/smoke.sh` when the
request path changes.

## Documentation Changes

Update `README.md` when user-facing commands, config fields, provider behavior, or native/shim
semantics change. Update `CHANGELOG.md` for notable changes.

When documenting setup, prefer environment-variable references:

```bash
occ provider add umans \
  --adapter anthropic \
  --base-url https://api.code.umans.ai \
  --api-key '${UMANS_API_KEY}' \
  --default-model umans-coder \
  --model umans-coder,umans-flash \
  --set-default
```

## Pull Request Checklist

- [ ] `cargo fmt`
- [ ] `cargo test`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] Relevant smoke or command-surface script
- [ ] README or CHANGELOG updated when behavior changed
- [ ] No credentials, tokens, or machine-local secrets in the diff
- [ ] Native Claude Code behavior still works through `occ native`
