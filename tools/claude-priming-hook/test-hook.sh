#!/usr/bin/env sh
# Plain shell test suite for lookout-prime.sh. No bats required.
set -eu

HOOK="$(dirname "$0")/lookout-prime.sh"
FAIL=0

assert_contains() {
    needle="$1"
    haystack="$2"
    label="$3"
    case "$haystack" in
        *"$needle"*) echo "PASS: $label" ;;
        *) echo "FAIL: $label (expected to contain '$needle', got: $haystack)"; FAIL=1 ;;
    esac
}

# --- SessionStart ---
out="$(printf '{"hook_event_name":"SessionStart"}' | "$HOOK")"
assert_contains "127.0.0.1:9477/mcp" "$out" "SessionStart mentions lookout URL"
assert_contains "lookout-companion" "$out" "SessionStart references the skill"
assert_contains "set_session_label" "$out" "SessionStart mentions session label convention"
assert_contains "subagent" "$out" "SessionStart mentions subagent dispatch guidance"


# --- UserPromptSubmit ---
out="$(printf '{"hook_event_name":"UserPromptSubmit"}' | "$HOOK")"
assert_contains "lookout" "$out" "UserPromptSubmit mentions lookout"
assert_contains "show_" "$out" "UserPromptSubmit mentions show_* tools"
assert_contains "lookout-companion" "$out" "UserPromptSubmit references the skill"

exit "$FAIL"
