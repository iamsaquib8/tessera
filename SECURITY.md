# Security Policy

Tessera is a local-first tool. It indexes source files into a SQLite database on your machine and does not require a cloud service.

## Reporting a Vulnerability

Please do not open a public issue for security-sensitive reports.

Send a private report through GitHub Security Advisories for the repository. Include:

- affected version or commit
- operating system
- reproduction steps
- expected and actual behavior
- any relevant logs or sample files

## Scope

Security-sensitive issues include:

- unexpected network access
- unsafe handling of repository contents
- path traversal or arbitrary file writes
- malformed MCP input causing code execution
- dependency vulnerabilities with practical impact

Parser inaccuracies and ordinary crashes are usually normal bugs unless they cross one of those boundaries.
