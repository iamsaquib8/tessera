<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="Tessera" width="420">
  </picture>
</p>

<h3 align="center">에이전트가 "완료"라고 하면, Tessera가 확인합니다.</h3>

<p align="center">
  모든 AI 코딩 에이전트에 편집 전 결정적인 영향 범위 분석과 편집 후 검증을 제공합니다.<br>
  로컬 실행, LLM 미사용, Apache-2.0.
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.ja.md">日本語</a> ·
  한국어 ·
  <a href="README.es.md">Español</a> ·
  <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

> **번역 초안**: 이 번역은 원어민 검토를 기다리고 있습니다. 내용이 다를 경우 [영문 README](README.md)가 기준입니다. 수정 제안을 환영합니다.
> *Draft translation awaiting review by a native speaker; the English README is authoritative.*

## 왜 필요한가

코딩 에이전트는 코드의 일부만 읽은 채로 편집합니다. grep을 하고, 파일 몇 개를 열고, 함수를 바꾼 뒤 성공을 보고합니다. 다른 모듈에서 여전히 예전 인자로 호출하는 코드는 나중에 CI, 코드 리뷰, 혹은 운영 환경에서 드러납니다.

Tessera는 에이전트에게 정확한 로컬 코드 지도를 주고, 그 지도로 작업을 검사합니다.

- **편집 전:** `blast`는 변경이 영향을 줄 수 있는 모든 호출자를 영향도 순으로 보여주고 실행할 테스트를 알려줍니다. `context`는 본문, 호출자, 피호출자를 한 번에 돌려줍니다.
- **편집 후:** `verify`는 존재하지 않는 이름 호출, 잘못된 인자 개수, 깨진 import, 이름 변경 후 남은 호출 지점을 찾아냅니다. 훅(hook)이 편집할 때마다 이를 실행하고, 문제가 고쳐질 때까지 에이전트가 "완료"하지 못하게 합니다. Claude Code, Codex, Gemini CLI, Copilot, Cursor 등을 지원합니다.
- **서비스 간:** `map`은 HTTP 라우트, gRPC 메서드, Kafka 토픽, 테이블을 양쪽 코드와 연결합니다. 클라이언트가 아직 호출하는 라우트를 삭제하면 `verify`가 잡아냅니다.
- **모든 풀 리퀘스트에서:** GitHub Action이 변경이 무엇을 깨뜨리는지, 변경된 코드의 호출자, 실행할 테스트를 댓글로 남깁니다.

이 지도는 결정적입니다. tree-sitter가 파싱하고 SQLite가 저장하며, 같은 코드는 언제나 같은 답을 냅니다. 모델이 만들지 않으며, 코드는 여러분의 컴퓨터를 떠나지 않습니다.

## 빠른 시작

아무 저장소에서:

```sh
npx tessera-codegraph blast <symbol>
```

설정 단계가 없습니다. 첫 쿼리가 `.tessera/`에 인덱스를 만들고, 이후 쿼리는 바뀐 부분만 갱신합니다. 이제 에이전트에 연결하세요:

```sh
npx tessera-codegraph init
```

`init`은 사용 중인 에이전트를 찾아 설정 변경의 diff를 보여주고, 쓰기 전에 확인을 받습니다. Claude Code에서는 플러그인으로도 설치할 수 있습니다:

```text
/plugin marketplace add iamsaquib8/tessera
/plugin install tessera@tessera
```

## 명령어

| 명령어 | 답하는 질문 |
| --- | --- |
| `tessera blast <symbol>` / `--diff <rev>` | 이 변경이 무엇에 영향을 주는가? 영향도 순 호출자와 실행할 테스트 |
| `tessera context <symbol>` | 편집 전에 무엇을 읽어야 하는가? |
| `tessera verify --changed` | 호출 지점, 인자 개수, import를 깨뜨렸는가? 그렇다면 종료 코드 1 |
| `tessera find` / `refs` / `search` | 어디에 정의됐나? 누가 호출하나? 어떤 심볼이 일치하나? |
| `tessera path <from> <to>` | A가 B에 닿는가, 어떻게? 서비스 간 라우트와 gRPC도 따라감 |
| `tessera map <route\|rpc\|topic\|table\|service>` | 어떤 서비스가 이 엔드포인트, 메서드, 토픽, 테이블을 쓰는가? |
| `tessera ui` | 변경 영향과 서비스 간 계약 지도를 브라우저에서 보기 |
| `tessera init` | 이 프로젝트가 쓰는 에이전트에 Tessera 연결 |

모든 쿼리는 MCP 도구(`tessera mcp`)로도 제공되며, 모든 명령어는 `--json`을 지원합니다.

## 지원 에이전트와 언어

Tessera는 모델을 호출하지 않으므로 Claude, GPT, Gemini, Grok, DeepSeek, Qwen, 로컬 모델 모두에서 동작합니다. `tessera init`은 21개 에이전트를 지원하며, 그중 Claude Code, Codex CLI, Gemini CLI, GitHub Copilot CLI, Cursor, Qwen Code, Augment, Antigravity는 편집할 때마다 자동으로 검사합니다. 다른 에이전트는 `tessera init --git-hook`으로 커밋 전 검사를 설치할 수 있습니다. 전체 목록은 [영문 README](README.md#works-with-every-agent-and-every-model)를 참고하세요.

언어: TypeScript, TSX/React, JavaScript, Java, Python, Go, Rust, C, C++, C#, Ruby, PHP. TypeScript, JavaScript, Java는 심층 단계로, 공개 저장소에서 높은 신뢰도 결과가 컴파일러 정답과 **99.7%** 일치합니다([정확도](docs/accuracy.md)).

## 문서

- [첫 5분](docs/first-five-minutes.md)
- [편집 안전 훅](docs/hooks.md)
- [계약 지도](docs/contracts.md)
- [에이전트별 설정](docs/integrations.md)
- [1.x 안정성 약속](docs/stability.md)
- [로드맵](ROADMAP.md)

## 라이선스

Apache-2.0. [LICENSE](LICENSE)를 참고하세요.
