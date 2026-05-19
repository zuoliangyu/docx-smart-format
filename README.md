# docx-smart-format

> LLM-driven Word document formatter — let the model decide the style; let a local engine write the XML.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20x64-lightgrey)](#平台与依赖)
[![Skill: Claude Code / Codex](https://img.shields.io/badge/skill-Claude%20Code%20%2F%20Codex-8A2BE2)](SKILL.md)
[![Release](https://img.shields.io/github/v/release/yaya200325/docx-smart-format)](https://github.com/yaya200325/docx-smart-format/releases)

中文为主、英文同步的 Skill。把"用 Word 写论文/报告/纪要时的排版判断"交给 LLM，把"操作 OOXML 的细节"交给本地引擎，避免 LLM 直接拼 XML。

## 特性

- **单一契约**：LLM 只输出一种 `FormatPlan`。块有 `ref` 即重排现有文档、无 `ref` 即从零生成，两种意图同形。
- **LLM 只出决策、不出 XML**：输出 `FormatPlan` JSON，引擎降级为内部表示后写回。
- **覆盖排版痛点**：分节、奇偶页眉、STYLEREF 字段、`pgNumType` + `PAGE` 字段、参考文献条目书签 + 上标 REF 交叉引用、横向页竖排页码（`wps:wsp` + VML 双轨）。
- **本地、离线**：skill 不联网，所有模板规则在 `references/`。
- **可校验**：`scripts/validate_decision.py` 校验旧版 decision 结构（FormatPlan 校验器待后续提供）。

## 平台与依赖

- **当前仅支持 Windows x64**。`engine/runtime/docx-auto-template-engine.exe` 是 .NET self-contained 部署。macOS / Linux 暂未发布预编译产物；可按 `CONTRIBUTING.md` 自行构建对应 RID。
- Skill 自身运行不需要 .NET SDK，只要 runtime 可执行文件存在即可。
- 重新构建引擎需要 .NET SDK 8.0+。

## 安装

### 方式 A：下载 Release（推荐）

1. 到 [Releases](https://github.com/yaya200325/docx-smart-format/releases) 下载最新版本的 `docx-smart-format-<version>-win-x64.zip`。
2. 解压到本机 skill 目录：
   - Claude Code：`~/.claude/skills/docx-smart-format/`
   - Codex：`~/.codex/skills/docx-smart-format/`
   - （Windows 实际路径为 `C:\Users\<USER>\.claude\skills\...` 或 `C:\Users\<USER>\.codex\skills\...`）
3. 解压后目录里应当包含 `engine/runtime/docx-auto-template-engine.exe`。

### 方式 B：克隆仓库 + 本地构建

适用于 Linux/macOS 用户或希望自行编译引擎的开发者：

```powershell
git clone https://github.com/yaya200325/docx-smart-format.git
cd docx-smart-format
# Windows
.\build.ps1
# Linux / macOS
./build.sh
```

`build.ps1` / `build.sh` 会调用 `dotnet publish` 把 `engine/src/` 编译并发布到 `engine/runtime/`。脚本默认 RID 为 `win-x64`，可用 `-Rid linux-x64` / `--rid osx-arm64` 等改写。

> ⚠️ Linux / macOS RID 当前未在 CI 自动验证，引擎对非 Windows 平台的兼容性尚未官方背书；如遇问题欢迎开 Issue。

## 快速开始

两条命令：`analyze`（仅重排现有文档时需要）和 `build`（写回）。`build` 有 `--source` 即重排、无 `--source` 即从零生成。

### 重排现有 `.docx`

```powershell
$skill = "$env:USERPROFILE\.claude\skills\docx-smart-format"

# 1. 分析源文档（每个 body 块得到稳定 path）
& "$skill\engine\runtime\docx-auto-template-engine.exe" `
    analyze --input .\input.docx --output .\analysis.json

# 2. LLM 据 analysis.json 生成 format-plan.json（块用 ref 指向源块 path，结构见 references/format-plan-schema.md）
# 3. 写回
& "$skill\engine\runtime\docx-auto-template-engine.exe" `
    build --plan   .\format-plan.json `
          --source .\input.docx `
          --output .\result.docx `
          --template .\template.docx
```

### 从零生成

```powershell
$skill = "$env:USERPROFILE\.claude\skills\docx-smart-format"

# FormatPlan 块不带 ref，自带 text/image/table/equation
& "$skill\engine\runtime\docx-auto-template-engine.exe" `
    build --plan   .\format-plan.json `
          --output .\result.docx `
          --normalize-references
```

`--normalize-references` 触发参考文献书签 + REF 字段交叉引用规范化，传 `false`/`off`/`0` 关闭。详见 [`references/reference-normalization.md`](references/reference-normalization.md)。

### 校验

`scripts/validate_decision.py` 仅校验旧版 decision 结构（FormatPlan 校验器待后续提供）。当前以 [`scripts/sample-format-plan.json`](scripts/sample-format-plan.json) 为结构参照。

## 目录结构

```
docx-smart-format/
├── SKILL.md                       # Skill 主文档（LLM 入口）
├── agents/openai.yaml             # Codex/OpenAI agent 元数据
├── references/                    # 模板规则、能力清单、深度规范
│   ├── format-plan-schema.md      # LLM 唯一契约
│   ├── template-catalog.md        # 字体/字号唯一权威
│   ├── local-capability-map.md
│   ├── header-footer-odd-even.md
│   ├── reference-normalization.md
│   └── landscape-vertical-pagenum.md
├── scripts/
│   ├── sample-format-plan.json    # FormatPlan 样例
│   ├── sample-render-spec.json    # 旧样例（R4 前保留）
│   ├── sample-chem-render-spec.json
│   ├── sample-decision.json
│   └── validate_decision.py
└── engine/
    ├── runtime/                   # 预编译引擎（gitignore，发布到 Release）
    └── src/                       # 引擎源代码（C# / .NET 8）
```

## 文档导航

- LLM 怎么用：从 [`SKILL.md`](SKILL.md) 开始。
- 字段定义：[`references/format-plan-schema.md`](references/format-plan-schema.md)。
- 字体字号默认值：[`references/template-catalog.md`](references/template-catalog.md)。
- 奇偶页眉与页码：[`references/header-footer-odd-even.md`](references/header-footer-odd-even.md)。
- 参考文献交叉引用：[`references/reference-normalization.md`](references/reference-normalization.md)。
- 横向页竖排页码：[`references/landscape-vertical-pagenum.md`](references/landscape-vertical-pagenum.md)。
- 引擎可调用能力清单：[`references/local-capability-map.md`](references/local-capability-map.md)。

## 贡献

欢迎 Issue 与 PR：

- 贡献规则 → [CONTRIBUTING.md](CONTRIBUTING.md)
- 行为准则 → [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- 安全问题 → [SECURITY.md](SECURITY.md)（请不要在公开 Issue 中提交安全漏洞）
- 变更历史 → [CHANGELOG.md](CHANGELOG.md)

## 许可证

Apache License 2.0。详见 [`LICENSE`](LICENSE) 与 [`NOTICE`](NOTICE)。
