#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMPDIR_OCC="$(mktemp -d)"
cleanup() {
  if [[ -n "${PID:-}" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${FAKE_PID:-}" ]]; then
    kill "$FAKE_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMPDIR_OCC"
}
trap cleanup EXIT

export CLAUDE_OCC_HOME="$TMPDIR_OCC"

node <<'NODE' >"$TMPDIR_OCC/fake-openai.log" 2>&1 &
const http = require("node:http");

const server = http.createServer((req, res) => {
  if (req.method !== "POST" || req.url !== "/v1/chat/completions") {
    res.writeHead(404).end("not found");
    return;
  }
  let raw = "";
  req.on("data", chunk => raw += chunk);
  req.on("end", () => {
    const body = JSON.parse(raw || "{}");
    if (body.stream) {
      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
      });
      res.write(`data: ${JSON.stringify({ choices: [{ delta: { content: "fake " } }] })}\n\n`);
      res.write(`data: ${JSON.stringify({ choices: [{ delta: { content: "stream ok" } }] })}\n\n`);
      res.write("data: [DONE]\n\n");
      res.end();
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({
      id: "chatcmpl_fake",
      object: "chat.completion",
      choices: [{
        index: 0,
        message: { role: "assistant", content: "fake upstream ok" },
        finish_reason: "stop"
      }],
      usage: { prompt_tokens: 9, completion_tokens: 4, total_tokens: 13 }
    }));
  });
});

server.listen(19113, "127.0.0.1", () => {
  console.log("fake OpenAI upstream listening");
});
NODE
FAKE_PID="$!"

for _ in $(seq 1 50); do
  if grep -q "fake OpenAI upstream listening" "$TMPDIR_OCC/fake-openai.log" 2>/dev/null; then
    break
  fi
  sleep 0.1
done

cargo run --manifest-path "$ROOT/Cargo.toml" -- init --reset >/dev/null
cargo run --manifest-path "$ROOT/Cargo.toml" -- provider add fake \
  --adapter openai-chat \
  --base-url http://127.0.0.1:19113/v1 \
  --default-model fake-model \
  --model fake-model \
  --make-default >/dev/null
TOKEN="$(node -e "console.log(require(process.argv[1]).gateway_token)" "$TMPDIR_OCC/config.json")"
cargo run --manifest-path "$ROOT/Cargo.toml" -- start --port 19112 >"$TMPDIR_OCC/server.log" 2>&1 &
PID="$!"

for _ in $(seq 1 50); do
  if curl -fsS http://127.0.0.1:19112/healthz >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

curl -fsS -H "x-api-key: $TOKEN" http://127.0.0.1:19112/v1/models >"$TMPDIR_OCC/models.json"
node -e '
const body = require(process.argv[1]);
const ids = body.data?.map(m => m.id) ?? [];
if (!ids.includes("fake/fake-model")) throw new Error("gateway model discovery missing fake/fake-model");
' "$TMPDIR_OCC/models.json"
curl -fsS \
  -H "x-api-key: $TOKEN" \
  -H "content-type: application/json" \
  http://127.0.0.1:19112/v1/messages/count_tokens \
  -d '{"model":"openrouter/openai/gpt-5","messages":[{"role":"user","content":"hello"}]}' >/dev/null

curl -fsS \
  -H "x-api-key: $TOKEN" \
  -H "content-type: application/json" \
  http://127.0.0.1:19112/v1/messages \
  -d '{"model":"fake/fake-model","max_tokens":64,"messages":[{"role":"user","content":"hello"}]}' \
  >"$TMPDIR_OCC/message.json"
node -e '
const body = require(process.argv[1]);
if (body.type !== "message") throw new Error("not an Anthropic message");
if (body.content?.[0]?.text !== "fake upstream ok") throw new Error("unexpected content");
if (body.usage?.input_tokens !== 9 || body.usage?.output_tokens !== 4) throw new Error("usage not mapped");
' "$TMPDIR_OCC/message.json"

curl -fsS \
  -H "x-api-key: $TOKEN" \
  -H "content-type: application/json" \
  http://127.0.0.1:19112/v1/messages \
  -d '{"model":"fake/fake-model","max_tokens":64,"stream":true,"messages":[{"role":"user","content":"hello"}]}' \
  >"$TMPDIR_OCC/message.sse"
grep -q "event: message_start" "$TMPDIR_OCC/message.sse"
grep -q "fake " "$TMPDIR_OCC/message.sse"
grep -q "stream ok" "$TMPDIR_OCC/message.sse"

cargo run --manifest-path "$ROOT/Cargo.toml" -- stop >/dev/null

echo "smoke ok"
