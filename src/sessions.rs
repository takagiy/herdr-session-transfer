//! Copying sessions between harness stores with the txcript library.

use std::path::{Path, PathBuf};
use txcript::{Common, HarnessId, Transcript, local};

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
        let found = local::discover_harness(from)
            .map_err(|e| format!("Could not list {} sessions: {e}", from.as_str()))?
            .into_iter()
            .find(|session| session.meta.id == id)
            .ok_or_else(|| format!("Session {id} was not found in {}.", from.as_str()))?;
        let source = found
            .read()
            .map_err(|e| format!("Could not read session {id}: {e}"))?;
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
