//! The pane, through the herdr CLI.

use crate::transfer::{Herdr, Pane};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct Reply<T> {
    result: T,
}

#[derive(Deserialize)]
struct PaneResult {
    pane: PaneInfo,
}

#[derive(Deserialize)]
struct PaneInfo {
    agent: Option<String>,
    agent_session: Option<AgentSession>,
    cwd: PathBuf,
}

#[derive(Deserialize)]
struct AgentSession {
    value: String,
}

/// Read the output of `herdr pane get`.
pub fn read_pane(json: &str) -> Result<Pane, String> {
    let reply: Reply<PaneResult> = serde_json::from_str(json)
        .map_err(|e| format!("unexpected `herdr pane get` output: {e}"))?;
    let pane = reply.result.pane;
    Ok(Pane {
        agent: pane.agent,
        session_id: pane.agent_session.map(|s| s.value),
        cwd: pane.cwd,
    })
}

#[derive(Deserialize)]
struct ProcessResult {
    process_info: Processes,
}

/// The pane's shell and the processes in front of it.
#[derive(Deserialize)]
pub struct Processes {
    shell_pid: Option<u32>,
    foreground_processes: Vec<Process>,
}

#[derive(Deserialize)]
struct Process {
    pid: u32,
}

impl Processes {
    fn shell_is_foreground(&self) -> bool {
        self.foreground_processes.len() == 1
            && self.shell_pid == Some(self.foreground_processes[0].pid)
    }

    /// Whether the foreground holds a program started from the shell, rather
    /// than the shell itself.
    pub fn started_from_shell(&self) -> bool {
        self.shell_pid.is_some()
            && !self.foreground_processes.is_empty()
            && self
                .foreground_processes
                .iter()
                .all(|p| Some(p.pid) != self.shell_pid)
    }
}

#[derive(Deserialize)]
struct Context {
    focused_pane_id: Option<String>,
}

/// The pane under a popup, from the `HERDR_PLUGIN_CONTEXT_JSON` herdr gives it.
pub fn read_context_pane(json: &str) -> Result<String, String> {
    let context: Context =
        serde_json::from_str(json).map_err(|e| format!("unexpected plugin context: {e}"))?;
    context
        .focused_pane_id
        .ok_or_else(|| "herdr did not say which pane the picker was opened over.".into())
}

/// Read the output of `herdr pane process-info`.
pub fn read_processes(json: &str) -> Result<Processes, String> {
    let reply: Reply<ProcessResult> = serde_json::from_str(json)
        .map_err(|e| format!("unexpected `herdr pane process-info` output: {e}"))?;
    Ok(reply.result.process_info)
}

/// One pane, driven through the herdr binary that started this plugin.
pub struct HerdrCli {
    pub bin: PathBuf,
    pub pane_id: String,
}

