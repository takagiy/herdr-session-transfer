//! Transferring the session in one pane to another harness, in that pane.

use std::path::{Path, PathBuf};
use txcript::HarnessId;

/// What herdr knows about the pane being transferred.
pub struct Pane {
    /// herdr's name for the agent running in the pane.
    pub agent: Option<String>,
    /// The agent's session id, as reported to herdr.
    pub session_id: Option<String>,
    pub cwd: PathBuf,
}

/// The pane, through herdr.
pub trait Herdr {
    fn pane(&self) -> Result<Pane, String>;
    /// Whether the agent is a program started from the pane's shell, so the
    /// shell is left when it stops.
    fn agent_started_from_shell(&self) -> Result<bool, String>;
    fn stop_agent(&self) -> Result<(), String>;
    /// Type `command` into the pane's shell and run it.
    fn run(&self, command: &str) -> Result<(), String>;
}

/// The harnesses' session stores.
pub trait Sessions {
    /// Copy session `id` of `from` into `to`'s store; return the copy's id.
    fn copy(
        &self,
        from: HarnessId,
        id: &str,
        to: HarnessId,
        pane_cwd: &Path,
    ) -> Result<String, String>;
    /// The shell command that resumes session `id` in `harness`.
    fn resume_command(&self, harness: HarnessId, id: &str) -> String;
}

/// The harness of the agent running in `pane`.
///
/// # Errors
/// When no agent this plugin can transfer is running there.
pub fn source_harness(pane: &Pane) -> Result<HarnessId, String> {
    pane.agent
        .as_deref()
        .and_then(crate::harnesses::harness_for_agent)
        .ok_or_else(|| "No agent this plugin can transfer is running in this pane.".into())
}

