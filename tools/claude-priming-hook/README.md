# Claude Priming Hook

A Claude Code hook + skill pair that keeps lookout top-of-mind for Claude
sessions. See `docs/superpowers/specs/2026-05-20-claude-priming-hook-design.md`
for the design.

## Contents

- `lookout-prime.sh`: hook script (copy to `~/.claude/hooks/`)
- `SKILL.md`: skill content (copy to `~/.claude/skills/lookout-companion/SKILL.md`)
- `settings.fragment.json`: merge into `~/.claude/settings.json` under `hooks`
- `test-hook.sh`: local test suite for the hook script

Installation instructions: see "Install" section below (added in Task 9).
