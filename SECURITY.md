# Security Policy

## Supported Versions

openclaude is pre-1.0. Security fixes target the latest `main` branch and the newest npm/release
version when releases are available.

## Reporting a Vulnerability

Do not open a public issue for credential leaks, token exposure, or remote-access bypasses.

Until a dedicated security advisory channel exists, contact the maintainer privately through the
repository owner profile or use a private fork/patch handoff. Include:

- affected version or commit;
- operating system;
- whether the Claude shim was installed;
- relevant config shape with secrets redacted;
- minimal reproduction steps;
- impact assessment.

## Credential Boundaries

openclaude has two separate credential classes:

1. Provider credentials, stored in `~/.openclaude/config.json` as direct values or environment-variable
   references such as `${UMANS_API_KEY}`.
2. The generated local `gateway_token`, used by Claude Code to authenticate to the local openclaude
   gateway.

Treat both as secrets. Do not commit `~/.openclaude/config.json`, shell histories containing direct
keys, or logs with auth headers.

On Unix systems, openclaude writes config and runtime files with owner-only permissions (`0600`).
Keep parent directories and shell environment files under the same access discipline.

## Claude Code Subscription OAuth

Claude Code subscription OAuth is native-only. openclaude intentionally does not extract, forward, or
reuse Claude Code subscription tokens as upstream provider credentials.

Use:

```bash
occ native
```

when you want Claude Code's built-in subscription behavior. `occ native` stops the proxy and removes
the launcher shim.

## Local Gateway Exposure

By default openclaude binds to `127.0.0.1` and requires the generated gateway token for `/v1/*` and
`/api/*` routes. `GET /healthz` is intentionally unauthenticated for process checks.

Avoid binding to `0.0.0.0` unless you have reviewed the risk and network controls. If you expose the
gateway beyond loopback, the gateway token becomes a network credential.

## Shim Safety

`occ enable` / `occ codex-shim install` wraps the `claude` launcher so it can run `occ ensure`, export
gateway environment variables, and then execute the real Claude Code binary.

Use:

```bash
occ native
```

to remove the shim and stop the proxy. Use:

```bash
occ restore back
```

to re-enable openclaude routing.

## Secret Handling Guidelines

- Prefer `${ENV_VAR}` references over pasted API keys.
- Keep `.env` files out of git.
- Do not run with shell tracing (`set -x`) around `occ env`.
- Redact `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, provider API keys, and `gateway_token` in bug
  reports.
- Rotate provider keys if a config file or shell transcript is shared accidentally.
- Store npm publishing credentials only as the GitHub Actions `NPM_TOKEN` repository secret; never
  commit `.npmrc` files or token-bearing release logs.