/// Continue the pane's conversation in `to`, in the same pane.
///
/// # Errors
/// Why the pane was left as it was, for the user to read.
pub fn transfer(herdr: &impl Herdr, sessions: &impl Sessions, to: HarnessId) -> Result<(), String> {
    let pane = herdr.pane()?;
    let from = source_harness(&pane)?;
    let id = pane
        .session_id
        .ok_or("herdr does not know which session this pane holds.")?;
    if !herdr.agent_started_from_shell()? {
        return Err("The agent was not started from a shell in this pane, so stopping it would close the pane.".into());
    }
    let copy = sessions.copy(from, &id, to, &pane.cwd)?;
    let resume = sessions.resume_command(to, &copy);
    // Past this point the agent may be gone, so say how to reach the copy.
    herdr
        .stop_agent()
        .and_then(|()| herdr.run(&resume))
        .map_err(|why| format!("{why} To continue, run `{resume}` in the pane."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Log(RefCell<Vec<String>>);

    impl Log {
        fn push(&self, entry: String) {
            self.0.borrow_mut().push(entry);
        }
        fn entries(&self) -> Vec<String> {
            self.0.borrow().clone()
        }
    }

    struct FakeHerdr<'a> {
        agent: Option<&'static str>,
        session_id: Option<&'static str>,
        started_from_shell: bool,
        run_fails: bool,
        log: &'a Log,
    }

    impl Herdr for FakeHerdr<'_> {
        fn pane(&self) -> Result<Pane, String> {
            Ok(Pane {
                agent: self.agent.map(String::from),
                session_id: self.session_id.map(String::from),
                cwd: PathBuf::from("/work"),
            })
        }
        fn agent_started_from_shell(&self) -> Result<bool, String> {
            Ok(self.started_from_shell)
        }
        fn stop_agent(&self) -> Result<(), String> {
            self.log.push("stop agent".into());
            Ok(())
        }
        fn run(&self, command: &str) -> Result<(), String> {
            if self.run_fails {
                return Err("pane is gone".into());
            }
            self.log.push(format!("run {command}"));
            Ok(())
        }
    }

    struct FakeSessions<'a> {
        copy_fails: bool,
        log: &'a Log,
    }

    impl Sessions for FakeSessions<'_> {
        fn copy(
            &self,
            from: HarnessId,
            id: &str,
            to: HarnessId,
            _: &Path,
        ) -> Result<String, String> {
            if self.copy_fails {
                return Err("store rejected the copy".into());
            }
            self.log
                .push(format!("copy {} {id} to {}", from.as_str(), to.as_str()));
            Ok("copy-1".into())
        }
        fn resume_command(&self, harness: HarnessId, id: &str) -> String {
            format!("resume {} {id}", harness.as_str())
        }
    }

    fn claude_pane(log: &Log) -> FakeHerdr<'_> {
        FakeHerdr {
            agent: Some("claude"),
            session_id: Some("session-1"),
            started_from_shell: true,
            run_fails: false,
            log,
        }
    }

    fn working_sessions(log: &Log) -> FakeSessions<'_> {
        FakeSessions {
            copy_fails: false,
            log,
        }
    }

    #[test]
    fn claude_code_pane_continues_its_conversation_in_codex() {
        // Given a pane running Claude Code in session-1, started from the shell
        let log = Log::default();
        // When the user transfers it to Codex
        let result = transfer(
            &claude_pane(&log),
            &working_sessions(&log),
            HarnessId::Codex,
        );
        // Then session-1 is copied into Codex, the agent stops, and the pane
        // resumes the copy in Codex
        assert_eq!(result, Ok(()));
        assert_eq!(
            log.entries(),
            [
                "copy claude_code session-1 to codex",
                "stop agent",
                "run resume codex copy-1",
            ]
        );
    }

    /// The pane was left as it was: its agent never stopped and nothing ran.
    fn pane_untouched(log: &Log) -> bool {
        log.entries()
            .iter()
            .all(|entry| entry != "stop agent" && !entry.starts_with("run "))
    }

    #[test]
    fn failed_start_shows_the_command_that_continues() {
        // Given a pane running Claude Code that cannot start Codex once its
        // agent has stopped
        let log = Log::default();
        let herdr = FakeHerdr {
            run_fails: true,
            ..claude_pane(&log)
        };
        // When the user transfers it to Codex
        let result = transfer(&herdr, &working_sessions(&log), HarnessId::Codex);
        // Then the user is told the command that continues the copy
        assert!(result.is_err_and(|why| why.contains("resume codex copy-1")));
    }
    #[test]
    fn pane_without_a_known_agent_is_refused() {
        // Given a pane running an agent this plugin does not list
        let log = Log::default();
        let herdr = FakeHerdr {
            agent: Some("devin"),
            ..claude_pane(&log)
        };
        // When the user transfers it to Codex
        let result = transfer(&herdr, &working_sessions(&log), HarnessId::Codex);
        // Then the user is told why, and the pane stays as it was
        assert!(result.is_err_and(|why| why.contains("No agent")));
        assert!(pane_untouched(&log));
    }

    #[test]
    fn pane_whose_session_herdr_does_not_know_is_refused() {
        // Given a pane running Claude Code whose session id herdr never received
        let log = Log::default();
        let herdr = FakeHerdr {
            session_id: None,
            ..claude_pane(&log)
        };
        // When the user transfers it to Codex
        let result = transfer(&herdr, &working_sessions(&log), HarnessId::Codex);
        // Then the user is told why, and the pane keeps its agent
        assert!(result.is_err_and(|why| why.contains("session")));
        assert!(pane_untouched(&log));
    }

    #[test]
    fn agent_that_is_the_panes_own_process_is_refused() {
        // Given a pane whose own process is Claude Code, with no shell under it
        let log = Log::default();
        let herdr = FakeHerdr {
            started_from_shell: false,
            ..claude_pane(&log)
        };
        // When the user transfers it to Codex
        let result = transfer(&herdr, &working_sessions(&log), HarnessId::Codex);
        // Then the user is told why, and the pane keeps its agent
        assert!(result.is_err_and(|why| why.contains("shell")));
        assert!(pane_untouched(&log));
    }

    #[test]
    fn failed_copy_leaves_the_agent_running() {
        // Given a pane running Claude Code, and a Codex store that rejects writes
        let log = Log::default();
        let sessions = FakeSessions {
            copy_fails: true,
            log: &log,
        };
        // When the user transfers it to Codex
        let result = transfer(&claude_pane(&log), &sessions, HarnessId::Codex);
        // Then the user is told why, and the pane keeps its agent
        assert!(result.is_err_and(|why| why.contains("rejected")));
        assert!(pane_untouched(&log));
    }
}
