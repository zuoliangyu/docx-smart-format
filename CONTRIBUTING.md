# 贡献指南

感谢你愿意为 docx-smart-format 贡献代码、文档或想法。这里集中放对贡献者最有用的信息。请先粗读完再开始动手。

## 仓库结构速记

- `SKILL.md`：LLM 入口。规则修改优先在这里及 `references/` 中体现。
- `references/`：规则与能力清单。**修改这些会直接改变 LLM 行为**，请同步考虑是否影响 `scripts/sample-*.json`。
- `references/format-plan-schema.md`：LLM 唯一契约 FormatPlan 的字段定义。
- `scripts/validate_decision.py`：旧版 decision 结构校验器（FormatPlan 校验器待补）。
- `tests/golden/`：行为回归网（架构重写安全网）。改引擎后必须 `run.ps1 -Mode verify`。
- `agents/openai.yaml`：Codex/OpenAI agent 元数据。
- `engine/src/`：C# 引擎源代码（.NET 8）。
- `engine/runtime/`：本地 `dotnet publish` 出来的产物，**不要提交到 git**（已在 `.gitignore`）。

## 本地环境

| 工具         | 用途                                | 推荐版本    |
|--------------|-------------------------------------|-------------|
| .NET SDK     | 构建引擎                            | 8.0 或更高  |
| Python       | 跑 `validate_decision.py`           | 3.10 或更高 |
| PowerShell   | Windows 上运行 `build.ps1` 与脚本   | 5.1+ / 7    |
| Bash         | Linux/macOS 上运行 `build.sh`       | 4+          |
| Git          | 版本控制                            | 任意近期版本|

## 构建引擎

> Skill 的日常使用者**不需要**做这一步，只有要改 C# 代码、要发布新 RID、或者要在非 Windows 平台上跑引擎时才需要。

```powershell
# Windows
.\build.ps1                       # 默认 RID = win-x64
.\build.ps1 -Rid linux-x64        # 指定 RID

# Linux / macOS
./build.sh                        # 默认 RID = linux-x64
./build.sh --rid osx-arm64        # 指定 RID
```

脚本会调用：

```bash
dotnet publish engine/src/docx-auto-template-engine.csproj \
  -c Release \
  -r <RID> \
  --self-contained true \
  /p:PublishSingleFile=false \
  -o engine/runtime
```

发布到 `engine/runtime/` 后，`SKILL.md` 中描述的 `analyze` / `apply` / `render` 命令即可使用。

## 跑测试与校验

- 校验决策 JSON：

  ```powershell
  python scripts\validate_decision.py scripts\sample-decision.json
  ```

  应输出 `[OK] decision.json 结构合法`。CI 会在每次 PR 上跑这一步。

- 构建检查：

  ```powershell
  dotnet build engine/src/docx-auto-template-engine.csproj -c Release
  ```

  这是 CI 在 PR 上的最低门槛之一。

- 端到端 smoke：手工跑一遍 `scripts/sample-render-spec.json`，确认生成的 `.docx` 能用 Word 正常打开。

## 提交规范

### 分支与 PR

- 从 `main` 切出工作分支，命名建议 `feat/<topic>` / `fix/<topic>` / `docs/<topic>` / `chore/<topic>`。
- PR 描述请说明：**做了什么**、**为什么这么做**、**怎么验证**。
- 影响 LLM 行为（`SKILL.md` / `references/`）的改动，请说明对模板与决策 JSON 的兼容性影响；必要时一并更新 `scripts/sample-*.json` 与 `CHANGELOG.md` 的 `[Unreleased]` 节。

### Commit message

参考 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/v1.0.0/) 风格：

```
<type>(<scope>): <subject>

<body>

<footer>
```

`type` 推荐使用：

- `feat`：新功能（能力、规则、字段）。
- `fix`：bug 修复（错排、错误结构、错误校验）。
- `docs`：仅文档。
- `refactor`：重构（不改外部行为）。
- `test`：增删测试。
- `chore`：构建脚本、CI、依赖等。
- `revert`：回滚。

`scope` 建议：`skill` / `references` / `engine` / `scripts` / `ci` / `release`。

### 代码风格

- C#：跟随 `engine/src/` 现有风格；优先简洁清晰，不要为未来需求添加预留开关。
- Python：`scripts/` 下兼容 3.10，类型注解优先。
- Markdown：中文为主、英文术语保留原文；标题层级与现有 `references/*.md` 一致。

## Issue 规范

模板见 `.github/ISSUE_TEMPLATE/`。提交前请确认：

- 已搜索过相似 issue。
- 列出能复现的最小步骤；附 decision JSON / RenderSpec 时务必脱敏。
- 若涉及生成 `.docx` 行为异常，请说明用 Word / WPS / LibreOffice 哪个打开、版本号。

安全相关 issue 走 [SECURITY.md](SECURITY.md) 私下渠道，**不要公开提**。

## 行为准则

参与者须遵守 [行为准则](CODE_OF_CONDUCT.md)。

## 许可证

提交 PR 即视为同意你的贡献按本项目根 [LICENSE](LICENSE) Apache 2.0 授权。
