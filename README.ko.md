# claude-occ

<p align="center">
  <a href="README.md">English</a> · 한국어 · <a href="README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <b>Claude Code를 내가 고른 provider gateway로 라우팅합니다.</b>
</p>

<p align="center">
  <code>occ init</code> · <code>claude</code> · <code>occ native</code> · <b>localhost:10110</b>
</p>

<p align="center">
  <img src="assets/occ-banner.png" alt="Claude Code 요청을 로컬 occ gateway를 통해 provider API로 보내는 occ 배너" width="820">
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/claude-occ"><img src="https://img.shields.io/npm/v/claude-occ?logo=npm&label=npm" alt="npm package version"></a>
  <a href="https://github.com/flyingsquirrel0419/claude-occ/releases/latest"><img src="https://img.shields.io/github/v/release/flyingsquirrel0419/claude-occ?logo=github" alt="latest GitHub release"></a>
  <img src="https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/github/license/flyingsquirrel0419/claude-occ" alt="MIT license">
</p>

claude-occ는 Claude Code용 로컬 gateway proxy입니다. Claude Code는 로컬 `occ` daemon과
Anthropic Messages API로 통신하고, `occ`는 요청을 Anthropic 호환, OpenAI 호환, Google,
Azure, 로컬, 커스텀 provider로 라우팅합니다.

