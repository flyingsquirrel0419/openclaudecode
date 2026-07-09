# claude-occ

<p align="center">
  <a href="README.md">English</a> · <a href="README.ko.md">한국어</a> · <a href="README.zh-CN.md">简体中文</a> · Español · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <b>Claude Code, enrutado a través de tu propio provider gateway.</b>
</p>

<p align="center">
  <code>occ init</code> · <code>claude</code> · <code>occ native</code> · <b>localhost:10110</b>
</p>

<p align="center">
  <img src="assets/occ-banner.png" alt="banner de occ que muestra Claude Code enrutado por un gateway local hacia APIs de providers" width="820">
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/claude-occ"><img src="https://img.shields.io/npm/v/claude-occ?logo=npm&label=npm" alt="npm package version"></a>
  <a href="https://github.com/flyingsquirrel0419/claude-occ/releases/latest"><img src="https://img.shields.io/github/v/release/flyingsquirrel0419/claude-occ?logo=github" alt="latest GitHub release"></a>
  <img src="https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/github/license/flyingsquirrel0419/claude-occ" alt="MIT license">
</p>

claude-occ es un gateway proxy local para Claude Code. Hace que Claude Code hable con un daemon
local `occ` mediante la Anthropic Messages API, mientras `occ` enruta las solicitudes hacia
providers compatibles con Anthropic, compatibles con OpenAI, Google, Azure, modelos locales o
endpoints personalizados.

