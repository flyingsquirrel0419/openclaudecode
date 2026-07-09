# claude-occ

<p align="center">
  <a href="README.md">English</a> · <a href="README.ko.md">한국어</a> · 简体中文 · <a href="README.es.md">Español</a> · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <b>让 Claude Code 通过你自己的 provider gateway 路由请求。</b>
</p>

<p align="center">
  <code>occ init</code> · <code>claude</code> · <code>occ native</code> · <b>localhost:10110</b>
</p>

<p align="center">
  <img src="assets/occ-banner.png" alt="occ banner showing Claude Code routed through a local gateway to provider APIs" width="820">
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/claude-occ"><img src="https://img.shields.io/npm/v/claude-occ?logo=npm&label=npm" alt="npm package version"></a>
  <a href="https://github.com/flyingsquirrel0419/claude-occ/releases/latest"><img src="https://img.shields.io/github/v/release/flyingsquirrel0419/claude-occ?logo=github" alt="latest GitHub release"></a>
  <img src="https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/github/license/flyingsquirrel0419/claude-occ" alt="MIT license">
</p>

claude-occ 是一个面向 Claude Code 的本地 gateway proxy。Claude Code 通过 Anthropic
Messages API 连接本地 `occ` daemon，而 `occ` 会把请求路由到 Anthropic-compatible、
OpenAI-compatible、Google、Azure、本地或自定义 provider。

