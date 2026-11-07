The MCP stdio server no longer exits when a request line is not valid UTF-8. It answers that line with a JSON-RPC parse error (`-32700`, `id: null`) and keeps serving the next requests.
