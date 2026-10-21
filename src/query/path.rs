use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;

use super::legacy;
use crate::output::AgentText;
use crate::types::ConnectResult;

const DEFAULT_DEPTH: usize = 8;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PathArgs {
    pub from: String,
    pub to: String,
    pub depth: usize,
}

pub fn run(conn: &Connection, args: &PathArgs) -> Result<ConnectResult> {
    let depth = if args.depth == 0 {
        DEFAULT_DEPTH
    } else {
        args.depth
    };
    legacy::connect_conn(conn, &args.from, &args.to, depth)
}

impl AgentText for ConnectResult {
    fn agent_text(&self) -> String {
        self.to_string()
    }
}
