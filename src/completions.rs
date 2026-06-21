use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionScript {
    pub shell: CompletionShell,
    pub script: String,
}

impl Display for CompletionScript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.script)
    }
}

pub fn generate(shell: CompletionShell) -> CompletionScript {
    let commands = [
        "index",
        "watch",
        "doctor",
        "init",
        "completions",
        "find-definition",
        "find-references",
        "get-outline",
        "expand-symbol",
        "impact",
        "validate",
        "validate-snippet",
        "stats",
        "search",
        "unused",
        "context-pack",
        "plan-query",
        "edit-prep",
        "diff-impact",
        "imports",
        "imported-by",
        "signature",
        "siblings",
        "tests-for",
        "connect",
        "export",
        "bench",
        "snapshot",
        "mcp",
        "mcp-http",
        "shell",
    ];
    let joined = commands.join(" ");
    let script = match shell {
        CompletionShell::Bash => format!(
            r#"_tessera()
{{
  local cur="${{COMP_WORDS[COMP_CWORD]}}"
  if [[ $COMP_CWORD -eq 1 ]]; then
    COMPREPLY=( $(compgen -W "{joined}" -- "$cur") )
  fi
}}
complete -F _tessera tessera
"#
        ),
        CompletionShell::Zsh => format!(
            r#"#compdef tessera
_arguments '1:command:({joined})' '*::arg:->args'
"#
        ),
        CompletionShell::Fish => {
            commands
                .iter()
                .map(|command| {
                    format!("complete -c tessera -f -n '__fish_use_subcommand' -a {command}")
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        }
        CompletionShell::Powershell => format!(
            r#"Register-ArgumentCompleter -Native -CommandName tessera -ScriptBlock {{
  param($wordToComplete, $commandAst, $cursorPosition)
  "{joined}".Split(" ") | Where-Object {{ $_ -like "$wordToComplete*" }} | ForEach-Object {{ [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }}
}}
"#
        ),
    };
    CompletionScript { shell, script }
}
