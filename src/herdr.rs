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
    foreground_process_group_id: Option<u32>,
    foreground_processes: Vec<Process>,
}

#[derive(Deserialize)]
struct Process {
    pid: u32,
}

impl Processes {
    /// The foreground processes other than the shell, so stopping them leaves
    /// the pane open.
    pub fn to_stop(&self) -> Vec<u32> {
        self.foreground_processes
            .iter()
            .map(|p| p.pid)
            .filter(|&pid| Some(pid) != self.shell_pid)
            .collect()
    }

    /// Whether the foreground holds a program started from the shell, rather
    /// than the shell itself.
    pub fn started_from_shell(&self) -> bool {
        self.shell_pid.is_some()
            && !self.foreground_processes.is_empty()
            && self.to_stop().len() == self.foreground_processes.len()
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
        kill(&self.processes()?)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.processes()?.started_from_shell() {
            if Instant::now() > deadline {
                return Err("The agent did not stop within 5 seconds.".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn run(&self, command: &str) -> Result<(), String> {
        self.call(&["pane", "run", &self.pane_id, command])
            .map(drop)
    }
}

/// Force the processes in front of the shell to exit.
fn kill(processes: &Processes) -> Result<(), String> {
    let status = if cfg!(windows) {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/T", "/F"]);
        for pid in processes.to_stop() {
            cmd.args(["/PID", &pid.to_string()]);
        }
        cmd.output()
    } else {
        let group = processes
            .foreground_process_group_id
            .ok_or("herdr did not report the agent's process group.")?;
        Command::new("kill")
            .args(["-KILL", "--", &format!("-{group}")])
            .output()
    }
    .map_err(|e| format!("could not stop the agent: {e}"))?;
    if status.status.success() {
        Ok(())
    } else {
        Err(format!(
            "could not stop the agent: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        ))
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
    fn shell_is_left_out_of_the_processes_to_stop() {
        // Given `pane process-info` reports the shell beside another process
        let json = r#"{"result":{"process_info":{"foreground_processes":[{"pid":32676},{"pid":8948}],"shell_pid":32676}}}"#;
        // When the reader lists the processes to stop
        let processes = read_processes(json).unwrap();
        // Then only the other process is listed
        assert_eq!(processes.to_stop(), [8948]);
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
