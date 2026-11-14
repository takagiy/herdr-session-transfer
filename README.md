# herdr-session-transfer

A [herdr](https://herdr.dev) plugin that transfers the session in a pane to another coding agent, in the same pane.

Supported agents: Claude Code and Codex.

## Usage

In an agent's pane, press the [transfer key](#install) and choose the agent to transfer to. The plugin then:

1. Copies the conversation into the chosen agent as a new session.
2. Stops the agent in the pane.
3. Starts the chosen agent in the same pane, resuming the copy.

Thanks to [txcript](https://github.com/skillsynchq/txcript), which does the conversion between agents.

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

- herdr must know which session the pane holds.
- The agent must have been started from the pane's shell.

## License

MIT