impl HerdrCli {
    fn call(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new(&self.bin)
            .args(args)
            .output()
            .map_err(|e| format!("could not run herdr: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() || stdout.contains(r#""error":"#) {
            return Err(format!(
                "`herdr {}` failed: {}{}",
                args.join(" "),
                stdout.trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(stdout)
    }

    fn processes(&self) -> Result<Processes, String> {
        read_processes(&self.call(&["pane", "process-info", "--pane", &self.pane_id])?)
    }

    /// Show `title` and `body` as a herdr notification.
    pub fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        self.call(&["notification", "show", title, "--body", body])
            .map(drop)
    }
}

impl Herdr for HerdrCli {
    fn pane(&self) -> Result<Pane, String> {
        read_pane(&self.call(&["pane", "get", &self.pane_id])?)
    }

    fn agent_started_from_shell(&self) -> Result<bool, String> {
        Ok(self.processes()?.started_from_shell())
    }

    fn stop_agent(&self) -> Result<(), String> {
        // One key per request: Claude Code reads Ctrl+C bytes that arrive together as one press.
        // Claude Code attached to a background session detaches to agent view first, so it needs more presses.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut exit_request = Ok(());
        while !self.processes()?.shell_is_foreground() {
            if Instant::now() > deadline {
                exit_request?;
                return Err("The agent did not exit within 5 seconds. Exit it normally before trying again.".into());
            }
            exit_request = exit_request.and_then(|()| {
                self.call(&["agent", "send-keys", &self.pane_id, "ctrl+c"])
                    .map(drop)
            });
        }
        Ok(())
    }

    fn run(&self, command: &str) -> Result<(), String> {
        self.call(&["pane", "run", &self.pane_id, command])
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_PANE: &str = r#"{"id":"cli:pane:get","result":{"pane":{"agent":"claude","agent_session":{"agent":"claude","kind":"id","source":"herdr:claude","value":"efcbb07e-a664-4f64-8f61-f1c810523780"},"agent_status":"idle","cwd":"C:\\Users\\takag\\","focused":false,"pane_id":"w3:p19","revision":29,"tab_id":"w3:t9","terminal_title":"x","workspace_id":"w3"},"type":"pane_info"}}"#;

    #[test]
    fn pane_with_agent_session_gives_agent_session_and_cwd() {
        // Given `pane get` reports Claude Code with an agent_session
        // When the reader reads it
        let pane = read_pane(CLAUDE_PANE).unwrap();
        // Then it gives the agent name, the session id, and the cwd
        assert_eq!(pane.agent.as_deref(), Some("claude"));
        assert_eq!(
            pane.session_id.as_deref(),
            Some("efcbb07e-a664-4f64-8f61-f1c810523780")
        );
        assert_eq!(pane.cwd, std::path::PathBuf::from(r"C:\Users\takag\"));
    }

    #[test]
    fn pane_without_agent_session_gives_no_session_id() {
        // Given `pane get` reports Codex without an agent_session
        let json = r#"{"result":{"pane":{"agent":"codex","agent_status":"idle","cwd":"/work","pane_id":"w3:p1W"},"type":"pane_info"}}"#;
        // When the reader reads it
        let pane = read_pane(json).unwrap();
        // Then it gives the agent name with no session id
        assert_eq!(pane.agent.as_deref(), Some("codex"));
        assert_eq!(pane.session_id, None);
    }

    #[test]
    fn pane_without_agent_gives_no_agent() {
        // Given `pane get` reports a plain shell pane
        let json = r#"{"result":{"pane":{"agent_status":"unknown","cwd":"/work","pane_id":"w2:p5"},"type":"pane_info"}}"#;
        // When the reader reads it
        let pane = read_pane(json).unwrap();
        // Then it gives a pane with no agent
        assert_eq!(pane.agent, None);
    }

    const CLAUDE_PROCESSES: &str = r#"{"id":"cli:pane:process_info","result":{"process_info":{"foreground_process_group_id":8948,"foreground_processes":[{"argv":["claude.exe"],"cwd":"/work","name":"claude.exe","pid":8948}],"pane_id":"w3:p2A","shell_pid":20732},"type":"pane_process_info"}}"#;

    #[test]
    fn foreground_process_other_than_the_shell_was_started_from_it() {
        // Given `pane process-info` reports claude.exe in front of the shell
        // When the reader reads it
        let processes = read_processes(CLAUDE_PROCESSES).unwrap();
        // Then it says a program was started from the shell
        assert!(processes.started_from_shell());
    }

    #[test]
    fn shell_in_the_foreground_has_started_nothing() {
        // Given `pane process-info` reports the shell itself in the foreground
        let json = r#"{"result":{"process_info":{"foreground_process_group_id":32676,"foreground_processes":[{"name":"cmd.exe","pid":32676}],"pane_id":"w2:p5","shell_pid":32676},"type":"pane_process_info"}}"#;
        // When the reader reads it
        let processes = read_processes(json).unwrap();
        // Then it says nothing was started from the shell
        assert!(!processes.started_from_shell());
    }

    #[test]
    fn shell_beside_another_process_is_not_a_separate_foreground_agent() {
        // Given `pane process-info` reports the shell beside another process
        let json = r#"{"result":{"process_info":{"foreground_processes":[{"pid":32676},{"pid":8948}],"shell_pid":32676}}}"#;
        // When the reader checks whether an agent is running in front of the shell
        let processes = read_processes(json).unwrap();
        // Then a group containing the shell is not a separate foreground agent
        assert!(!processes.started_from_shell());
    }
    #[test]
    fn missing_foreground_processes_do_not_mean_the_shell_is_ready() {
        // Given the foreground process is temporarily unknown during shutdown
        let processes = read_processes(
            r#"{"result":{"process_info":{"shell_pid":42,"foreground_processes":[]}}}"#,
        )
        .unwrap();
        // When transfer checks whether it can start the next harness
        // Then it keeps waiting for the shell itself
        assert!(!processes.shell_is_foreground());
    }

    #[test]
    fn only_the_shell_in_the_foreground_is_ready_for_the_next_harness() {
        // Given the agent has exited and the shell is the only foreground process
        let processes = read_processes(
            r#"{"result":{"process_info":{"shell_pid":42,"foreground_processes":[{"pid":42}]}}}"#,
        )
        .unwrap();
        // When transfer checks whether it can start the next harness
        // Then the shell is ready
        assert!(processes.shell_is_foreground());
    }

    /// A herdr whose agent exits after `exit_presses` Ctrl+C presses. Press `n` fails with
    /// the error in `failures` when listed there.
    #[cfg(windows)]
    fn fake_herdr(exit_presses: usize, failures: &[(usize, &str)]) -> (HerdrCli, PathBuf) {
        let dir = std::env::temp_dir().join(format!("herdr-transfer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        for (press, error) in failures {
            std::fs::write(dir.join(format!("fail{press}")), error).unwrap();
        }
        let shell =
            r#"{"result":{"process_info":{"shell_pid":42,"foreground_processes":[{"pid":42}]}}}"#;
        let agent = r#"{"result":{"process_info":{"shell_pid":42,"foreground_process_group_id":4294967294,"foreground_processes":[{"pid":4294967294}]}}}"#;
        let script = format!(
            r#"@echo off
cd /d "{}"
echo %*>> calls
if "%1 %2"=="agent send-keys" goto keys
if "%1 %2"=="pane process-info" goto info
echo Unexpected call >&2
exit /b 1
:keys
call :count
set /a n+=1
>presses echo %n%
if exist fail%n% (type fail%n% & exit /b 1)
exit /b 0
:info
call :count
if %n% GEQ {exit_presses} (echo {shell}) else (echo {agent})
exit /b 0
:count
set n=0
if exist presses set /p n=<presses
exit /b 0
"#,
            dir.display()
        );
        let bin = dir.join("herdr.cmd");
        std::fs::write(&bin, script).unwrap();
        (
            HerdrCli {
                bin,
                pane_id: "w1:p1".into(),
            },
            dir,
        )
    }

    #[cfg(windows)]
    fn presses(dir: &std::path::Path) -> usize {
        let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
        calls.matches("agent send-keys w1:p1 ctrl+c\r\n").count()
    }

    #[cfg(windows)]
    #[test]
    fn transfer_presses_ctrl_c_until_the_shell_returns() {
        // Given an agent that exits on its second Ctrl+C
        let (herdr, dir) = fake_herdr(2, &[]);
        // When transfer stops the agent
        herdr.stop_agent().unwrap();
        // Then it presses Ctrl+C through the agent API, once per request, until the shell returns
        assert_eq!(presses(&dir), 2);
    }

    #[cfg(windows)]
    #[test]
    fn agent_view_on_the_way_out_is_left_with_more_ctrl_c() {
        // Given Claude Code attached to a background session: two presses detach to agent view, two more leave it
        let (herdr, dir) = fake_herdr(4, &[]);
        // When transfer stops the agent
        herdr.stop_agent().unwrap();
        // Then it keeps pressing Ctrl+C until the shell returns
        assert_eq!(presses(&dir), 4);
    }

    #[cfg(windows)]
    #[test]
    fn first_exit_key_does_not_send_a_second_key_to_the_shell() {
        // Given an agent that exits on the first Ctrl+C
        let (herdr, dir) = fake_herdr(1, &[]);
        // When transfer stops the agent
        herdr.stop_agent().unwrap();
        // Then it sends no second key after the agent has exited
        assert_eq!(presses(&dir), 1);
    }

    #[cfg(windows)]
    #[test]
    fn exit_between_the_check_and_second_key_is_successful() {
        // Given an agent that exits just as the second key reaches it
        let (herdr, dir) = fake_herdr(2, &[(2, r#"{"error":"agent_not_ready"}"#)]);
        // When the second key request reports agent_not_ready
        let result = herdr.stop_agent();
        // Then the returned shell makes the normal exit successful
        assert_eq!(presses(&dir), 2);
        assert_eq!(result, Ok(()));
    }

    #[cfg(windows)]
    #[test]
    fn failed_key_is_reported_when_the_agent_is_still_running() {
        // Given an agent that stays in front of its shell
        let (herdr, dir) = fake_herdr(1000, &[(2, r#"{"error":"transport failed"}"#)]);
        // When the second key request fails
        let error = herdr.stop_agent().unwrap_err();
        // Then the original API error is reported and no further key is sent
        assert!(error.contains("transport failed"), "{error}");
        assert_eq!(presses(&dir), 2);
    }

    #[cfg(windows)]
    #[test]
    fn agent_that_does_not_exit_normally_is_left_running() {
        // Given an agent that ignores the normal exit request
        let (herdr, dir) = fake_herdr(1000, &[]);
        // When transfer tries to stop it
        let error = herdr.stop_agent().unwrap_err();
        // Then it reports the failure without starting another harness
        assert!(error.contains("did not exit"), "{error}");
        let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
        assert!(!calls.contains("pane run"), "{calls}");
    }

    #[test]
    fn picker_context_gives_the_pane_under_it() {
        // Given the plugin context herdr gives a popup opened over pane w3:p2A
        let json = r#"{"workspace_id":"w3","tab_id":"w3:tK","focused_pane_id":"w3:p2A","focused_pane_cwd":"/work","focused_pane_agent":"claude","invocation_source":"api"}"#;
        // When the reader reads it
        let pane_id = read_context_pane(json).unwrap();
        // Then it gives that pane
        assert_eq!(pane_id, "w3:p2A");
    }
}
