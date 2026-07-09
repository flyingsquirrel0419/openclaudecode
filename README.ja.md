# claude-occ

<p align="center">
  <a href="README.md">English</a> · <a href="README.ko.md">한국어</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.es.md">Español</a> · 日本語
</p>

<p align="center">
  <b>Claude Code を自分の provider gateway 経由でルーティングします。</b>
</p>

<p align="center">
  <code>occ init</code> · <code>claude</code> · <code>occ native</code> · <b>localhost:10110</b>
</p>

<p align="center">
  <img src="assets/occ-banner.png" alt="Claude Code のリクエストをローカル occ gateway から provider API へルーティングするバナー" width="820">
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/claude-occ"><img src="https://img.shields.io/npm/v/claude-occ?logo=npm&label=npm" alt="npm package version"></a>
  <a href="https://github.com/flyingsquirrel0419/claude-occ/releases/latest"><img src="https://img.shields.io/github/v/release/flyingsquirrel0419/claude-occ?logo=github" alt="latest GitHub release"></a>
  <img src="https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/github/license/flyingsquirrel0419/claude-occ" alt="MIT license">
</p>

claude-occ は Claude Code 向けのローカル gateway proxy です。Claude Code は Anthropic
Messages API でローカルの `occ` daemon と通信し、`occ` はリクエストを Anthropic 互換、
OpenAI 互換、Google、Azure、ローカル、またはカスタム provider にルーティングします。

