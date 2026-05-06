# lookout

Streaming visualizer with an MCP interface — a long-running terminal pane any Claude
session can push cards into (tables, charts, trees, diffs, logs, images, progress,
status, questions).

See `docs/superpowers/specs/2026-05-05-lookout-design.md` for the full design.

## Run

    cargo run                   # listens on http://127.0.0.1:9477/mcp
    cargo run -- --port 9999    # different port
    cargo run --example smoke   # push one of every card type to a running server

## Connect from a Claude session

Add to your client config (example fragment):

    {
      "mcpServers": {
        "lookout": { "type": "http", "url": "http://127.0.0.1:9477/mcp" }
      }
    }

## Keybindings

    j / k or arrows   move focus
    o / Enter         expand / collapse focused card
    Esc               collapse / cancel filter prompt
    p / P             pin / unpin focused card
    /                 filter prompt (title substring)
    c                 clear feed (pins remain)
    q / Ctrl-C        quit (graceful drain)
