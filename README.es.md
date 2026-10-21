<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="Tessera" width="420">
  </picture>
</p>

<h3 align="center">Tu agente dice que terminó. Tessera lo comprueba.</h3>

<p align="center">
  Alcance de impacto determinista antes de cada edición y verificación después, para cualquier agente de programación con IA.<br>
  Local, sin LLM, Apache-2.0.
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  Español ·
  <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

> **Traducción en borrador**: esta traducción espera la revisión de un hablante nativo. Si hay diferencias, manda el [README en inglés](README.md). Las correcciones son bienvenidas.
> *Draft translation awaiting review by a native speaker; the English README is authoritative.*

## Por qué

Los agentes de programación editan código que solo han leído en parte. Hacen grep, abren algunos archivos, cambian una función y anuncian éxito. La llamada en otro módulo que todavía pasa los argumentos antiguos aparece después: en CI, en la revisión o en producción.

Tessera le da al agente un mapa local y exacto del código, y comprueba su trabajo contra ese mapa:

- **Antes de editar:** `blast` ordena por impacto todos los llamadores que un cambio puede afectar y lista las pruebas que hay que ejecutar. `context` devuelve el cuerpo, los llamadores y las funciones llamadas en una sola lectura.
- **Después de editar:** `verify` detecta llamadas a nombres que no existen, números de argumentos incorrectos, imports rotos y llamadas que un renombrado dejó atrás. Los hooks lo ejecutan tras cada edición y no dejan que el agente diga "terminé" hasta que el problema esté resuelto, en Claude Code, Codex, Gemini CLI, Copilot, Cursor y más.
- **Entre servicios:** `map` enlaza rutas HTTP, métodos gRPC, topics de Kafka y tablas con el código de ambos lados; `verify` detecta una ruta eliminada que un cliente todavía llama.
- **En cada pull request:** la GitHub Action comenta qué rompe el cambio, quién llama al código modificado y qué pruebas ejecutar.

El mapa es determinista. tree-sitter analiza, SQLite almacena, y el mismo código siempre da la misma respuesta. Ningún modelo lo construye y nada sale de tu máquina.

## Inicio rápido

Dentro de cualquier repositorio:

```sh
npx tessera-codegraph blast <symbol>
```

No hay paso de configuración: la primera consulta crea el índice en `.tessera/` y las siguientes solo actualizan lo que cambió. Después conéctalo a tus agentes:

```sh
npx tessera-codegraph init
```

`init` encuentra los agentes que usas, muestra el diff de cada cambio de configuración y pregunta antes de escribir. En Claude Code también puedes instalar el plugin:

```text
/plugin marketplace add iamsaquib8/tessera
/plugin install tessera@tessera
```

## Comandos

| Comando | Responde |
| --- | --- |
| `tessera blast <symbol>` / `--diff <rev>` | ¿Qué afecta este cambio? Llamadores ordenados por impacto y pruebas a ejecutar |
| `tessera context <symbol>` | ¿Qué debo leer antes de editar esto? |
| `tessera verify --changed` | ¿El cambio rompió una llamada, un número de argumentos o un import? Sale con 1 si es así |
| `tessera find` / `refs` / `search` | ¿Dónde se define? ¿Quién lo llama? ¿Qué símbolos coinciden? |
| `tessera path <from> <to>` | ¿A llega a B, y cómo? Sigue rutas y llamadas gRPC entre servicios |
| `tessera map <route\|rpc\|topic\|table\|service>` | ¿Qué servicios usan este endpoint, método, topic o tabla? |
| `tessera ui` | El impacto del cambio y el mapa de contratos entre servicios, en el navegador |
| `tessera init` | Conecta Tessera a los agentes que usa este proyecto |

Las consultas también son herramientas MCP (`tessera mcp`), y todos los comandos aceptan `--json`.

## Agentes y lenguajes

Tessera nunca llama a un modelo, así que funciona con Claude, GPT, Gemini, Grok, DeepSeek, Qwen y modelos locales. `tessera init` configura 21 agentes; en Claude Code, Codex CLI, Gemini CLI, GitHub Copilot CLI, Cursor, Qwen Code, Augment y Antigravity, además comprueba cada edición. Para los demás, `tessera init --git-hook` instala una comprobación antes de cada commit. La lista completa está en el [README en inglés](README.md#works-with-every-agent-and-every-model).

Lenguajes: TypeScript, TSX/React, JavaScript, Java, Python, Go, Rust, C, C++, C#, Ruby, PHP. TypeScript, JavaScript y Java están en el nivel profundo: en repositorios públicos, los resultados de alta confianza coinciden con la verdad del compilador el **99,7 %** de las veces ([precisión](docs/accuracy.md)).

## Documentación

- [Los primeros cinco minutos](docs/first-five-minutes.md)
- [Hooks de edición segura](docs/hooks.md)
- [Mapa de contratos](docs/contracts.md)
- [Configuración por agente](docs/integrations.md)
- [Compromiso de estabilidad 1.x](docs/stability.md)
- [Hoja de ruta](ROADMAP.md)

## Licencia

Apache-2.0. Consulta [LICENSE](LICENSE).
