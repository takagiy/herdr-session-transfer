//! Copying sessions between harness stores with the txcript library.

use std::path::{Path, PathBuf};
use txcript::harness::claude_code::ClaudeStore;
use txcript::harness::codex::{Codex, CodexStore};
use txcript::{Codec, Common, Harness, HarnessId, Store, Transcript, local};

use crate::transfer::Sessions;

/// The session stores on this machine, through txcript.
pub struct TxcriptSessions;

impl Sessions for TxcriptSessions {
    fn copy(
        &self,
        from: HarnessId,
        id: &str,
        to: HarnessId,
        pane_cwd: &Path,
    ) -> Result<String, String> {
        let source = match from {
            HarnessId::ClaudeCode => read_session(ClaudeStore::default_root(), id, |s, _| Ok(s)),
            HarnessId::Codex => read_session(CodexStore::default_root(), id, with_history),
            _ => Err(format!("Cannot transfer sessions from {from}.")),
        }?;
        let written = local::write(to, &prepared_copy(&source, to, pane_cwd), None)
            .map_err(|e| format!("Could not copy session {id} into {}: {e}", to.as_str()))?;
        Ok(written.id)
    }

    fn resume_command(&self, harness: HarnessId, id: &str) -> String {
        let (bin, args) = local::resume_command(harness, id);
        std::iter::once(bin)
            .chain(args)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The session `id` in `store`, completed by `whole` from the other sessions there.
fn read_session<S: Store>(
    store: Option<S>,
    id: &str,
    whole: impl Fn(
        Transcript<S::H>,
        &dyn Fn(&str) -> Result<Transcript<S::H>, String>,
    ) -> Result<Transcript<S::H>, String>,
) -> Result<Transcript<Common>, String>
where
    S::H: Codec,
{
    let harness = <S::H as Harness>::NAME;
    let store = store.ok_or_else(|| format!("Could not locate {harness} sessions."))?;
    let sessions = store
        .discover()
        .map_err(|e| format!("Could not list {harness} sessions: {e}"))?;
    let load = |id: &str| {
        let found = sessions
            .iter()
            .find(|session| session.meta.id == id)
            .ok_or_else(|| format!("Session {id} was not found in {harness}."))?;
        store
            .load(&found.reference)
            .map_err(|e| format!("Could not read session {id}: {e}"))
    };
    let native = whole(load(id)?, &load)?;
    S::H::to_common(&native).map_err(|e| format!("Could not convert session {id}: {e}"))
}

/// `session` with the history before its fork, which Codex keeps only in the
/// session its `history_base` names.
fn with_history(
    session: Transcript<Codex>,
    load: &dyn Fn(&str) -> Result<Transcript<Codex>, String>,
) -> Result<Transcript<Codex>, String> {
    let mut read = vec![session.meta.id.clone()];
    with_history_after(session, load, &mut read)
}

/// [`with_history`], where `read` holds the sessions already on the way back.
fn with_history_after(
    mut session: Transcript<Codex>,
    load: &dyn Fn(&str) -> Result<Transcript<Codex>, String>,
    read: &mut Vec<String>,
) -> Result<Transcript<Codex>, String> {
    let Some(base) = session
        .body
        .first()
        .and_then(|line| line.payload.get("history_base"))
    else {
        return Ok(session);
    };
    let thread_id = base["thread_id"].as_str().unwrap_or_default();
    let end = base["end_ordinal_exclusive"].as_u64().unwrap_or_default();
    if read.iter().any(|id| id == thread_id) {
        return Ok(session);
    }
    read.push(thread_id.to_owned());
    let history = with_history_after(load(thread_id)?, load, read)?
        .body
        .into_iter()
        .filter(|line| {
            line.kind != "session_meta"
                && line
                    .extra
                    .get("ordinal")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|ordinal| ordinal < end)
        });
    session.body.splice(1..1, history);
    Ok(session)
}

/// `source` as a new session for `to`'s store, which the source is left out of.
fn prepared_copy(
    source: &Transcript<Common>,
    to: HarnessId,
    pane_cwd: &Path,
) -> Transcript<Common> {
    let mut copy = source.clone();
    copy.meta.id = match to {
        // Codex gives its own sessions v7 ids.
        HarnessId::Codex => uuid::Uuid::now_v7(),
        _ => uuid::Uuid::new_v4(),
    }
    .to_string();
    copy.meta.timestamp = chrono::Utc::now();
    // Stores file sessions by working directory, so a copy filed under a
    // directory that is gone would be invisible to the harness in the pane.
    if !copy
        .meta
        .cwd
        .as_deref()
        .is_some_and(|cwd| Path::new(cwd).is_dir())
    {
        // Rebuilt from its components to drop the trailing separator herdr
        // reports, which would file the copy under another directory name.
        let cwd: PathBuf = pane_cwd.components().collect();
        copy.meta.cwd = Some(cwd.to_string_lossy().into_owned());
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use txcript::TextCodec;
    use txcript::common::Meta;

    fn source(cwd: Option<&str>) -> Transcript<Common> {
        Transcript::new(
            Meta {
                id: "source-1".into(),
                timestamp: chrono::DateTime::UNIX_EPOCH,
                cwd: cwd.map(String::from),
                git_branch: None,
                title: None,
                cli_version: None,
                model: None,
            },
            Vec::new(),
        )
    }

    #[test]
    fn copy_gets_a_new_id_and_the_current_time() {
        // Given a session that started at the Unix epoch
        let source = source(None);
        let before = chrono::Utc::now();
        // When it is copied into Claude Code
        let copy = prepared_copy(&source, HarnessId::ClaudeCode, Path::new("."));
        // Then the copy has its own id and a current start time, and the
        // source keeps its own
        assert_ne!(copy.meta.id, source.meta.id);
        assert!(copy.meta.timestamp >= before);
        assert_eq!(source.meta.id, "source-1");
        assert_eq!(source.meta.timestamp, chrono::DateTime::UNIX_EPOCH);
    }

    #[test]
    fn copy_into_codex_gets_a_uuid_v7() {
        // Given a session from another harness
        let source = source(None);
        // When it is copied into Codex
        let copy = prepared_copy(&source, HarnessId::Codex, Path::new("."));
        // Then its id is a UUID v7, the kind Codex gives its own sessions
        let id = uuid::Uuid::parse_str(&copy.meta.id).unwrap();
        assert_eq!(id.get_version_num(), 7);
    }

    #[test]
    fn missing_working_directory_is_replaced_by_the_panes() {
        // Given a session whose working directory no longer exists
        let source = source(Some("/no/such/directory/for/herdr-session-transfer"));
        let pane_cwd = std::env::current_dir().unwrap();
        // When it is copied
        let copy = prepared_copy(&source, HarnessId::Codex, &pane_cwd);
        // Then the copy runs in the pane's working directory
        assert_eq!(copy.meta.cwd.as_deref(), pane_cwd.to_str());
    }

    #[test]
    fn existing_working_directory_is_kept() {
        // Given a session whose working directory exists
        let dir = std::env::temp_dir();
        let source = source(dir.to_str());
        // When it is copied from a pane in another directory
        let copy = prepared_copy(&source, HarnessId::Codex, &std::env::current_dir().unwrap());
        // Then the copy keeps the session's working directory
        assert_eq!(copy.meta.cwd.as_deref(), dir.to_str());
    }

    #[test]
    fn trailing_separator_of_the_panes_directory_is_dropped() {
        // Given a session with no working directory, and a pane whose
        // directory herdr reports with a trailing separator
        let source = source(None);
        let dir = std::env::current_dir().unwrap();
        let reported = format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR);
        // When it is copied
        let copy = prepared_copy(&source, HarnessId::Codex, Path::new(&reported));
        // Then the copy's working directory has no trailing separator
        assert_eq!(copy.meta.cwd.as_deref(), dir.to_str());
    }

    #[test]
    fn copied_conversations_preserve_text_and_tool_pairs() {
        use txcript::common::{Block, Message, Role, Tool, ToolOutput};

        // Given isolated stores in a child process, without changing this process's environment
        let root = match std::env::var_os("HERDR_TRANSFER_TEST_ROOT") {
            Some(root) => PathBuf::from(root),
            None => {
                let temp = std::env::temp_dir();
                assert!(temp.is_absolute());
                let root = temp.join(format!("herdr-copy-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir(&root).unwrap();
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "sessions::tests::copied_conversations_preserve_text_and_tool_pairs",
                        "--nocapture",
                    ])
                    .env("HERDR_TRANSFER_TEST_ROOT", &root)
                    .env("CLAUDE_CONFIG_DIR", root.join("claude"))
                    .env("CODEX_HOME", root.join("codex"))
                    .output()
                    .unwrap();
                assert_eq!(root.parent(), Some(temp.as_path()));
                std::fs::remove_dir_all(&root).unwrap();
                assert!(
                    output.status.success(),
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                return;
            }
        };
        let message = |role, content| Message {
            role,
            content,
            timestamp: chrono::DateTime::UNIX_EPOCH,
            model: None,
            stop_reason: None,
            usage: None,
        };
        let mut conversation = source(root.to_str());
        conversation.body = vec![
            message(
                Role::User,
                vec![Block::Text {
                    text: "内容を調べて".into(),
                }],
            ),
            message(
                Role::Assistant,
                vec![Block::ToolUse {
                    id: "read-1".into(),
                    tool: Tool::Read {
                        file_path: "notes.txt".into(),
                        offset: None,
                        limit: None,
                    },
                }],
            ),
            message(
                Role::User,
                vec![Block::ToolResult {
                    tool_use_id: "read-1".into(),
                    content: ToolOutput::Text("保存された本文".into()),
                    is_error: false,
                }],
            ),
            message(
                Role::Assistant,
                vec![Block::Text {
                    text: "確認しました".into(),
                }],
            ),
        ];
        let find = |harness, id: &str| {
            local::discover_harness(harness)
                .unwrap()
                .into_iter()
                .find(|session| session.meta.id == id)
                .unwrap()
        };
        for (from, to) in [
            (HarnessId::ClaudeCode, HarnessId::Codex),
            (HarnessId::Codex, HarnessId::ClaudeCode),
        ] {
            conversation.meta.id = uuid::Uuid::now_v7().to_string();
            let original = local::write(from, &conversation, None).unwrap();
            let source_session = find(from, &original.id);
            let source_bytes = std::fs::read(source_session.location()).unwrap();
            // When the real copier converts and writes the conversation
            let copy_id = TxcriptSessions.copy(from, &original.id, to, &root).unwrap();
            let copy = find(to, &copy_id).read().unwrap();
            // Then text and the complete tool pair survive, while the source file is unchanged
            let actual: Vec<_> = copy.body.iter().map(|m| (&m.role, &m.content)).collect();
            let expected: Vec<_> = conversation
                .body
                .iter()
                .map(|m| (&m.role, &m.content))
                .collect();
            assert_eq!(actual, expected, "{from} -> {to}");
            assert_eq!(copy.tool_pairs().unwrap(), vec![(1, 2)]);
            assert_ne!(copy_id, original.id);
            assert_eq!(copy.meta.id, copy_id);
            assert_eq!(
                std::fs::read(source_session.location()).unwrap(),
                source_bytes
            );
        }
    }

    /// A Codex rollout of user prompts at their ordinals, forked from
    /// `base` = (thread id, first ordinal not taken from it) when given.
    fn rollout(id: &str, base: Option<(&str, u64)>, prompts: &[(u64, &str)]) -> Transcript<Codex> {
        let mut meta = serde_json::json!({"id": id, "timestamp": "2026-11-15T00:00:00Z"});
        if let Some((thread_id, end)) = base {
            meta["history_base"] =
                serde_json::json!({"thread_id": thread_id, "end_ordinal_exclusive": end});
        }
        let lines = std::iter::once(serde_json::json!({"type": "session_meta", "payload": meta}))
            .chain(prompts.iter().map(|(ordinal, text)| {
                serde_json::json!({"ordinal": ordinal, "type": "response_item", "payload": {
                    "type": "message", "role": "user",
                    "content": [{"type": "input_text", "text": text}],
                }})
            }))
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        Codex::from_text(&lines).unwrap()
    }

    fn prompts(session: &Transcript<Codex>) -> Vec<String> {
        Codex::to_common(session)
            .unwrap()
            .body
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|block| match block {
                txcript::common::Block::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn codex_fork_gets_the_history_before_the_fork() {
        // Given a Codex session forked from base-1 after its first prompt
        let base = rollout("base-1", None, &[(1, "before the fork")]);
        let fork = rollout("fork-1", Some(("base-1", 2)), &[(3, "after the fork")]);
        let load = |id: &str| match id {
            "base-1" => Ok(base.clone()),
            _ => Err(format!("Session {id} was not found in codex.")),
        };
        // When the copier reads the fork
        let whole = with_history(fork, &load).unwrap();
        // Then the history before the fork comes first, then the fork's own
        assert_eq!(prompts(&whole), ["before the fork", "after the fork"]);
        assert_eq!(whole.meta.id, "fork-1");
    }

    #[test]
    fn what_the_base_said_after_the_fork_is_left_out() {
        // Given base-1 went on after fork-1 was forked from it at ordinal 2
        let base = rollout(
            "base-1",
            None,
            &[(1, "before the fork"), (2, "base went on")],
        );
        let fork = rollout("fork-1", Some(("base-1", 2)), &[(3, "after the fork")]);
        let load = |_: &str| Ok(base.clone());
        // When the copier reads the fork
        let whole = with_history(fork, &load).unwrap();
        // Then only the history before the fork point is taken from base-1
        assert_eq!(prompts(&whole), ["before the fork", "after the fork"]);
    }

    #[test]
    fn forks_are_followed_back_to_the_first_session() {
        // Given fork-2 forked from fork-1, which was forked from base-1
        let base = rollout("base-1", None, &[(1, "in the first session")]);
        let fork = rollout("fork-1", Some(("base-1", 2)), &[(3, "in the first fork")]);
        let fork_of_fork = rollout("fork-2", Some(("fork-1", 4)), &[(5, "in the second fork")]);
        let load = |id: &str| match id {
            "base-1" => Ok(base.clone()),
            "fork-1" => Ok(fork.clone()),
            _ => Err(format!("Session {id} was not found in codex.")),
        };
        // When the copier reads the second fork
        let whole = with_history(fork_of_fork, &load).unwrap();
        // Then the history from every session before it comes first, in order
        assert_eq!(
            prompts(&whole),
            [
                "in the first session",
                "in the first fork",
                "in the second fork"
            ]
        );
    }

    #[test]
    fn fork_whose_source_is_missing_is_not_read_without_its_history() {
        // Given a fork whose source session is not in the store
        let fork = rollout("fork-1", Some(("base-1", 2)), &[(3, "after the fork")]);
        let load = |id: &str| Err(format!("Session {id} was not found in codex."));
        // When the copier reads the fork
        let result = with_history(fork, &load);
        // Then it reports the missing source instead of a fork without its history
        assert!(result.is_err_and(|why| why.contains("base-1") && why.contains("not found")));
    }

    #[test]
    fn forks_that_loop_are_followed_until_they_repeat() {
        // Given fork-a forked from fork-b, which names fork-a as its own source
        let fork_a = rollout("fork-a", Some(("fork-b", 4)), &[(5, "in fork-a")]);
        let fork_b = rollout("fork-b", Some(("fork-a", 2)), &[(3, "in fork-b")]);
        let load = |id: &str| match id {
            "fork-a" => Ok(fork_a.clone()),
            "fork-b" => Ok(fork_b.clone()),
            _ => Err(format!("Session {id} was not found in codex.")),
        };
        // When the copier reads fork-a
        let whole = with_history(fork_a.clone(), &load).unwrap();
        // Then it stops at fork-b, whose source was already read, and keeps the history up to there
        assert_eq!(prompts(&whole), ["in fork-b", "in fork-a"]);
    }

    #[test]
    fn session_missing_from_its_store_is_reported_as_not_found() {
        // Given an id no Claude Code session has
        let id = "herdr-session-transfer-missing-session";
        // When it is copied into Codex
        let result =
            TxcriptSessions.copy(HarnessId::ClaudeCode, id, HarnessId::Codex, Path::new("."));
        // Then the copier reports that it was not found
        assert!(result.is_err_and(|why| why.contains("not found")));
    }
}
