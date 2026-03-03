# cagent

`cagent` は Claude / Codex セッションをローカルで管理し、CLI・Telegram から操作するためのツールです。

## 必要環境

- Rust (Cargo)
- `claude` または `codex` コマンド
- (Telegram を使う場合) Bot Token

## ビルド

```bash
cargo build
```

## 使い方

### 1. サーバーを起動する

まずフォアグラウンドでサーバーを起動します。

```bash
cagent server
```

- `Ctrl-C` で停止します（特別なシグナル処理なし）
- PID ファイル: `${XDG_STATE_HOME:-~/.local/state}/cagent/server-pid`
- ソケット: `${XDG_STATE_HOME:-~/.local/state}/cagent/server.sock`

### 2. セッションを作る

別ターミナルから実行します。

```bash
cagent agent claude
# or
cagent agent codex
```

標準出力に `session_id` が出ます。

### 3. セッション操作

```bash
cagent agent list
cagent agent subscribe <session_id>
cagent agent attach <session_id>
cagent agent send <session_id> "hello"
cagent agent kill <session_id>
cagent agent prune
```

## Telegram 連携

設定ファイル: `~/.config/cagent/config.toml`

```toml
agent = "claude" # or "codex"
claude_command = "claude"
codex_command = "codex"
# claude_config_dir = "/path/to/.claude"

[telegram]
token = "<BOT_TOKEN>"
# working_dir = "/path/to/workdir"
```

起動:

```bash
cagent telegram start
```

## Cron サブコマンド

ジョブ定義を保存・一覧・削除します。

```bash
cagent cron add --cron "*/5 * * * *" --prompt "status"
cagent cron list
cagent cron rm <job_id>
```

保存先: `~/.config/cagent/cron.json`

## 開発用コマンド

```bash
make build
make run
make test
make check
make fmt
```