[opencodex](https://github.com/lidge-jun/opencodex) とは意図的に分離したプロジェクトです。
opencodex は Codex と OpenAI Responses API を対象にし、claude-occ は Claude Code と
`ANTHROPIC_BASE_URL` / `ANTHROPIC_API_KEY` の統合面を対象にします。

> Claude Code のサブスクリプション OAuth は native-only のままです。claude-occ は
> Claude Code のサブスクリプショントークンを upstream credential として抽出したり再利用
> したりしません。Claude Code の組み込みサブスクリプションモードを使う場合は
> `occ native` を使ってください。

## なぜ

Claude Code はすでに Anthropic Messages API を話します。claude-occ は Claude Code と
選択した upstream の間に小さな認証付き proxy を置くことで、次のことを可能にします。

- `${UMANS_API_KEY}` のような環境変数参照や provider API key を使う。
- Claude Code の `/model` picker にルーティング済みモデルを表示する。
- OpenAI 互換のローカルサーバーを Claude Code から使う。
- 1 コマンドで native Claude Code に戻す。
- gateway を loopback に置き、生成されたローカル gateway token で Claude Code を認証する。

<p align="center">
  <img src="assets/occ-flow.png" alt="Claude Code から occ を経由して設定済み provider へ流れるリクエスト図" width="820">
</p>

## クイックスタート

前提:

- Claude Code がインストールされ、`claude` として利用できること。
- npm インストールには Node.js 18+、ソースビルドには Rust stable。

npm からインストール:

```bash
npm install -g claude-occ
```

ソース checkout からインストール:

```bash
cargo build --release
npm install -g ./npm
```

provider を設定:

```bash
occ init
```

`occ init` は対話式です。`~/.claude-occ/config.json` を書き込み、古い proxy を先に停止し、
provider を選ばせ、API key を直接値または環境変数参照として保存し、モデル選択と Claude
autostart shim のインストールを案内します。shim がデフォルトの経路です。`claude` を実行
すると、先に `occ ensure` を実行し、ローカル gateway 用の環境変数を注入してから本物の
Claude Code binary を起動します。

通常どおり Claude Code を使います:

```bash
claude
```

shim なしで実行する場合:

```bash
occ ensure
eval "$(occ env)"
claude
```

native Claude Code に戻る場合:

```bash
occ native
```

`occ native` は claude-occ proxy を停止し、`claude` shim を削除して Claude Code の native
動作を復元します。あとで再度ルーティングを有効にするには:

```bash
occ restore back
```

## よく使うコマンド

```bash
occ init                         # 対話式セットアップ
occ start [--port 10110]         # ローカル gateway を開始
occ ensure                       # 必要なら開始し、stale-token gateway を置き換える
occ stop                         # gateway を停止
occ restart                      # 停止して再起動
occ status [--json]              # config path、shim 状態、proxy health を表示
occ health [--json]              # /v1/models 経由の認証付き health check
occ env                          # 手動 Claude Code ルーティング用の shell exports を出力
occ enable                       # Claude autostart shim をインストール
occ native                       # proxy を停止し native Claude Code を復元
occ restore back                 # native mode 後に shim を再有効化
occ uninstall                    # shim/runtime/config を削除

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

`occ codex-shim ...` は opencodex 互換 alias として残していますが、このプロジェクトでは
Claude Code launcher shim を管理します。`occ claude-shim ...` も alias として利用できます。

## Providers

`occ init` には opencodex に着想を得た registry-style の provider picker が含まれます。

| Provider family | Adapter | Auth style |
|---|---|---|
| Umans AI Coding Plan | `anthropic` | API key または env var |
| Anthropic API | `anthropic` | API key または env var |
| OpenRouter | `openai-chat` | API key または env var |
| OpenAI API | `openai-chat` | API key または env var |
| Google Gemini API | `google` | API key または env var |
| Azure OpenAI | `azure-openai` | API key |
| DeepSeek, Groq, Together, Fireworks, Cerebras, Mistral, Hugging Face, NVIDIA NIM, MiniMax, Qwen Portal, Ollama Cloud など | `openai-chat` | API key または env var |
| Ollama, vLLM, LM Studio | `openai-chat` | local、通常は空 key |
| Custom OpenAI-compatible endpoint | `openai-chat` | optional key |

opencodex の OAuth/forward-login エントリは、Claude Code がそれらの credential を安全に再利用
できない場合の互換 stub としてのみ存在します。`occ login <provider>` は token を抽出せず、
その境界を説明します。

## モデル検出

gateway は次を公開します:

```http
GET /v1/models
```

Claude Code では設定済みモデルが `provider/model` として見えます。

```text
umans/umans-coder
umans/umans-kimi-k2.7
umans/umans-glm-5.2
openrouter/anthropic/claude-sonnet-5
local/llama3
```

launcher shim は Claude Code の native model slots も proxy model id で上書きするため、
`/model` で native Claude family だけでなくルーティング済みモデルを表示できます。

## 設定

config の場所:

```text
~/.claude-occ/config.json
```

例:

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

API key には literal string、`$ENV_VAR`、`${ENV_VAR}` 参照を使えます。生成された
`gateway_token` は Claude Code とローカル claude-occ gateway の間だけで使われます。

## Gateway API

claude-occ は Claude Code-facing な Anthropic Messages API の主要部分を実装します。

| Endpoint | Purpose | Auth |
|---|---|---|
| `GET /healthz` | 認証なし process health | なし |
| `GET /v1/models` | モデル検出 | `x-api-key` または `Authorization: Bearer` |
| `POST /v1/messages` | non-streaming / streaming messages | `x-api-key` または `Authorization: Bearer` |
| `POST /v1/messages/count_tokens` | ローカル token estimate | `x-api-key` または `Authorization: Bearer` |
| `GET /api/config` | management summary | `x-api-key` または `Authorization: Bearer` |
| `GET /api/providers` | 設定済み providers | `x-api-key` または `Authorization: Bearer` |
| `POST /api/stop` | graceful local stop | `x-api-key` または `Authorization: Bearer` |

デフォルトでは gateway は `127.0.0.1` に bind します。loopback 以外への bind は
experimental として扱い、config file と gateway token を credential と同様に保護してください。

## ソースからビルド

```bash
cd claude-occ
cargo test
cargo build --release
./target/release/occ --help
```

npm package smoke check:

```bash
cd npm
node ./bin/postinstall.js
node ./bin/occ.js --version
npm pack --dry-run
```

## 検証

```bash
cargo test
cargo clippy --all-targets -- -D warnings
./scripts/command-surface.sh
./scripts/smoke.sh
cargo build --release
```

`scripts/smoke.sh` は fake OpenAI-compatible upstream を起動し、Claude Messages request を
claude-occ 経由で送り、non-streaming と streaming の response を検証します。

## プロジェクト構成

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

## 互換性メモ

- opencodex は Codex と `/v1/responses` を対象にし、claude-occ は Claude Code と
  `/v1/messages` を対象にします。
- `occ service` はコマンド互換性のために存在しますが、現在の実装は OS service manager ではなく
  `occ ensure` / launcher-shim の動作を使います。
- `occ gui` は現在ローカル dashboard URL を表示します。web dashboard はまだ実装されていません。
- `occ sync-cache` は、Claude Code に Codex のような書き込み可能な model cache がないため no-op です。

## セキュリティ

[SECURITY.md](SECURITY.md) を参照してください。要約:

- `~/.claude-occ/config.json` を commit しないでください。
- claude-occ は Unix で config と runtime files を owner-only 権限で書き込みます。
- provider key には環境変数参照を推奨します。
- exposure を確認していない限り gateway は loopback に維持してください。
- Claude Code サブスクリプション OAuth を使う場合は `occ native` を使ってください。

## コントリビュート

開発環境、test command、provider/adapter ガイドは [CONTRIBUTING.md](CONTRIBUTING.md) を
参照してください。

## Changelog

[CHANGELOG.md](CHANGELOG.md) を参照してください。

## ライセンス

MIT。[LICENSE](LICENSE) を参照してください。

## Disclaimer

claude-occ は独立したコミュニティプロジェクトであり、Anthropic、OpenAI、Umans、または
いかなる provider とも提携または承認されていません。ローカル proxy 経由で traffic を
ルーティングする前に、各 upstream provider の Terms of Service を確認してください。
