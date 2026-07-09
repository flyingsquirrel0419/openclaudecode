# claude-occ

This npm package installs the `occ` launcher for claude-occ. It also exposes `claude-occ` as a
package-name alias.

claude-occ is a local Claude Code gateway proxy. It starts a local `occ` daemon, injects
`ANTHROPIC_BASE_URL` / `ANTHROPIC_API_KEY` for Claude Code, and routes Claude Messages API traffic to
configured providers.

## Install

```bash
npm install -g claude-occ
occ init
claude
```

For source checkouts:

```bash
cargo build --release
npm install -g ./npm
```

See the repository README for provider setup, native-mode behavior, security notes, and development
commands.

Localized documentation: [English](https://github.com/flyingsquirrel0419/claude-occ#readme),
[한국어](https://github.com/flyingsquirrel0419/claude-occ/blob/main/README.ko.md),
[简体中文](https://github.com/flyingsquirrel0419/claude-occ/blob/main/README.zh-CN.md).