它有意与 [opencodex](https://github.com/lidge-jun/opencodex) 分开。opencodex 面向
Codex 和 OpenAI Responses API；claude-occ 面向 Claude Code 以及
`ANTHROPIC_BASE_URL` / `ANTHROPIC_API_KEY` 集成面。

> Claude Code 订阅 OAuth 保持 native-only。claude-occ 不会提取或复用 Claude Code
> 订阅 token 作为 upstream credential。需要使用 Claude Code 内置订阅模式时，请运行
> `occ native`。

## 为什么

Claude Code 已经使用 Anthropic Messages API。claude-occ 在 Claude Code 和你选择的
upstream 之间放置一个小型认证 proxy，因此你可以：

- 使用 provider API key，或 `${UMANS_API_KEY}` 这样的环境变量引用；
- 在 Claude Code 的 `/model` picker 中显示被路由的模型；
- 在 Claude Code 中使用 OpenAI-compatible 本地服务器；
- 用一条命令回到 native Claude Code；
- 将 gateway 保持在 loopback，并用生成的本地 gateway token 认证 Claude Code。

<p align="center">
  <img src="assets/occ-flow.png" alt="occ request flow from Claude Code to configured model providers" width="820">
</p>

## 快速开始

前置条件：

- 已安装 Claude Code，并且 `claude` 命令可用。
- npm 安装需要 Node.js 18+；从源码构建需要 Rust stable。

通过 npm 安装：

```bash
npm install -g claude-occ
```

从源码 checkout 安装：

```bash
cargo build --release
npm install -g ./npm
```

设置 provider：

```bash
occ init
```

`occ init` 是交互式配置流程。它会先停止过期 proxy，选择 provider，将 API key 保存为直接值
或环境变量引用，选择模型，并提供 Claude autostart shim。默认路径是安装 shim：运行
`claude` 时，它会先运行 `occ ensure`，注入本地 gateway 环境变量，然后启动真正的
Claude Code binary。

正常使用 Claude Code：

```bash
claude
```

不使用 shim 时：

```bash
occ ensure
eval "$(occ env)"
claude
```

回到 native Claude Code：

```bash
occ native
```

`occ native` 会停止 claude-occ proxy，移除 `claude` shim，并恢复 Claude Code 原生行为。
之后可用以下命令重新启用路由：

```bash
occ restore back
```

## 常用命令

```bash
occ init                         # 交互式设置
occ start [--port 10110]         # 启动本地 gateway
occ ensure                       # 按需启动；替换 stale-token gateway
occ stop                         # 停止 gateway
occ restart                      # 停止并重启 gateway
occ status [--json]              # 显示 config path、shim 状态和 proxy health
occ health [--json]              # 通过 /v1/models 做认证 health check
occ env                          # 输出手动路由 Claude Code 所需的 shell exports
occ enable                       # 安装 Claude autostart shim
occ native                       # 停止 proxy 并恢复 native Claude Code
occ restore back                 # native mode 后重新启用 shim
occ uninstall                    # 删除 shim/runtime/config

occ provider list [--json]
occ provider add umans --set-default
occ provider add openrouter --api-key '${OPENROUTER_API_KEY}' --set-default
occ provider add local \
  --adapter openai-chat \
  --base-url http://127.0.0.1:11434/v1 \
  --default-model llama3 \
  --model llama3 \
  --set-default
occ models [--json]
occ models --provider umans
```

`occ codex-shim ...` 保留为 opencodex-compatible alias，但在本项目中它管理 Claude Code
launcher shim。也可以使用 `occ claude-shim ...` 作为 alias。

## Providers

`occ init` 包含一个受 opencodex 启发的 registry-style provider picker。

| Provider family | Adapter | Auth style |
|---|---|---|
| Umans AI Coding Plan | `anthropic` | API key 或 env var |
| Anthropic API | `anthropic` | API key 或 env var |
| OpenRouter | `openai-chat` | API key 或 env var |
| OpenAI API | `openai-chat` | API key 或 env var |
| Google Gemini API | `google` | API key 或 env var |
| Azure OpenAI | `azure-openai` | API key |
| DeepSeek, Groq, Together, Fireworks, Cerebras, Mistral, Hugging Face, NVIDIA NIM, MiniMax, Qwen Portal, Ollama Cloud 等 | `openai-chat` | API key 或 env var |
| Ollama, vLLM, LM Studio | `openai-chat` | 本地，通常 key 为空 |
| Custom OpenAI-compatible endpoint | `openai-chat` | 可选 key |

opencodex 中的 OAuth/forward-login 条目只作为兼容 stub 存在，因为 Claude Code 不能安全复用
这些 credential。`occ login <provider>` 会解释这个边界，而不是提取 token。

## 模型发现

gateway 暴露：

```http
GET /v1/models
```

Claude Code 会以 `provider/model` 形式看到配置模型，例如：

```text
umans/umans-coder
umans/umans-kimi-k2.7
umans/umans-glm-5.2
openrouter/anthropic/claude-sonnet-5
local/llama3
```

launcher shim 也会用 proxy model id 覆盖 Claude Code 的 native model slots，因此 `/model`
可以显示被路由的模型，而不是只显示 native Claude families。

## 配置

配置文件位于：

```text
~/.claude-occ/config.json
```

示例：

```json
{
  "host": "127.0.0.1",
  "port": 10110,
  "gateway_token": "occ_generated_local_token",
  "default_provider": "umans",
  "providers": {
    "umans": {
      "adapter": "anthropic",
      "base_url": "https://api.code.umans.ai",
      "api_key": "${UMANS_API_KEY}",
      "default_model": "umans-coder",
      "models": [
        "umans-coder",
        "umans-kimi-k2.7",
        "umans-glm-5.2"
      ]
    }
  }
}
```

API key 可以是 literal string、`$ENV_VAR` 或 `${ENV_VAR}` 引用。生成的 `gateway_token`
只用于 Claude Code 和本地 claude-occ gateway 之间。

## Gateway API

claude-occ 实现了 Claude Code-facing 的 Anthropic Messages API 核心部分：

| Endpoint | Purpose | Auth |
|---|---|---|
| `GET /healthz` | 未认证 process health | 无 |
| `GET /v1/models` | 模型发现 | `x-api-key` 或 `Authorization: Bearer` |
| `POST /v1/messages` | non-streaming 和 streaming messages | `x-api-key` 或 `Authorization: Bearer` |
| `POST /v1/messages/count_tokens` | 本地 token estimate | `x-api-key` 或 `Authorization: Bearer` |
| `GET /api/config` | management summary | `x-api-key` 或 `Authorization: Bearer` |
| `GET /api/providers` | 已配置 providers | `x-api-key` 或 `Authorization: Bearer` |
| `POST /api/stop` | graceful local stop | `x-api-key` 或 `Authorization: Bearer` |

默认情况下 gateway 绑定到 `127.0.0.1`。非 loopback 绑定视为 experimental，请像保护
credential 一样保护 config file 和 gateway token。

## 从源码构建

```bash
cd claude-occ
cargo test
cargo build --release
./target/release/occ --help
```

npm package smoke check：

```bash
cd npm
node ./bin/postinstall.js
node ./bin/occ.js --version
npm pack --dry-run
```

## 验证

```bash
cargo test
cargo clippy --all-targets -- -D warnings
./scripts/command-surface.sh
./scripts/smoke.sh
cargo build --release
```

`scripts/smoke.sh` 会启动 fake OpenAI-compatible upstream，通过 claude-occ 发送 Claude
Messages 请求，并验证 non-streaming 和 streaming 响应。

## 项目结构

```text
src/main.rs          CLI, init flow, provider registry, process lifecycle
src/config.rs        config format, model slot mapping, Umans metadata
src/integration.rs   Claude launcher shim install/uninstall
src/server.rs        local Axum gateway endpoints
src/providers.rs     provider routing and protocol translation
src/models.rs        Anthropic Messages request/response structures
npm/                 npm wrapper and release-binary installer
scripts/             command-surface and gateway smoke tests
```

## 兼容性说明

- opencodex 面向 Codex 和 `/v1/responses`；claude-occ 面向 Claude Code 和 `/v1/messages`。
- `occ service` 为命令兼容性存在，但当前实现使用轻量的 `occ ensure` / launcher-shim
  行为，而不是 OS service manager。
- `occ gui` 目前只打印本地 dashboard URL；web dashboard 尚未实现。
- `occ sync-cache` 是 compatibility no-op，因为 Claude Code 没有像 Codex 那样可写的模型缓存。

## 安全

见 [SECURITY.md](SECURITY.md)。简要说明：

- 不要提交 `~/.claude-occ/config.json`；
- claude-occ 在 Unix 上用 owner-only 权限写入 config 和 runtime files；
- provider key 推荐使用环境变量引用；
- 除非已评估风险，否则保持 gateway 只绑定 loopback；
- 使用 Claude Code 订阅 OAuth 时运行 `occ native`。

## 贡献

开发环境、测试命令和 provider/adapter 指南见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 更新日志

见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

MIT。见 [LICENSE](LICENSE)。

## Disclaimer

claude-occ 是独立社区项目，不隶属于 Anthropic、OpenAI、Umans 或任何 provider，也不受其
背书。通过本地 proxy 路由流量前，请检查每个 upstream provider 的 Terms of Service。
