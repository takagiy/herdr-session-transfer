# herdr-session-transfer

A [herdr](https://herdr.dev) plugin that transfers the session in a pane to another coding agent, in the same pane. Start a task in Claude Code, and continue the same conversation in Codex without leaving the pane, or the other way around.

Supported agents: Claude Code and Codex.

## Usage

In an agent's pane, press the key bound in [Install](#install) and choose the agent to transfer to. The plugin then:

1. Copies the conversation into the chosen agent as a new session.
2. Stops the agent in the pane.
3. Starts the chosen agent in the same pane, resuming the copy.

The conversion between agents is done by [txcript](https://github.com/skillsynchq/txcript).

## Requirements

- herdr 0.9.4 or later.
- herdr's integration for each agent you use (`herdr integration install <agent>`).
- Each agent's CLI on your `PATH`.

## Install

With a recent stable Rust toolchain (`cargo`) installed:

```sh
herdr plugin install takagiy/herdr-session-transfer
```

Then bind a key to the plugin's `transfer` action in `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "herdr-session-transfer.transfer"
description = "transfer this pane's session to another agent"
```

and reload the config:

```sh
herdr server reload-config
```

## Limitations

- herdr must know which session the pane holds. If it does not, you get a notification and the pane is not changed.
- The agent must have been started from the pane's shell.

## License

MIT
