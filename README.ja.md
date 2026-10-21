<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="Tessera" width="420">
  </picture>
</p>

<h3 align="center">エージェントが「完了」と言ったら、Tessera が確かめる。</h3>

<p align="center">
  どの AI コーディングエージェントにも、編集前の決定的な影響範囲と編集後の検証を。<br>
  ローカルで動作、LLM 不使用、Apache-2.0。
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  日本語 ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

> **翻訳ドラフト**：この翻訳はネイティブスピーカーによるレビュー待ちです。内容に違いがある場合は[英語版 README](README.md) が正となります。修正の提案を歓迎します。
> *Draft translation awaiting review by a native speaker; the English README is authoritative.*

## なぜ必要か

コーディングエージェントは、コードの一部しか読まないまま編集します。grep して、いくつかのファイルを開き、関数を変更して、成功を報告します。古い引数のまま呼び出している別モジュールの呼び出し元は、あとになって CI やレビュー、本番環境で見つかります。

Tessera はエージェントに正確なローカルのコードマップを渡し、その作業をマップに照らして確認します。

- **編集前：** `blast` は変更が影響しうるすべての呼び出し元を影響度順に並べ、実行すべきテストを挙げます。`context` は本体・呼び出し元・呼び出し先を一度の読み取りで返します。
- **編集後：** `verify` は存在しない名前の呼び出し、引数の数の不一致、壊れた import、リネームで取り残された呼び出し箇所を検出します。フックが編集のたびにこれを実行し、問題が直るまでエージェントの「完了」を止めます。Claude Code、Codex、Gemini CLI、Copilot、Cursor などに対応。
- **サービス間：** `map` は HTTP ルート、gRPC メソッド、Kafka トピック、テーブルを両側のコードと結び付けます。クライアントがまだ呼んでいるルートを削除すると `verify` が検出します。
- **すべてのプルリクエストで：** GitHub Action が、変更が壊すもの、変更されたコードの呼び出し元、実行すべきテストをコメントします。

マップは決定的です。tree-sitter が解析し、SQLite が保存し、同じコードからは常に同じ答えが得られます。モデルは使わず、コードがマシンの外に出ることもありません。

## クイックスタート

任意のリポジトリで：

```sh
npx tessera-codegraph blast <symbol>
```

セットアップは不要です。最初のクエリが `.tessera/` にインデックスを作り、以降は変更分だけを更新します。続いてエージェントに接続します：

```sh
npx tessera-codegraph init
```

`init` は使用中のエージェントを検出し、設定変更の diff を見せてから、書き込みの前に確認を求めます。Claude Code ではプラグインも使えます：

```text
/plugin marketplace add iamsaquib8/tessera
/plugin install tessera@tessera
```

## コマンド

| コマンド | 答える問い |
| --- | --- |
| `tessera blast <symbol>` / `--diff <rev>` | この変更は何に影響するか？影響度順の呼び出し元と実行すべきテスト |
| `tessera context <symbol>` | 編集前に何を読むべきか？ |
| `tessera verify --changed` | 呼び出し箇所、引数の数、import を壊していないか？壊していれば終了コード 1 |
| `tessera find` / `refs` / `search` | どこで定義？誰が呼ぶ？どのシンボルが一致？ |
| `tessera path <from> <to>` | A から B に到達するか、どの経路で？サービス間のルートや gRPC も追跡 |
| `tessera map <route\|rpc\|topic\|table\|service>` | どのサービスがこのエンドポイント、メソッド、トピック、テーブルを使うか？ |
| `tessera ui` | 変更の影響とサービス間の契約マップをブラウザで表示 |
| `tessera init` | このプロジェクトで使うエージェントに Tessera を接続 |

クエリはすべて MCP ツール（`tessera mcp`）としても使え、どのコマンドも `--json` に対応しています。

## 対応エージェントと言語

Tessera はモデルを呼び出さないため、Claude、GPT、Gemini、Grok、DeepSeek、Qwen、ローカルモデルのいずれでも動作します。`tessera init` は 21 のエージェントに対応し、そのうち Claude Code、Codex CLI、Gemini CLI、GitHub Copilot CLI、Cursor、Qwen Code、Augment、Antigravity では編集のたびに自動でチェックします。その他のエージェントでは `tessera init --git-hook` でコミット前チェックを導入できます。一覧は[英語版 README](README.md#works-with-every-agent-and-every-model) を参照してください。

言語：TypeScript、TSX/React、JavaScript、Java、Python、Go、Rust、C、C++、C#、Ruby、PHP。TypeScript、JavaScript、Java はディープ層で、公開リポジトリにおいて高信頼度の結果がコンパイラの正解と **99.7%** 一致します（[精度](docs/accuracy.md)）。

## ドキュメント

- [最初の 5 分](docs/first-five-minutes.md)
- [編集安全フック](docs/hooks.md)
- [契約マップ](docs/contracts.md)
- [エージェントごとの設定](docs/integrations.md)
- [1.x の安定性の約束](docs/stability.md)
- [ロードマップ](ROADMAP.md)

## ライセンス

Apache-2.0。[LICENSE](LICENSE) を参照してください。
