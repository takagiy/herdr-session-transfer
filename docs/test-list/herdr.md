# herdr

Reading herdr's CLI output and acting on panes through the herdr CLI.

- [x] When `pane get` reports an agent with an `agent_session`, the reader can give the agent name, session id, and cwd.
- [x] When `pane get` reports an agent without an `agent_session`, the reader can give the agent name with no session id.
- [x] When `pane get` reports no agent, the reader can give a pane with no agent.
- [x] When `pane process-info` reports a foreground process other than the shell, the reader can say the agent was started from the shell.
- [x] When `pane process-info` reports the shell itself as the foreground process, the reader can say nothing was started from the shell.
- [x] When the picker opens over a pane, the reader can find that pane in the plugin context.
- [x] When the shell is among the foreground processes, the reader can tell that no other agent is in front of it.
- [x] When transfer stops the agent, the stopper can press Ctrl+C through herdr, one key per request, until the shell is in front again.
- [x] When Claude Code leaves through agent view, the stopper can keep pressing Ctrl+C until the shell returns.
- [x] When the first Ctrl+C ends the agent, the stopper can leave the shell without a second Ctrl+C.
- [x] When the agent exits just as an exit key reaches it and the key request fails, the stopper can still count the returned shell as a normal exit.
- [x] When an exit key request fails and the shell has not returned, the stopper can report that error.
- [x] When the agent does not exit normally, the stopper can give the reason and leave the next harness unstarted.
- [x] When the foreground process list is empty, the reader can say the shell has not returned yet.
