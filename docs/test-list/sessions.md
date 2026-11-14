# Sessions

Copying a session between harness stores with the txcript library.

- [x] When a session is copied, the copy can get a new id and the current time, leaving the source as it was.
- [x] When a session is copied into Codex, the copy's id can be a UUID v7 like Codex's own.
- [x] When the source session's working directory no longer exists, the copy can use the pane's working directory instead.
- [x] When the source session's working directory exists, the copy can keep it.
- [x] When the pane's working directory ends with a path separator, the copy can use it without the separator.
- [x] When the source session id is not in its harness's store, the copier can report that it was not found.
- [x] When a conversation is copied between Claude Code and Codex in either direction, the copy can be read back with its text and its tool calls paired with their results.
- [ ] When the copier looks for the source session, it can scan only that harness's store.
- [x] When a Codex session was forked, the copier can read the history before the fork from the session it was forked from.
- [x] When the session it was forked from went on after the fork, the copier can leave out what came after the fork point.
- [x] When the session it was forked from was itself forked, the copier can follow the forks back to the first session.
- [x] When the session it was forked from is missing, the copier can report that it was not found.
- [x] When the forks lead back to a session already read, the copier can stop following them there and keep the history read so far.
