# facuherdr

A personal fork of [Herdr](https://github.com/herdrdev/herdr) (Apache 2.0) built for running
many coding agents at once in a single monorepo. Upstream Herdr groups agents by git
repository, which collapses into one group when everything lives in one repo. This fork adds
**feature groups**: named groups you create and fill yourself, shown in the agents panel.

It installs as `facuherdr` and keeps its own config, sessions and sockets, so it runs side by
side with an official `herdr` install. The original Herdr README follows
[below](#herdr).

## What the fork adds

**Feature groups in the agents panel**

- Click the label on the right of the agents panel header to cycle `grouped` → `priority` →
  `features`. The features view lists each feature as a header (theme `surface0` background,
  emoji names welcome) with its agents indented underneath, then an `ungrouped` block.
- Each agent row shows its **space name** (renaming the space renames the row) and, on a second
  line, its account label: `facundo (56%) | opus 5.5` (see [account labels](#account-labels)).
  When two agents share a space, the row adds the agent's name.
- **Drag and drop**: reorder agents inside a feature, drag them into another feature or into
  `ungrouped`, or drag a feature header to move the whole feature.
- **Collapse** a feature with the ▾/▸ toggle; a collapsed header shows the agent count and the
  most urgent status of its agents.
- **Right-click menus**
  - feature header: New agent, Rename, Collapse/Expand, Delete (delete only ungroups agents);
  - agent row: the menu of its space (Rename renames the row);
  - `ungrouped` header or empty panel space: New feature….
- Feature groups are saved with the session and survive detaching, server restarts and live
  handoff.

**Starting agents**

- `prefix+a` (`keys.new_agent`) opens a picker: choose a feature, type a new name to create
  one, or pick **"no feature: say where it goes in the task"**. Then type the agent's task.
- Every new agent gets **its own space**, started with `terminal.new_agent_command`.
- Without a feature, the task is sent with the feature commands as context (not as an
  instruction), so your message decides whether the agent joins an existing feature or creates
  one.
- Panes created by `herdr` commands run from inside a grouped pane (`workspace create`,
  `tab create`, `pane split`, `worktree create/open`) join the caller's feature, so an
  orchestrator agent's children stay grouped with it.

**Feature commands** (for you, scripts and agents)

```sh
facuherdr feature list                                  # feature groups and their panes
facuherdr feature create "🎬 clips"                      # an empty feature
facuherdr feature join "🎬 clips" [--create] [--pane <id>]  # this pane by default
```

Inside a pane, prefer `"${HERDR_BIN_PATH:-herdr}"` over `herdr`: Herdr sets `HERDR_BIN_PATH` to
the binary that runs the pane, so commands reach `facuherdr` even when `herdr` on `PATH` is the
official one.

## Build and install

Requires the usual Herdr toolchain (Rust and Zig 0.16.0; see [CONTRIBUTING.md](CONTRIBUTING.md)).

```sh
# Release build that keeps its data in ~/.config/facuherdr instead of ~/.config/herdr.
HERDR_APP_DIR_NAME=facuherdr cargo build --release --locked

# Keep every build as its own file and point facuherdr at the current one, so a running
# server never has its binary overwritten and rollback is one symlink.
release=~/.local/share/facuherdr/releases/facuherdr-$(git rev-parse --short HEAD)-$(date +%Y%m%d-%H%M%S)
mkdir -p "$(dirname "$release")" && cp target/release/herdr "$release"
ln -sfn "$release" ~/.local/bin/facuherdr

facuherdr --session main   # or plain `facuherdr` for the default session
```

To move a running session onto a new build without stopping its agents:

```sh
facuherdr --session main server live-handoff --import-exe "$release"
```

Then detach (`prefix+q`) and reattach to load the new sidebar. Stopping instead
(`facuherdr --session main server stop`) saves the session; agents resume their conversations
when it reopens.

- `HERDR_APP_DIR_NAME` is read at compile time and only affects release builds. Debug builds
  always use `herdr-dev`.
- Never run `facuherdr update`: it installs official Herdr over the fork. The config below
  turns the update prompt off.
- Back up before big changes: `tar -czf facuherdr-backup.tgz -C ~/.config facuherdr`.

### Sandbox

`contrib/facuherdr/bin/facuherdr-dev` rebuilds the working copy (debug) and runs it with its own
data in `~/.config/herdr-dev`, apart from your real sessions. It clears the variables a
`facuherdr` pane sets, so it works from inside one.

```sh
facuherdr-dev               # try the latest code
facuherdr-dev server stop   # stop the sandbox
```

Set `FACUHERDR_REPO` if the checkout is not at `~/workspace/herdr`.

## Configuration

`~/.config/facuherdr/config.toml` (your usual Herdr config plus):

```toml
[ui]
agent_panel_sort = "features"   # open the agents panel in the features view

[keys]
new_agent = "prefix+a"          # the default

[terminal]
# Typed into each new agent's shell; the task, when given, is appended as one quoted argument.
# With clauth, pin each agent to the account active when it starts:
new_agent_command = 'clauth start "$(clauth which)"'

[update]
version_check = false           # never suggest installing official Herdr over the fork
```

## Account labels

The second line of an agent row is reported by the agent itself as pane metadata, keyed by agent
kind (`claude_account`, `codex_account`) so a label left by an earlier agent in the same pane is
never shown on the current one. Labels are not persisted; they come back as agents report again.
All scripts live in [`contrib/facuherdr`](contrib/facuherdr); copy `bin/*` to `~/.local/bin`.

### Claude Code: `herdr-claude-statusline`

A wrapper in front of your status line. On each redraw it reports
`<account, 7 chars> (<weekly % used>) | <model>` and passes the input on unchanged to the real
status line command given as its arguments.

- Account and usage come from [clauth](#clauth) when it knows the session's profile
  (`clauth which`, `clauth status --json`); otherwise from the account email in
  `$CLAUDE_CONFIG_DIR/.claude.json` and the `rate_limits` in the status line input, which Claude
  Code sends only after the first reply.
- The model comes from the model id (`claude-opus-5-5` → `opus 5.5`).
- Reports in the background, when the label changes or at least once a minute.

```json
"statusLine": {
  "type": "command",
  "command": "~/.local/bin/herdr-claude-statusline <your existing status line command>"
}
```

Without an existing status line, use the script alone; it then draws nothing.

### Codex: `herdr-codex-account` and `codex-no-daemon.zsh`

Codex has no custom status line, so a hook reports the label (`facundo (12%) | gpt-6-astra`)
when a session starts and after every turn. It reads the login email from `~/.codex/auth.json`,
the weekly window (`window_minutes == 10080`) from the newest session log with rate limits, and
the model from the session log (falling back to `model` in `~/.codex/config.toml`).

Register it in `~/.codex/hooks.json`, next to any existing hooks:

```json
{
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "bash ~/.local/bin/herdr-codex-account", "timeout": 10 }] }],
    "Stop":         [{ "hooks": [{ "type": "command", "command": "bash ~/.local/bin/herdr-codex-account", "timeout": 10 }] }]
  }
}
```

- Codex asks once to trust a new hook.
- The hook reports before exiting, because Codex stops a hook's leftover processes.
- Codex runs its start hook when the first message is sent, so a label appears after the first
  reply.
- **Interactive Codex runs hooks in a shared background daemon** that has no idea which Herdr
  pane it serves. [`codex-no-daemon.zsh`](contrib/facuherdr/codex-no-daemon.zsh) wraps `codex`
  so that, only inside Herdr panes, interactive sessions (`codex`, `codex resume`, `codex fork`)
  run with `--no-daemon`. Source it from `~/.zshrc`. Those sessions are then not visible to the
  shared daemon's features (`codex agents`, remote control).

### clauth

[clauth](https://crates.io/crates/clauth) (`cargo install clauth`) manages several Claude
accounts. Plain `claude` sessions share one login, which clauth swaps when you change the active
account, so every plain session moves with it. `clauth start <profile>` gives a session its own
login instead. Use it for new agents (`terminal.new_agent_command` above) and, through
`herdr-start-claude`, for agents started by other agents:

```sh
herdr-start-claude <pane-id> <agent-name> [--account <profile>] [--feature <name>] [--timeout <s>] [-- <claude args>...]
```

Starts `clauth start <account>` in a pane (default account: the caller's own), waits until Herdr
detects Claude, names the agent so `herdr agent prompt <name>` works, optionally files it under a
feature (created if missing), and prints JSON. Codex agents keep using
`herdr agent start <name> --kind codex --pane <id>`.

## Keeping up with upstream

```sh
git fetch origin                       # origin = herdrdev/herdr
git rebase origin/master               # on this fork's branch
cargo test --bin herdr                 # or `just test` with cargo-nextest
facuherdr-dev                          # try it in the sandbox
# then build, install and live-hand off as above
```

## Known limitations

- Codex agents show a grey status dot: Herdr's screen detection does not yet match Codex 0.157.
- Claude agents that Herdr resumes after a restart run plain `claude`, so they use the shared
  login rather than a pinned clauth account; the label shows which account they landed on.
- Closing every space in a session also discards its saved feature groups.
- If official Herdr ever runs a fork session, it drops the feature groups on its next save.

---

# herdr


<p align="center">
  <img src="assets/logo.png" alt="herdr" width="100" />
</p>

<p align="center">
  <a href="https://herdr.dev">herdr.dev</a> · <a href="#install">install</a> · <a href="https://herdr.dev/docs/quick-start/">quick start</a> · <a href="https://herdr.dev/docs/">docs</a>
</p>

<p align="center">
  English · <a href="README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
  <a href="https://github.com/herdrdev/herdr/releases"><img src="https://img.shields.io/github/downloads/herdrdev/herdr/total?labelColor=333333&color=666666" alt="total GitHub release downloads" /></a>
  <a href="https://github.com/herdrdev/herdr/stargazers"><img src="https://img.shields.io/github/stars/herdrdev/herdr?labelColor=333333&color=666666&logo=github" alt="GitHub stars" /></a>
  <a href="https://github.com/herdrdev/herdr/releases/latest"><img src="https://img.shields.io/github/v/release/herdrdev/herdr?label=release&labelColor=333333&color=666666" alt="latest stable release" /></a>
  <a href="https://formulae.brew.sh/formula/herdr"><img src="https://img.shields.io/homebrew/v/herdr?label=homebrew&labelColor=333333&color=666666" alt="Homebrew version" /></a>
  <a href="https://x.com/herdrdev"><img src="https://img.shields.io/badge/follow-%40herdrdev-000000?logo=x&logoColor=white" alt="follow @herdrdev on X" /></a>
</p>

---

https://github.com/user-attachments/assets/043ec09f-4bdd-41d5-aee0-8fda6b83e267

**the runtime your coding agents live on.**

- **detach without stopping work** — herdr keeps terminals running in a background server when you close the client or lose your SSH connection. after a server or machine restart, herdr restores the saved layout and can resume supported agent sessions; the original processes do not survive. [session state →](https://herdr.dev/docs/session-state/)
- **several machines, one window** — keep local work and saved ssh machines together, with a combined agent list and independent reconnects. [remote machines →](https://herdr.dev/docs/connecting-machines/)
- **never hunt for the stuck one** — every pane is marked working, blocked, or idle. when an agent stops and needs an answer, herdr says so.
- **agent-native** — agents drive herdr through the cli and socket api: they can spawn panes, prompt each other, and wait until another agent is genuinely blocked. [agent skill →](https://herdr.dev/docs/agent-skill/)
- **runs what you already run** — claude code, codex, cursor, opencode, grok and the rest. herdr doesn't wrap or replace them; it owns their terminals.
- **keyboard and mouse, both first-class** — tmux-style prefix keys *and* click, drag, split. pick per moment, not per tool.
- **plugins** — extend panes and workflows. [browse the marketplace →](https://herdr.dev/plugins/)
- **one rust binary, no electron** — runs in whatever terminal you already use.

---

## install

```bash
curl -fsSL https://herdr.dev/install.sh | sh
```

or `brew install herdr` · `mise use -g herdr` · windows: `powershell -ExecutionPolicy Bypass -c "irm https://herdr.dev/install.ps1 | iex"` · [endpoint-protected Windows](https://herdr.dev/docs/windows-beta/) · [binaries](https://github.com/herdrdev/herdr/releases)

then start it where the work lives:

```bash
herdr
```

run your agents, split panes, walk away. `ctrl+b q` detaches, `herdr` reattaches. [quick start →](https://herdr.dev/docs/quick-start/)

## docs

everything lives at [herdr.dev/docs](https://herdr.dev/docs/): [quick start](https://herdr.dev/docs/quick-start/) · [concepts](https://herdr.dev/docs/concepts/) · [supported agents](https://herdr.dev/docs/agents/) · [keyboard](https://herdr.dev/docs/keyboard/) · [configuration](https://herdr.dev/docs/configuration/) · [session state](https://herdr.dev/docs/session-state/) · [connecting machines](https://herdr.dev/docs/connecting-machines/) · [remote](https://herdr.dev/docs/persistence-remote/) · [integrations](https://herdr.dev/docs/integrations/) · [plugins](https://herdr.dev/docs/plugins/) · [socket api](https://herdr.dev/docs/socket-api/)

## thanks

every past sponsor and backer is listed in [SPONSORS.md](./SPONSORS.md) — thank you 🐑

enterprise / partnership: hey@herdr.dev

## agent instructions

if you are an ai agent helping with this repository, read [`AGENTS.md`](./AGENTS.md) before making changes and read [`CONTRIBUTING.md`](./CONTRIBUTING.md) before opening issues or PRs.

## development

```bash
git clone https://github.com/herdrdev/herdr
cd herdr
cargo build --release

just test        # unit tests
just check       # formatting, tests, and maintenance checks
```

## license

Herdr is licensed under the [Apache License 2.0](LICENSE).