Es un proyecto separado de [opencodex](https://github.com/lidge-jun/opencodex) de forma
intencional. opencodex apunta a Codex y a OpenAI Responses API; claude-occ apunta a Claude Code y a
la superficie de integración `ANTHROPIC_BASE_URL` / `ANTHROPIC_API_KEY`.

> El OAuth de suscripción de Claude Code se mantiene native-only. claude-occ no extrae ni reutiliza
> tokens de suscripción de Claude Code como credenciales upstream. Usa `occ native` cuando quieras
> volver al modo de suscripción integrado de Claude Code.

## Por Qué

Claude Code ya habla la Anthropic Messages API. claude-occ coloca un proxy autenticado pequeño entre
Claude Code y el upstream que elijas para que puedas:

- usar API keys de providers o referencias a variables de entorno como `${UMANS_API_KEY}`;
- exponer modelos enrutados en el selector `/model` de Claude Code;
- ejecutar servidores locales compatibles con OpenAI desde Claude Code;
- volver a Claude Code nativo con un solo comando;
- mantener el gateway en loopback y autenticar Claude Code con un gateway token local generado.

<p align="center">
  <img src="assets/occ-flow.png" alt="flujo de solicitudes de occ desde Claude Code hacia providers configurados" width="820">
</p>

## Inicio Rápido

Requisitos:

- Claude Code instalado y disponible como `claude`.
- Node.js 18+ para instalación con npm, o Rust stable para compilar desde fuente.

Instalar desde npm:

```bash
npm install -g claude-occ
```

Instalar desde un checkout de fuente:

```bash
cargo build --release
npm install -g ./npm
```

Configurar un provider:

```bash
occ init
```

`occ init` es interactivo. Escribe `~/.claude-occ/config.json`, detiene primero cualquier proxy
obsoleto, pide un provider, guarda API keys como valores directos o referencias a variables de
entorno, permite elegir un modelo y ofrece instalar el Claude autostart shim. El shim es el camino
por defecto: cuando ejecutas `claude`, primero ejecuta `occ ensure`, inyecta el entorno del gateway
local y luego lanza el binario real de Claude Code.

Usa Claude Code normalmente:

```bash
claude
```

O ejecútalo sin shim:

```bash
occ ensure
eval "$(occ env)"
claude
```

Volver a Claude Code nativo:

```bash
occ native
```

`occ native` detiene el proxy de claude-occ, elimina el shim de `claude` y restaura el
comportamiento nativo de Claude Code. Para reactivar el enrutamiento después:

```bash
occ restore back
```

## Comandos Comunes

```bash
occ init                         # configuración interactiva
occ start [--port 10110]         # iniciar el gateway local
occ ensure                       # iniciar si hace falta; reemplaza gateways con token obsoleto
occ stop                         # detener el gateway
occ restart                      # detener e iniciar el gateway
occ status [--json]              # mostrar config path, estado del shim y salud del proxy
occ health [--json]              # health check autenticado a través de /v1/models
occ env                          # imprimir exports para enrutar Claude Code manualmente
occ enable                       # instalar el Claude autostart shim
occ native                       # detener proxy y restaurar Claude Code nativo
occ restore back                 # reactivar el shim después del modo nativo
occ uninstall                    # eliminar shim/runtime/config

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

`occ codex-shim ...` se conserva como alias compatible con opencodex, pero en este proyecto gestiona
el launcher shim de Claude Code. `occ claude-shim ...` también está disponible como alias.

## Providers

`occ init` incluye un selector de providers estilo registry inspirado por opencodex.

| Provider family | Adapter | Auth style |
|---|---|---|
| Umans AI Coding Plan | `anthropic` | API key o env var |
| Anthropic API | `anthropic` | API key o env var |
| OpenRouter | `openai-chat` | API key o env var |
| OpenAI API | `openai-chat` | API key o env var |
| Google Gemini API | `google` | API key o env var |
| Azure OpenAI | `azure-openai` | API key |
| DeepSeek, Groq, Together, Fireworks, Cerebras, Mistral, Hugging Face, NVIDIA NIM, MiniMax, Qwen Portal, Ollama Cloud y más | `openai-chat` | API key o env var |
| Ollama, vLLM, LM Studio | `openai-chat` | local, normalmente key vacía |
| Custom OpenAI-compatible endpoint | `openai-chat` | key opcional |

Las entradas OAuth/forward-login de opencodex existen solo como stubs de compatibilidad cuando
Claude Code no puede reutilizar esas credenciales de forma segura. `occ login <provider>` explica
ese límite en lugar de extraer tokens.

## Descubrimiento De Modelos

El gateway expone:

```http
GET /v1/models
```

Claude Code ve los modelos configurados como `provider/model`, por ejemplo:

```text
umans/umans-coder
umans/umans-kimi-k2.7
umans/umans-glm-5.2
openrouter/anthropic/claude-sonnet-5
local/llama3
```

El launcher shim también sobrescribe los slots de modelos nativos de Claude Code con ids de modelos
proxy para que `/model` pueda mostrar modelos enrutados y no solo familias nativas de Claude.

## Configuración

La configuración vive en:

```text
~/.claude-occ/config.json
```

Ejemplo:

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

Las API keys pueden ser strings literales, `$ENV_VAR` o referencias `${ENV_VAR}`. El
`gateway_token` generado solo se usa entre Claude Code y el gateway local de claude-occ.

## Gateway API

claude-occ implementa las partes de Anthropic Messages API que mira Claude Code:

| Endpoint | Purpose | Auth |
|---|---|---|
| `GET /healthz` | salud del proceso sin autenticación | ninguna |
| `GET /v1/models` | descubrimiento de modelos | `x-api-key` o `Authorization: Bearer` |
| `POST /v1/messages` | mensajes non-streaming y streaming | `x-api-key` o `Authorization: Bearer` |
| `POST /v1/messages/count_tokens` | estimación local de tokens | `x-api-key` o `Authorization: Bearer` |
| `GET /api/config` | resumen de gestión | `x-api-key` o `Authorization: Bearer` |
| `GET /api/providers` | providers configurados | `x-api-key` o `Authorization: Bearer` |
| `POST /api/stop` | parada local ordenada | `x-api-key` o `Authorization: Bearer` |

Por defecto el gateway se enlaza a `127.0.0.1`. Trata cualquier binding fuera de loopback como
experimental y protege el archivo de configuración y el gateway token como credenciales.

## Compilar Desde Fuente

```bash
cd claude-occ
cargo test
cargo build --release
./target/release/occ --help
```

Smoke checks del paquete npm:

```bash
cd npm
node ./bin/postinstall.js
node ./bin/occ.js --version
npm pack --dry-run
```

## Verificación

```bash
cargo test
cargo clippy --all-targets -- -D warnings
./scripts/command-surface.sh
./scripts/smoke.sh
cargo build --release
```

`scripts/smoke.sh` inicia un upstream falso compatible con OpenAI, envía solicitudes Claude Messages
a través de claude-occ y verifica respuestas non-streaming y streaming.

## Estructura Del Proyecto

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

## Notas De Compatibilidad

- opencodex apunta a Codex y `/v1/responses`; claude-occ apunta a Claude Code y `/v1/messages`.
- `occ service` existe por compatibilidad de comandos, pero la implementación actual usa
  `occ ensure` / launcher-shim en lugar de un service manager del sistema operativo.
- `occ gui` actualmente imprime la URL del dashboard local; aún no hay dashboard web.
- `occ sync-cache` es un no-op de compatibilidad porque Claude Code no tiene una cache de modelos
  escribible como Codex.

## Seguridad

Consulta [SECURITY.md](SECURITY.md). En resumen:

- no hagas commit de `~/.claude-occ/config.json`;
- claude-occ escribe archivos de config y runtime con permisos solo para el owner en Unix;
- prefiere referencias a variables de entorno para keys de providers;
- mantén el gateway en loopback salvo que hayas revisado la exposición;
- usa `occ native` para el OAuth de suscripción de Claude Code.

## Contribuir

Consulta [CONTRIBUTING.md](CONTRIBUTING.md) para el entorno de desarrollo, comandos de test y guías
de providers/adapters.

## Changelog

Consulta [CHANGELOG.md](CHANGELOG.md).

## Licencia

MIT. Consulta [LICENSE](LICENSE).

## Disclaimer

claude-occ es un proyecto comunitario independiente y no está afiliado ni respaldado por Anthropic,
OpenAI, Umans ni ningún provider. Revisa los Terms of Service de cada upstream antes de enrutar
tráfico por un proxy local.
