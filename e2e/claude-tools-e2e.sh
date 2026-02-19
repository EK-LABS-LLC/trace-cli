#!/usr/bin/env bash
set -euo pipefail

# ── Claude Code Tool & Subagent Coverage Test ────────────────────
# Forces tool calls + subagent spawning, then dumps every field
# from every span to see exactly what data we captured vs missed.

for var in ANTHROPIC_API_KEY PULSE_API_URL PULSE_API_KEY; do
  if [ -z "${!var:-}" ]; then
    echo "FAIL: $var is not set"
    exit 1
  fi
done

PASS=0
FAIL=0

assert_eq() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label (expected=$expected, actual=$actual)"
    FAIL=$((FAIL + 1))
  fi
}

assert_neq() {
  local label="$1" unexpected="$2" actual="$3"
  if [ "$unexpected" != "$actual" ]; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label (should not be $unexpected)"
    FAIL=$((FAIL + 1))
  fi
}

assert_gte() {
  local label="$1" min="$2" actual="$3"
  if [ "$actual" -ge "$min" ] 2>/dev/null; then
    echo "  PASS: $label ($actual >= $min)"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label (expected >= $min, actual=$actual)"
    FAIL=$((FAIL + 1))
  fi
}

query_spans() {
  curl -sf \
    -H "Authorization: Bearer $PULSE_API_KEY" \
    -H "X-Project-Id: e2e-cc-tools" \
    "$PULSE_API_URL/v1/spans?$1" 2>&1 || echo '{"spans":[]}'
}

extract_spans() {
  echo "$1" | jq 'if type == "array" then . elif .spans? then .spans elif .data? then .data else [] end'
}

export PULSE_DEBUG=1
export PULSE_DEBUG_LOG=/tmp/pulse-debug.log

# ── Setup ─────────────────────────────────────────────────────────
echo "── Setup: pulse init + connect"
pulse init \
  --api-url "$PULSE_API_URL" \
  --api-key "$PULSE_API_KEY" \
  --project-id "e2e-cc-tools" \
  --no-validate

mkdir -p ~/.claude
echo '{}' > ~/.claude/settings.json
pulse connect

# Create a test file for Claude to read
mkdir -p /workdir
echo "Hello from the test file." > /workdir/test.txt

# ── Run Claude Code with tool-forcing prompt ──────────────────────
echo ""
echo "── Step 1: Running Claude Code (tool calls + subagent)"

BEFORE_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# This prompt forces:
# 1. Read tool (read a file)
# 2. Bash tool (run a command)
# 3. Task tool / subagent (explore)
CLAUDE_OUTPUT=$(claude -p "Do these 3 things in order:
1. Read the file /workdir/test.txt using the Read tool
2. Run the bash command: echo TOOL_TEST_OK
3. Use the Task tool with subagent_type Explore to answer: what files exist in /workdir?
After all 3, reply with DONE." 2>&1 || true)
echo "Claude output (last 20 lines):"
echo "$CLAUDE_OUTPUT" | tail -20

# Wait for async spans
sleep 8

# ── Query spans ───────────────────────────────────────────────────
echo ""
echo "── Step 2: Querying spans"

RESPONSE=$(query_spans "limit=200")
ALL_SPANS=$(extract_spans "$RESPONSE")

SESSION_SPANS=$(echo "$ALL_SPANS" | jq --arg ts "$BEFORE_TS" \
  'map(select(.timestamp >= $ts and .source == "claude_code"))')
SESSION_COUNT=$(echo "$SESSION_SPANS" | jq 'length')

echo "  Spans from this session: $SESSION_COUNT"

# ── Verify event types captured ───────────────────────────────────
echo ""
echo "── Step 3: Event type coverage"

EVENT_TYPES=$(echo "$SESSION_SPANS" | jq -r '[.[].eventType] | unique | sort | .[]')
echo "  Event types captured:"
for et in $EVENT_TYPES; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" 'map(select(.eventType == $et)) | length')
  echo "    $et: $COUNT"
done

# Check for expected event types
for et in session_start user_prompt_submit pre_tool_use post_tool_use stop session_end; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" 'map(select(.eventType == $et)) | length')
  assert_gte "has at least 1 $et" 1 "$COUNT"
done

# Subagent events (may or may not fire depending on Claude's behavior)
for et in subagent_start subagent_stop; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" 'map(select(.eventType == $et)) | length')
  if [ "$COUNT" -ge 1 ]; then
    echo "  PASS: captured $et ($COUNT spans)"
    PASS=$((PASS + 1))
  else
    echo "  INFO: $et not captured (Claude may not have spawned a subagent)"
  fi
done

# ── Tool use span field audit ─────────────────────────────────────
echo ""
echo "── Step 4: Tool use span field audit"

# Get first pre_tool_use span
PRE_TOOL=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "pre_tool_use")) | .[0]')
if [ "$PRE_TOOL" != "null" ]; then
  echo "  pre_tool_use sample:"
  echo "    toolName:    $(echo "$PRE_TOOL" | jq -r '.toolName // "NULL"')"
  echo "    toolUseId:   $(echo "$PRE_TOOL" | jq -r '.toolUseId // "NULL"')"
  echo "    toolInput:   $(echo "$PRE_TOOL" | jq -c '.toolInput // "NULL"' | head -c 200)"
  echo "    model:       $(echo "$PRE_TOOL" | jq -r '.model // "NULL"')"
  echo "    cwd:         $(echo "$PRE_TOOL" | jq -r '.cwd // "NULL"')"
  echo "    agentName:   $(echo "$PRE_TOOL" | jq -r '.agentName // "NULL"')"

  assert_neq "pre_tool_use has toolName" "NULL" "$(echo "$PRE_TOOL" | jq -r '.toolName // "NULL"')"
  assert_neq "pre_tool_use has toolUseId" "NULL" "$(echo "$PRE_TOOL" | jq -r '.toolUseId // "NULL"')"
  assert_neq "pre_tool_use has toolInput" "NULL" "$(echo "$PRE_TOOL" | jq -r '.toolInput // "NULL"')"
