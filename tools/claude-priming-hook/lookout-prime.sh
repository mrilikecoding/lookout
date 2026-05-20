#!/usr/bin/env sh
# lookout-prime.sh: emits priming context for Claude sessions, reminding
# the agent that lookout is available and worth pushing to.
#
# Hook contract:
#   - stdin: JSON with hook_event_name and event-specific fields.
#   - stdout: JSON {"hookSpecificOutput":{"hookEventName":...,"additionalContext":...}}
#     for events that support context injection (SessionStart, UserPromptSubmit,
#     PostToolUse). PreToolUse and SubagentStop don't support context, so we
#     fold subagent guidance into SessionStart and use PostToolUse(Agent) for
#     the post-return nudge.
set -eu

payload="$(cat || true)"
event="$(printf '%s' "$payload" | jq -r '.hook_event_name // empty' 2>/dev/null || true)"

emit() {
    jq -n --arg hook "$event" --arg ctx "$1" \
        '{hookSpecificOutput: {hookEventName: $hook, additionalContext: $ctx}}' \
        2>/dev/null || true
}

case "$event" in
    SessionStart)
        emit "Lookout is running at http://127.0.0.1:9477/mcp, an MCP visualizer TUI the user is watching in another pane. PUSH cards on these triggers (don't deliberate, do it): before any git operation, show_status naming the action + branch state; after running tests or builds, show_log or show_status with results; when you'd produce a table, tree, diff, or long enumeration, push the structured form to lookout, not just in chat; before dispatching a subagent, show_status naming the delegation; when starting a multi-step task, open with show_status, update in place as you advance. Call set_session_label once early with a short label naming the work. Reuse card_id to update cards rather than spamming new ones. See the lookout-companion skill for the card-type catalog. Chat for prose; lookout for state."
        ;;
    UserPromptSubmit)
        emit "Lookout active. Push on: git ops, test/build results, structured output (tables/trees/diffs/long lists), subagent dispatch, multi-step starts. Update via card_id + pin rather than new cards. See lookout-companion if unsure."
        ;;
    PostToolUse)
        # settings.json's matcher already filters to Agent in production; this
        # check makes the script self-defensive if anyone reuses or rewires it.
        tool="$(printf '%s' "$payload" | jq -r '.tool_name // empty' 2>/dev/null || true)"
        if [ "$tool" = "Agent" ]; then
            emit "Subagent returned. Push their findings as a show_* card before folding into your reply (show_table for enumerations, show_diff for changes, show_status for state). Skip only if the result is one line of prose."
        fi
        ;;
    *) : ;;
esac

exit 0
