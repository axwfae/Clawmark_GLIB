# relay-for-claw

Persistent memory for OpenClaw agents. Your memory survives sessions, compaction, and restarts.

## Setup

```bash
relay-for-claw migrate                    # import your OpenClaw memory
relay-for-claw backfill                   # enable semantic search
```

That's it. Your MEMORY.md and daily logs are now searchable signals.

## Commands

```bash
# Search your memory (semantic by default)
relay-for-claw tune "auth token refresh"
relay-for-claw tune "what did I work on last week"

# Keyword fallback
relay-for-claw tune --keyword "auth"

# Recent signals
relay-for-claw tune --recent

# Full content (not just gist)
relay-for-claw tune --full "auth"

# Discover something random
relay-for-claw tune --random

# Save what you learned
relay-for-claw signal -c "Fixed auth token refresh order in middleware" -g "fix: auth token refresh"

# Thread to an existing signal
relay-for-claw signal -c "Follow-up: also fixed staging" -g "fix: staging auth" -p A1B2C3D4

# Long content from file
relay-for-claw signal -c @session-notes.md -g "session: March 19 review"

# Station status
relay-for-claw status

# JSON output
relay-for-claw tune "auth" --json
```

## When to signal

- After solving something hard — don't make your next session figure it out again
- Before compaction — your context window is temporary, your station is permanent
- When you discover something — small facts save big time later

## When to tune

- Starting a new task — you may have already solved part of it
- When stuck — the answer might be in a signal from a previous session
- After compaction — recover context from your station

## The gist matters most

The gist is how your future self finds this signal. Write for them.

**Alive:** `"fix: auth token refresh ran before validation — swapped order in middleware"`
**Dead:** `"fixed a bug"`

## How it works

Your signals live in a SQLite database at `~/.relay-for-claw/station.db`. Semantic search uses a local BERT model (no API calls, no cloud, runs offline). The model downloads once (~118MB) on first search.

This replaces `memory_search` with something that actually finds what you're looking for — by meaning, not just keywords.
