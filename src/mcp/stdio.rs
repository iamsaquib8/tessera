use std::io::{self, BufRead, Write};

use anyhow::Result;

use super::{handle, Outcome, ServerState};
use crate::workspace::Workspace;

pub fn serve_stdio(ws: Workspace) -> Result<()> {
    let state = ServerState::new(ws);
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Outcome::Reply(reply) = handle(&state, &line) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
