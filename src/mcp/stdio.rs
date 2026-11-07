use std::io::{self, BufRead, Write};

use anyhow::Result;
use serde_json::Value;

use super::protocol::{error_response, RpcError, PARSE_ERROR};
use super::{handle, start_background_indexing, Outcome, ServerState};
use crate::workspace::Workspace;

pub fn serve_stdio(ws: Workspace) -> Result<()> {
    let state = ServerState::new(ws);
    start_background_indexing(&state);
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout();
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        // Raw bytes: a line that is not UTF-8 gets a parse error, not a dead server.
        if stdin.read_until(b'\n', &mut buffer)? == 0 {
            return Ok(());
        }
        let reply = match std::str::from_utf8(&buffer) {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => match handle(&state, line) {
                Outcome::Reply(reply) => reply,
                _ => continue,
            },
            Err(_) => error_response(
                Value::Null,
                &RpcError::new(PARSE_ERROR, "Parse error: the request is not valid UTF-8"),
            ),
        };
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }
}
