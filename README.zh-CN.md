<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="Tessera" width="420">
  </picture>
</p>

<h3 align="center">你的 AI 助手说“完成了”。Tessera 来检查。</h3>

<p align="center">
  为任何 AI 编程助手提供确定性的影响范围分析（编辑前）和验证（编辑后）。<br>
  本地运行，不调用大模型，Apache-2.0 许可。
</p>

<p align="center">
  <a href="README.md">English</a> ·
  简体中文 ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

> **草稿翻译**：本译文尚待母语者审校。如有出入，以[英文 README](README.md) 为准。欢迎提交修正。
> *Draft translation awaiting review by a native speaker; the English README is authoritative.*

## 为什么需要它

AI 编程助手常常在只读了一部分代码的情况下就开始修改。它们 grep 一下、打开几个文件、改掉一个函数，然后报告成功。另一个模块里仍在用旧参数调用这个函数的地方，往往要到 CI、代码评审甚至生产环境才暴露出来。

Tessera 为助手提供一份精确的本地代码地图，并据此检查它的工作：

- **编辑前：** `blast` 按影响程度列出改动可能波及的所有调用方，以及需要运行的测试。`context` 一次性返回函数体、调用方和被调用方，省去五次打开文件。
- **编辑后：** `verify` 发现对不存在名称的调用、参数个数错误、失效的 import，以及重命名后遗漏的调用点。钩子（hooks）在每次编辑后运行它，并在问题修复之前阻止助手宣布“完成”。支持 Claude Code、Codex、Gemini CLI、Copilot、Cursor 等。
- **跨服务：** `map` 把 HTTP 路由、gRPC 方法、Kafka 主题和数据表与两端的代码关联起来；`verify` 能发现客户端仍在调用、却已被删除的路由。
- **每个 Pull Request：** GitHub Action 会评论这次改动破坏了什么、谁调用了被修改的代码、应运行哪些测试。

这份地图是确定性的：tree-sitter 解析，SQLite 存储，同样的代码永远得到同样的答案。没有模型参与构建，代码也不会离开你的机器。

## 快速开始

在任意代码仓库中：

```sh
npx tessera-codegraph blast <symbol>
```

无需任何配置：第一次查询会在 `.tessera/` 中建立索引，之后的查询只刷新变化的部分。然后把它接入你的助手：

```sh
npx tessera-codegraph init
```

`init` 会找到你使用的助手，展示每处配置改动的 diff，并在写入前征求确认。在 Claude Code 中也可以安装插件：

```text
/plugin marketplace add iamsaquib8/tessera
/plugin install tessera@tessera
```

## 命令

| 命令 | 回答的问题 |
| --- | --- |
| `tessera blast <symbol>` / `--diff <rev>` | 这次改动会影响什么？按影响排序的调用方和要运行的测试 |
| `tessera context <symbol>` | 修改它之前需要读些什么？ |
| `tessera verify --changed` | 改动是否破坏了调用点、参数个数或 import？有问题时退出码为 1 |
| `tessera find` / `refs` / `search` | 定义在哪？谁在调用？哪些符号匹配？ |
| `tessera path <from> <to>` | A 能否到达 B，如何到达？可跨服务追踪路由和 gRPC |
| `tessera map <route\|rpc\|topic\|table\|service>` | 哪些服务调用了这个接口、方法、主题或数据表？ |
| `tessera ui` | 在浏览器中查看改动影响和服务之间的契约图 |
| `tessera init` | 把 Tessera 接入本项目使用的助手 |

所有查询同时也是 MCP 工具（`tessera mcp`），每个命令都支持 `--json`。

## 支持的助手与语言

Tessera 从不调用模型，因此 Claude、GPT、Gemini、Grok、DeepSeek、Qwen 和本地模型都能使用。`tessera init` 支持 21 个助手；其中 Claude Code、Codex CLI、Gemini CLI、GitHub Copilot CLI、Cursor、Qwen Code、Augment 和 Antigravity 还会在每次编辑后自动检查。其他助手可以使用 `tessera init --git-hook` 安装提交前检查。完整列表见[英文 README](README.md#works-with-every-agent-and-every-model)。

语言：TypeScript、TSX/React、JavaScript、Java、Python、Go、Rust、C、C++、C#、Ruby、PHP。TypeScript、JavaScript 和 Java 属于深度层级：在公开仓库上，高置信度结果与编译器基准的一致率为 **99.7%**（[准确度](docs/accuracy.md)）。

## 文档

- [五分钟上手](docs/first-five-minutes.md)
- [编辑安全钩子](docs/hooks.md)
- [契约图](docs/contracts.md)
- [各助手的配置](docs/integrations.md)
- [1.x 稳定性承诺](docs/stability.md)
- [路线图](ROADMAP.md)

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
