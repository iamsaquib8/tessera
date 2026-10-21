<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="Tessera" width="420">
  </picture>
</p>

<h3 align="center">Seu agente diz que terminou. O Tessera confere.</h3>

<p align="center">
  Raio de impacto determinístico antes de cada edição e verificação depois, para qualquer agente de programação com IA.<br>
  Local, sem LLM, Apache-2.0.
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.es.md">Español</a> ·
  Português (Brasil)
</p>

> **Tradução em rascunho**: esta tradução aguarda revisão de um falante nativo. Em caso de divergência, vale o [README em inglês](README.md). Correções são bem-vindas.
> *Draft translation awaiting review by a native speaker; the English README is authoritative.*

## Por quê

Agentes de programação editam código que leram só em parte. Fazem grep, abrem alguns arquivos, mudam uma função e relatam sucesso. A chamada em outro módulo que ainda passa os argumentos antigos aparece depois: no CI, na revisão ou em produção.

O Tessera dá ao agente um mapa local e exato do código e confere o trabalho dele contra esse mapa:

- **Antes de editar:** `blast` ordena por impacto todos os chamadores que uma mudança pode afetar e lista os testes a rodar. `context` devolve o corpo, os chamadores e as funções chamadas numa única leitura.
- **Depois de editar:** `verify` encontra chamadas a nomes que não existem, quantidade errada de argumentos, imports quebrados e chamadas que uma renomeação deixou para trás. Hooks rodam a verificação após cada edição e seguram o agente em "terminei" até o problema ser corrigido, no Claude Code, Codex, Gemini CLI, Copilot, Cursor e outros.
- **Entre serviços:** `map` liga rotas HTTP, métodos gRPC, tópicos Kafka e tabelas ao código dos dois lados; `verify` detecta uma rota removida que um cliente ainda chama.
- **Em todo pull request:** a GitHub Action comenta o que a mudança quebra, quem chama o código alterado e quais testes rodar.

O mapa é determinístico. O tree-sitter analisa, o SQLite armazena, e o mesmo código sempre dá a mesma resposta. Nenhum modelo o constrói e nada sai da sua máquina.

## Início rápido

Dentro de qualquer repositório:

```sh
npx tessera-codegraph blast <symbol>
```

Não há etapa de configuração: a primeira consulta cria o índice em `.tessera/` e as seguintes só atualizam o que mudou. Depois conecte aos seus agentes:

```sh
npx tessera-codegraph init
```

O `init` encontra os agentes que você usa, mostra o diff de cada mudança de configuração e pergunta antes de gravar. No Claude Code, você também pode instalar o plugin:

```text
/plugin marketplace add iamsaquib8/tessera
/plugin install tessera@tessera
```

## Comandos

| Comando | Responde |
| --- | --- |
| `tessera blast <symbol>` / `--diff <rev>` | O que esta mudança afeta? Chamadores por impacto e testes a rodar |
| `tessera context <symbol>` | O que preciso ler antes de editar isto? |
| `tessera verify --changed` | A mudança quebrou uma chamada, a quantidade de argumentos ou um import? Sai com 1 se sim |
| `tessera find` / `refs` / `search` | Onde está definido? Quem chama? Quais símbolos correspondem? |
| `tessera path <from> <to>` | A chega a B, e como? Segue rotas e chamadas gRPC entre serviços |
| `tessera map <route\|rpc\|topic\|table\|service>` | Quais serviços usam este endpoint, método, tópico ou tabela? |
| `tessera ui` | O impacto da mudança e o mapa de contratos entre serviços, no navegador |
| `tessera init` | Conecta o Tessera aos agentes que este projeto usa |

As consultas também são ferramentas MCP (`tessera mcp`), e todo comando aceita `--json`.

## Agentes e linguagens

O Tessera nunca chama um modelo, então funciona com Claude, GPT, Gemini, Grok, DeepSeek, Qwen e modelos locais. O `tessera init` configura 21 agentes; no Claude Code, Codex CLI, Gemini CLI, GitHub Copilot CLI, Cursor, Qwen Code, Augment e Antigravity, ele também confere cada edição. Para os demais, `tessera init --git-hook` instala uma verificação antes de cada commit. A lista completa está no [README em inglês](README.md#works-with-every-agent-and-every-model).

Linguagens: TypeScript, TSX/React, JavaScript, Java, Python, Go, Rust, C, C++, C#, Ruby, PHP. TypeScript, JavaScript e Java estão no nível profundo: em repositórios públicos, os resultados de alta confiança batem com a verdade do compilador em **99,7%** das vezes ([precisão](docs/accuracy.md)).

## Documentação

- [Os primeiros cinco minutos](docs/first-five-minutes.md)
- [Hooks de edição segura](docs/hooks.md)
- [Mapa de contratos](docs/contracts.md)
- [Configuração por agente](docs/integrations.md)
- [Compromisso de estabilidade 1.x](docs/stability.md)
- [Roadmap](ROADMAP.md)

## Licença

Apache-2.0. Veja [LICENSE](LICENSE).
