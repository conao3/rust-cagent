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

Commands follow the AWS CLI grammar: `cagent [global options] <command> <subcommand> [--parameter value ...]`.

- Every parameter is named (`--session-id`, `--prompt`, ...). A value starting with `file://` is read from that file, and `file:///dev/stdin` reads standard input.
- `--cli-input-json file://input.json` passes the parameters as a JSON object keyed by the PascalCase parameter names; parameters on the command line take precedence. `--generate-cli-skeleton input|output` prints the input or output skeleton.
- Global options: `--output json|text|table` (default `json`), `--query <JMESPath>`, `--debug`, `--version`.
- `help` works at every level: `cagent help`, `cagent session help`, `cagent session send-prompt help`.
- Output keys are PascalCase.
- Exit codes: 0 success, 252 invalid command or parameter, 254 the target is missing (unknown session or cron job, server not running), 255 other failures.

### 1. Start the server

Run the server in the foreground:

```bash
cagent server start-server
```

- Stop with `Ctrl-C` (default process behavior; no custom signal handling)
- PID file: `${XDG_STATE_HOME:-~/.local/state}/cagent/server-pid`
- REST endpoint: `http://127.0.0.1:45931`
- Telegram bot responder is also started by this command.
- Logs go to standard error.

### 2. Create a session

In another terminal:

```bash
cagent session create-session --agent claude
# or
cagent session create-session --agent codex
```

The command prints `{"SessionId": "...", "Agent": "..."}`. `--session-id`, `--agent-command`, `--claude-config-dir` (claude only), and `--initial-prompt` are optional.

The server runs each session as `cagent session run-session --agent <agent> --session-id <id> ...` in the foreground.

### 3. Session operations

```bash
cagent session list-sessions
cagent session attach-session --session-id <session_id>
cagent session send-prompt --session-id <session_id> --prompt "hello"
echo "hello" | cagent session send-prompt --session-id <session_id> --prompt file:///dev/stdin
cagent session kill-session --session-id <session_id>
cagent session prune-sessions
```

`attach-session` streams the session's messages as JSON lines (one per message, as the agent emits them) and forwards standard input to the session until interrupted.

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

Telegram responses are served by `cagent server start-server`.

## Cron jobs

Store, list, and remove cron job definitions:

```bash
cagent cron-job create-cron-job --cron-expression "*/5 * * * *" --prompt "status"
cagent cron-job list-cron-jobs
cagent cron-job delete-cron-job --job-id <job_id>
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
