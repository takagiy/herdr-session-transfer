# Sessions

Copying a session between harness stores with the txcript library.

- [x] When a session is copied, the copy can get a new id and the current time, leaving the source as it was.
- [x] When a session is copied into Codex, the copy's id can be a UUID v7 like Codex's own.
- [x] When the source session's working directory no longer exists, the copy can use the pane's working directory instead.
- [x] When the source session's working directory exists, the copy can keep it.
- [x] When the pane's working directory ends with a path separator, the copy can use it without the separator.
- [x] When the source session id is not in its harness's store, the copier can report that it was not found.