[opencodex](https://github.com/lidge-jun/opencodex)와는 의도적으로 분리된 프로젝트입니다.
opencodex는 Codex와 OpenAI Responses API를 대상으로 하고, claude-occ는 Claude Code와
`ANTHROPIC_BASE_URL` / `ANTHROPIC_API_KEY` 연동 표면을 대상으로 합니다.

> Claude Code 구독 OAuth는 native-only입니다. claude-occ는 Claude Code 구독 토큰을
> upstream credential로 추출하거나 재사용하지 않습니다. Claude Code의 기본 구독 모드를
> 쓰고 싶다면 `occ native`를 사용하세요.

## 왜 필요한가

Claude Code는 이미 Anthropic Messages API를 사용합니다. claude-occ는 Claude Code와 원하는
upstream 사이에 작은 인증 proxy를 두어 다음을 가능하게 합니다.

- `${UMANS_API_KEY}` 같은 환경변수 참조나 provider API key 사용
- Claude Code의 `/model` picker에 라우팅된 모델 노출
- OpenAI 호환 로컬 서버를 Claude Code에서 사용
- 한 명령으로 native Claude Code 모드 복귀
- loopback 바인딩과 로컬 gateway token으로 proxy 보호

<p align="center">
  <img src="assets/occ-flow.png" alt="Claude Code에서 occ를 거쳐 provider로 요청이 흐르는 다이어그램" width="820">
</p>

## 빠른 시작

필요한 것:

- `claude` 명령으로 실행 가능한 Claude Code
- npm 설치용 Node.js 18 이상, 또는 소스 빌드용 Rust stable

npm으로 설치:

```bash
npm install -g claude-occ
```

소스 checkout에서 설치:

```bash
cargo build --release
npm install -g ./npm
```

provider 설정:

```bash
occ init
```

`occ init`은 대화형 설정입니다. 오래된 proxy를 먼저 중지하고, provider를 고르게 하며,
API key를 직접 값이나 환경변수 참조로 저장하고, 모델 선택과 Claude autostart shim 설치를
진행합니다. shim을 설치하면 `claude` 실행 시 `occ ensure`가 먼저 실행되고, 로컬 gateway
환경변수를 주입한 뒤 실제 Claude Code binary를 실행합니다.

평소처럼 Claude Code를 실행합니다.

```bash
claude
```

shim 없이 실행하려면:

```bash
occ ensure
eval "$(occ env)"
claude
```

native Claude Code로 돌아가려면:

```bash
occ native
```

`occ native`는 claude-occ proxy를 중지하고 `claude` shim을 제거해 Claude Code 기본 동작을
복구합니다. 나중에 다시 켜려면:

```bash
occ restore back
```

## 주요 명령

```bash
occ init                         # 대화형 설정
occ start [--port 10110]         # 로컬 gateway 시작
occ ensure                       # 필요하면 시작, stale token gateway 교체
occ stop                         # gateway 중지
occ restart                      # 중지 후 재시작
occ status [--json]              # config path, shim 상태, proxy health 출력
occ health [--json]              # /v1/models를 통한 인증 health check
occ env                          # 수동 Claude Code 라우팅용 shell export 출력
occ enable                       # Claude autostart shim 설치
occ native                       # proxy 중지 및 native Claude Code 복구
occ restore back                 # native mode 이후 shim 재활성화
occ uninstall                    # shim/runtime/config 제거

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

`occ codex-shim ...`은 opencodex 호환 alias로 남아 있지만, 이 프로젝트에서는 Claude Code
launcher shim을 관리합니다. `occ claude-shim ...`도 alias로 사용할 수 있습니다.

## Provider

`occ init`에는 opencodex 스타일의 provider picker가 포함되어 있습니다.

| Provider family | Adapter | Auth style |
|---|---|---|
| Umans AI Coding Plan | `anthropic` | API key 또는 env var |
| Anthropic API | `anthropic` | API key 또는 env var |
| OpenRouter | `openai-chat` | API key 또는 env var |
| OpenAI API | `openai-chat` | API key 또는 env var |
| Google Gemini API | `google` | API key 또는 env var |
| Azure OpenAI | `azure-openai` | API key |
| DeepSeek, Groq, Together, Fireworks, Cerebras, Mistral, Hugging Face, NVIDIA NIM, MiniMax, Qwen Portal, Ollama Cloud 등 | `openai-chat` | API key 또는 env var |
| Ollama, vLLM, LM Studio | `openai-chat` | 로컬, 보통 빈 key |
| Custom OpenAI-compatible endpoint | `openai-chat` | 선택적 key |

opencodex의 OAuth/forward-login 항목은 Claude Code가 해당 credential을 안전하게 재사용할 수
없기 때문에 호환 stub으로만 존재합니다. `occ login <provider>`는 토큰을 추출하지 않고 이
경계를 설명합니다.

## 모델 발견

gateway는 다음 endpoint를 노출합니다.

```http
GET /v1/models
```

Claude Code에서는 설정된 모델이 `provider/model` 형태로 보입니다.

```text
umans/umans-coder
umans/umans-kimi-k2.7
umans/umans-glm-5.2
openrouter/anthropic/claude-sonnet-5
local/llama3
```

launcher shim은 Claude Code의 native model slot도 proxy model id로 덮어써 `/model`에서
native Claude family만이 아니라 라우팅된 모델을 볼 수 있게 합니다.

## 설정

config 위치:

```text
~/.claude-occ/config.json
```

예시:

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

API key는 literal string, `$ENV_VAR`, `${ENV_VAR}` 참조를 사용할 수 있습니다. 생성된
`gateway_token`은 Claude Code와 로컬 claude-occ gateway 사이에서만 사용됩니다.

## Gateway API

claude-occ는 Claude Code가 사용하는 Anthropic Messages API의 핵심 endpoint를 구현합니다.

| Endpoint | Purpose | Auth |
|---|---|---|
| `GET /healthz` | 인증 없는 process health | 없음 |
| `GET /v1/models` | 모델 발견 | `x-api-key` 또는 `Authorization: Bearer` |
| `POST /v1/messages` | non-streaming 및 streaming message | `x-api-key` 또는 `Authorization: Bearer` |
| `POST /v1/messages/count_tokens` | 로컬 token estimate | `x-api-key` 또는 `Authorization: Bearer` |
| `GET /api/config` | management summary | `x-api-key` 또는 `Authorization: Bearer` |
| `GET /api/providers` | 설정된 provider | `x-api-key` 또는 `Authorization: Bearer` |
| `POST /api/stop` | graceful local stop | `x-api-key` 또는 `Authorization: Bearer` |

기본적으로 gateway는 `127.0.0.1`에 바인딩됩니다. loopback이 아닌 주소에 바인딩할 때는
experimental로 보고, config file과 gateway token을 credential처럼 보호하세요.

## 소스 빌드

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

## 검증

```bash
cargo test
cargo clippy --all-targets -- -D warnings
./scripts/command-surface.sh
./scripts/smoke.sh
cargo build --release
```

`scripts/smoke.sh`는 fake OpenAI-compatible upstream을 시작하고, Claude Messages 요청을
claude-occ를 통해 보내 non-streaming 및 streaming 응답을 검증합니다.

## 프로젝트 구조

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

## 호환성 메모

- opencodex는 Codex와 `/v1/responses`를 대상으로 하고, claude-occ는 Claude Code와
  `/v1/messages`를 대상으로 합니다.
- `occ service`는 명령 호환성을 위해 존재하지만 현재 구현은 OS service manager가 아니라
  `occ ensure` / launcher-shim 방식입니다.
- `occ gui`는 현재 local dashboard URL만 출력하며, web dashboard는 아직 구현되지 않았습니다.
- `occ sync-cache`는 Claude Code에 Codex 같은 writable model cache가 없기 때문에 no-op입니다.

## 보안

[SECURITY.md](SECURITY.md)를 참고하세요. 요약하면:

- `~/.claude-occ/config.json`을 commit하지 마세요.
- claude-occ는 Unix에서 config와 runtime file을 owner-only 권한으로 씁니다.
- provider key는 환경변수 참조를 권장합니다.
- 검토 없이 gateway를 loopback 밖에 노출하지 마세요.
- Claude Code 구독 OAuth를 쓸 때는 `occ native`를 사용하세요.

## 기여

개발 환경, 테스트 명령, provider/adapter 가이드는 [CONTRIBUTING.md](CONTRIBUTING.md)를
참고하세요.

## 변경 기록

[CHANGELOG.md](CHANGELOG.md)를 참고하세요.

## 라이선스

MIT. [LICENSE](LICENSE)를 참고하세요.

## Disclaimer

claude-occ는 독립 커뮤니티 프로젝트이며 Anthropic, OpenAI, Umans 또는 특정 provider와
제휴하거나 보증받은 프로젝트가 아닙니다. 로컬 proxy로 traffic을 라우팅하기 전에 각 upstream
provider의 Terms of Service를 확인하세요.
