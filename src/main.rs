//! herdr plugin popup: ask which harness to transfer the pane's agent
//! session to, then continue it there in the same pane.

mod harnesses;
mod herdr;
mod picker;
mod sessions;
mod transfer;

use std::path::PathBuf;
use std::process::ExitCode;
use transfer::Herdr;

fn main() -> ExitCode {
    let context = std::env::var("HERDR_PLUGIN_CONTEXT_JSON").unwrap_or_default();
    let pane_id = herdr::read_context_pane(&context);
    let herdr = herdr::HerdrCli {
        bin: std::env::var_os("HERDR_BIN_PATH")
            .map_or_else(|| PathBuf::from("herdr"), PathBuf::from),
        pane_id: pane_id.clone().unwrap_or_default(),
    };
    let result = pane_id.and_then(|_| {
        let from = transfer::source_harness(&herdr.pane()?)?;
        match picker::pick(from)? {
            Some(to) => {
                // Finding the session can take seconds; show that it started.
                println!(
                    "\r\n\r\n Transferring to {}...",
                    harnesses::display_name(to)
                );
                transfer::transfer(&herdr, &sessions::TxcriptSessions, to)
            }
            None => Ok(()),
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            let _ = herdr.notify("Session not transferred", &why);
            ExitCode::FAILURE
        }
    }
}
