# herdr

Reading herdr's CLI output and acting on panes through the herdr CLI.

- [x] When `pane get` reports an agent with an `agent_session`, the reader can give the agent name, session id, and cwd.
- [x] When `pane get` reports an agent without an `agent_session`, the reader can give the agent name with no session id.
- [x] When `pane get` reports no agent, the reader can give a pane with no agent.
- [x] When `pane process-info` reports a foreground process other than the shell, the reader can say the agent was started from the shell.
- [x] When `pane process-info` reports the shell itself as the foreground process, the reader can say nothing was started from the shell.
- [x] When the picker opens over a pane, the reader can find that pane in the plugin context.
- [x] When the shell is among the foreground processes, the reader can leave it out of the processes to stop.
