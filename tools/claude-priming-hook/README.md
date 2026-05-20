# Claude Priming Hook

A Claude Code hook + skill pair that keeps lookout top-of-mind for every
Claude session, so the agent pushes glanceable cards to lookout instead
of letting useful structured output get lost in the chat.

See the [design spec](../../docs/superpowers/specs/2026-05-20-claude-priming-hook-design.md)
for the why and what.

## Prerequisites

- Claude Code installed.
- `jq` installed (`brew install jq` on macOS).
- Lookout MCP server configured in `~/.claude.json`:

      "mcpServers": {
        "lookout": { "type": "http", "url": "http://127.0.0.1:9477/mcp" }
      }

- Lookout running locally (`cargo run` in this repo).

## Install

1. Copy the hook script:

       mkdir -p ~/.claude/hooks
       cp tools/claude-priming-hook/lookout-prime.sh ~/.claude/hooks/
       chmod +x ~/.claude/hooks/lookout-prime.sh

2. Copy the skill:

       mkdir -p ~/.claude/skills/lookout-companion
       cp tools/claude-priming-hook/SKILL.md ~/.claude/skills/lookout-companion/

3. Merge the hook entries into `~/.claude/settings.json`. If the file
   has no `hooks` key, paste the fragment's `hooks` block in. If it
   already has a `hooks` key, merge each event array:

       jq -s '.[0] * .[1]' ~/.claude/settings.json \
           tools/claude-priming-hook/settings.fragment.json \
           > ~/.claude/settings.json.new
       mv ~/.claude/settings.json.new ~/.claude/settings.json

   Verify:

       jq '.hooks | keys' ~/.claude/settings.json

   Expected: `["PostToolUse", "SessionStart", "UserPromptSubmit"]`
   (plus any pre-existing hooks).

4. Restart Claude Code (or start a new session) to pick up the hooks.

## Verify

Start a Claude Code session in any directory. Ask Claude to do something
that would produce structured output, e.g., "find all callers of X
across this repo and show them in a table." It should push a
`show_table` to lookout. Watch lookout's TUI for the card.

## Test the hook locally

    ./tools/claude-priming-hook/test-hook.sh

Expected: all `PASS:` lines, exit 0.

## Uninstall

Remove the three hook entries from `~/.claude/settings.json`. Optionally
delete `~/.claude/hooks/lookout-prime.sh` and
`~/.claude/skills/lookout-companion/`.