fi

# Get first post_tool_use span
POST_TOOL=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "post_tool_use")) | .[0]')
if [ "$POST_TOOL" != "null" ]; then
  echo ""
  echo "  post_tool_use sample:"
  echo "    toolName:    $(echo "$POST_TOOL" | jq -r '.toolName // "NULL"')"
  echo "    toolUseId:   $(echo "$POST_TOOL" | jq -r '.toolUseId // "NULL"')"
  echo "    toolInput:   $(echo "$POST_TOOL" | jq -c '.toolInput // "NULL"' | head -c 200)"
  echo "    toolResponse:$(echo "$POST_TOOL" | jq -c '.toolResponse // "NULL"' | head -c 200)"
  echo "    model:       $(echo "$POST_TOOL" | jq -r '.model // "NULL"')"
  echo "    cwd:         $(echo "$POST_TOOL" | jq -r '.cwd // "NULL"')"

  assert_neq "post_tool_use has toolName" "NULL" "$(echo "$POST_TOOL" | jq -r '.toolName // "NULL"')"
  assert_neq "post_tool_use has toolResponse" "NULL" "$(echo "$POST_TOOL" | jq -r '.toolResponse // "NULL"')"
fi

# ── Session span field audit ──────────────────────────────────────
echo ""
echo "── Step 5: Session span field audit"

SESSION_START=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "session_start")) | .[0]')
if [ "$SESSION_START" != "null" ]; then
  echo "  session_start:"
  echo "    model:     $(echo "$SESSION_START" | jq -r '.model // "NULL"')"
  echo "    cwd:       $(echo "$SESSION_START" | jq -r '.cwd // "NULL"')"
  echo "    metadata:  $(echo "$SESSION_START" | jq -c '.metadata // "NULL"')"
fi

SESSION_END=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "session_end")) | .[0]')
if [ "$SESSION_END" != "null" ]; then
  echo "  session_end:"
  echo "    metadata:  $(echo "$SESSION_END" | jq -c '.metadata // "NULL"')"
fi

USER_PROMPT=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "user_prompt_submit")) | .[0]')
if [ "$USER_PROMPT" != "null" ]; then
  echo "  user_prompt_submit:"
  echo "    metadata.prompt: $(echo "$USER_PROMPT" | jq -r '.metadata.prompt // "NULL"' | head -c 100)"
fi

# ── Subagent span field audit ─────────────────────────────────────
echo ""
echo "── Step 6: Subagent span field audit"

SUBAGENT_START=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "subagent_start")) | .[0]')
if [ "$SUBAGENT_START" != "null" ]; then
  echo "  subagent_start:"
  echo "    agentName:  $(echo "$SUBAGENT_START" | jq -r '.agentName // "NULL"')"
  echo "    metadata:   $(echo "$SUBAGENT_START" | jq -c '.metadata // "NULL"')"
  assert_neq "subagent_start has agentName" "NULL" "$(echo "$SUBAGENT_START" | jq -r '.agentName // "NULL"')"
else
  echo "  subagent_start: not captured"
fi

SUBAGENT_STOP=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "subagent_stop")) | .[0]')
if [ "$SUBAGENT_STOP" != "null" ]; then
  echo "  subagent_stop:"
  echo "    agentName:  $(echo "$SUBAGENT_STOP" | jq -r '.agentName // "NULL"')"
  echo "    metadata:   $(echo "$SUBAGENT_STOP" | jq -c '.metadata // "NULL"')"
else
  echo "  subagent_stop: not captured"
fi

# ── Full field matrix dump ────────────────────────────────────────
echo ""
echo "── Step 7: Full field matrix (captured vs null per event type)"
echo ""

SPAN_FIELDS="spanId sessionId source kind eventType status timestamp cwd model toolName toolUseId toolInput toolResponse error isInterrupt agentName metadata"

printf "  %-24s" "field"
UNIQUE_EVENTS=$(echo "$SESSION_SPANS" | jq -r '[.[].eventType] | unique | sort | .[]')
for et in $UNIQUE_EVENTS; do
  printf "%-16s" "$et"
done
echo ""

printf "  %-24s" "────"
for et in $UNIQUE_EVENTS; do
  printf "%-16s" "────"
done
echo ""

for field in $SPAN_FIELDS; do
  printf "  %-24s" "$field"
  for et in $UNIQUE_EVENTS; do
    VAL=$(echo "$SESSION_SPANS" | jq -r --arg et "$et" --arg f "$field" \
      'map(select(.eventType == $et)) | .[0] | .[$f] // null | if . == null then "✗" elif type == "object" or type == "array" then "✓ (obj)" elif . == "" then "✗" else "✓" end')
    printf "%-16s" "$VAL"
  done
  echo ""
done

# ── Raw debug log ─────────────────────────────────────────────────
echo ""
echo "── Step 8: Raw payloads from Claude Code"
if [ -f "$PULSE_DEBUG_LOG" ]; then
  cat "$PULSE_DEBUG_LOG"
else
  echo "  No debug log found"
fi

# ── Summary ───────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "══════════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
