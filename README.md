# cagent

`cagent` is a local session manager for Claude/Codex with CLI and Telegram interfaces.

## Requirements

- Rust (Cargo)
- `claude` and/or `codex` command available in `PATH`
- Telegram bot token (if you use Telegram integration)

## Build

```bash
cargo build
```

## Usage

### 1. Start the server

Run the server in the foreground:

```bash
cagent server
```

- Stop with `Ctrl-C` (default process behavior; no custom signal handling)
- PID file: `${XDG_STATE_HOME:-~/.local/state}/cagent/server-pid`
- REST endpoint: `http://127.0.0.1:45931`

### 2. Create a session

In another terminal:

```bash
cagent agent claude
# or
cagent agent codex
```

The command prints a `session_id`.

### 3. Session operations

```bash
cagent agent list
cagent agent subscribe <session_id>
cagent agent attach <session_id>
cagent agent send <session_id> "hello"
cagent agent kill <session_id>
cagent agent prune
```

## Telegram integration

Configuration file: `~/.config/cagent/config.toml`

```toml
agent = "claude" # or "codex"
claude_command = "claude"
codex_command = "codex"
# claude_config_dir = "/path/to/.claude"

[telegram]
token = "<BOT_TOKEN>"
# working_dir = "/path/to/workdir"
```

Start:

```bash
cagent telegram start
```

## Cron subcommands

Store, list, and remove cron job definitions:

```bash
cagent cron add --cron "*/5 * * * *" --prompt "status"
cagent cron list
cagent cron rm <job_id>
```

Storage file: `~/.config/cagent/cron.json`

## Development commands

```bash
make build
make run
make test
make check
make fmt
```
