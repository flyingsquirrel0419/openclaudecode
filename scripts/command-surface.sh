#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMPDIR_OCC="$(mktemp -d)"
TMPDIR_RESET="$(mktemp -d)"
cleanup() {
  rm -rf "$TMPDIR_OCC" "$TMPDIR_RESET"
}
trap cleanup EXIT

export CLAUDE_OCC_HOME="$TMPDIR_OCC"
unset UMANS_API_KEY DEEPSEEK_API_KEY
OCC=(cargo run --quiet --manifest-path "$ROOT/Cargo.toml" --)

printf '8\n\n\n10110\nn\n' | "${OCC[@]}" init >/dev/null
node - "$TMPDIR_OCC/config.json" <<'NODE'
const fs = require('node:fs');
const config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
if (config.default_provider !== 'umans') throw new Error('interactive init did not select umans');
if (config.providers.umans.api_key !== '${UMANS_API_KEY}') throw new Error('blank key did not save UMANS_API_KEY env reference');
if (config.providers.umans.default_model !== 'umans-coder') throw new Error('default model was not saved');
if (config.port !== 10110) throw new Error('default port was not saved');
NODE
touch "$TMPDIR_OCC/config.json.bak.old"
printf '8\n\n\n10110\nn\n' | "${OCC[@]}" init >/tmp/occ-init-repeat.log
if grep -q "previous config backed up" /tmp/occ-init-repeat.log; then
  echo "init printed backup path unexpectedly" >&2
  exit 1
fi
if [[ ! -f "$TMPDIR_OCC/config.json.bak" ]]; then
  echo "init did not create single backup file" >&2
  exit 1
fi
if [[ "$(find "$TMPDIR_OCC" -maxdepth 1 -name 'config.json.bak*' | wc -l)" != "1" ]]; then
  echo "init kept more than one backup file" >&2
  exit 1
fi
CLAUDE_OCC_HOME="$TMPDIR_RESET" "${OCC[@]}" init --reset >/dev/null
if [[ ! -f "$TMPDIR_RESET/config.json" ]]; then
  echo "init --reset did not create config" >&2
  exit 1
fi

commands=(
  "status"
  "status --json"
  "doctor"
  "env"
  "models"
  "models --json"
  "models --provider umans"
  "provider list"
  "provider list --json"
  "provider show umans"
  "provider show umans --json"
  "provider set-default umans"
  "provider set-default umans --json"
  "provider add openrouter --force --json"
  "provider add deepseek --api-key \${DEEPSEEK_API_KEY} --set-default --force --sync"
  "provider add alias-test --adapter openai-chat --base-url http://127.0.0.1:9/v1 --default-model test --model test --set-default"
  "sync"
  "sync-cache"
  "gui"
  "update"
  "login umans"
  "logout umans"
  "codex-shim status"
  "claude-shim status"
  "service status"
  "recover-history --legacy-openai"
  "version"
)

for command in "${commands[@]}"; do
  # shellcheck disable=SC2206
  args=($command)
  "${OCC[@]}" "${args[@]}" >/dev/null
done

"${OCC[@]}" health --json >/tmp/occ-health-command-surface.json || true
node - <<'NODE'
const fs = require('node:fs');
const body = JSON.parse(fs.readFileSync('/tmp/occ-health-command-surface.json', 'utf8'));
if (typeof body.ok !== 'boolean') throw new Error('health --json missing ok boolean');
NODE

"${OCC[@]}" status --json >/tmp/occ-status-command-surface.json
node - <<'NODE'
const fs = require('node:fs');
const body = JSON.parse(fs.readFileSync('/tmp/occ-status-command-surface.json', 'utf8'));
if (!body.proxy || !body.paths || typeof body.defaultProvider !== 'string') throw new Error('status --json shape mismatch');
NODE

"${OCC[@]}" env >/tmp/occ-env-command-surface.sh
grep -q '^unset ANTHROPIC_AUTH_TOKEN$' /tmp/occ-env-command-surface.sh
grep -q '^export ANTHROPIC_API_KEY=' /tmp/occ-env-command-surface.sh
grep -q '^export ANTHROPIC_MODEL=' /tmp/occ-env-command-surface.sh
grep -q '^export ANTHROPIC_DEFAULT_SONNET_MODEL=' /tmp/occ-env-command-surface.sh
grep -q '^export ANTHROPIC_DEFAULT_OPUS_MODEL=' /tmp/occ-env-command-surface.sh
grep -q '^export ANTHROPIC_DEFAULT_HAIKU_MODEL=' /tmp/occ-env-command-surface.sh
grep -q '^export ANTHROPIC_DEFAULT_FABLE_MODEL=' /tmp/occ-env-command-surface.sh
grep -q '^export ANTHROPIC_CUSTOM_MODEL_OPTION=' /tmp/occ-env-command-surface.sh
node - <<'NODE'
const fs = require('node:fs');
const raw = fs.readFileSync('/tmp/occ-env-command-surface.sh', 'utf8');
const read = (name) => {
  const match = raw.match(new RegExp(`^export ${name}='?([^'\\n]+)'?$`, 'm'));
  if (!match) throw new Error(`missing ${name}`);
  return match[1];
};
const slots = [
  'ANTHROPIC_DEFAULT_SONNET_MODEL',
  'ANTHROPIC_DEFAULT_OPUS_MODEL',
  'ANTHROPIC_DEFAULT_HAIKU_MODEL',
  'ANTHROPIC_DEFAULT_FABLE_MODEL',
  'ANTHROPIC_CUSTOM_MODEL_OPTION',
].map(read);
if (!slots.every(value => value.includes('/'))) {
  throw new Error('Claude model slots must use provider/model ids');
}
if (new Set(slots).size < 3) {
  throw new Error('Claude model slots collapsed to too few proxy models');
}
NODE

"${OCC[@]}" provider list --json >/tmp/occ-provider-list-command-surface.json
node - <<'NODE'
const fs = require('node:fs');
const body = JSON.parse(fs.readFileSync('/tmp/occ-provider-list-command-surface.json', 'utf8'));
if (!Array.isArray(body.configured)) throw new Error('provider list --json missing configured array');
if (typeof body.registryCount !== 'number' || body.registryCount < 30) throw new Error('provider registry count too small');
if (!body.configured.some(p => p.name === 'deepseek' && p.source === 'registry')) throw new Error('registry provider add was not reflected');
NODE

"${OCC[@]}" models --json >/tmp/occ-models-command-surface.json
node - <<'NODE'
const fs = require('node:fs');
const body = JSON.parse(fs.readFileSync('/tmp/occ-models-command-surface.json', 'utf8'));
if (!Array.isArray(body.models)) throw new Error('models --json missing models array');
if (!body.models.every(m => typeof m.provider === 'string' && typeof m.model === 'string' && typeof m.isDefault === 'boolean')) {
  throw new Error('models --json shape does not match expected ocx-style entries');
}
NODE

"${OCC[@]}" restore >/dev/null
"${OCC[@]}" eject >/dev/null
"${OCC[@]}" --help >/dev/null
"${OCC[@]}" -v >/dev/null
"${OCC[@]}" help provider >/dev/null
"${OCC[@]}" uninstall --help >/dev/null
"${OCC[@]}" remove --help >/dev/null

echo "command surface ok"
