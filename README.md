# docx-smart-format

> LLM-driven Word document formatter — let the model decide the style; let a local engine write the XML.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-cross--platform-brightgreen)](#平台与依赖)
[![Skill: Claude Code / Codex](https://img.shields.io/badge/skill-Claude%20Code%20%2F%20Codex-8A2BE2)](SKILL.md)

中文为主、英文同步的 **Skill**。把"用 Word 写论文/报告/纪要时的排版判断"交给 LLM,把"操作 OOXML 的细节"交给本地引擎,避免 LLM 直接拼 XML。

---

## ⚠️ 使用前必读 — 别 `git clone`,去下载 Release

这是个 **Skill**(给 Claude Code / Codex 用的能力包)。**用户安装的正确姿势是去 [Releases](https://github.com/zuoliangyu/docx-smart-format/releases/latest) 下载对应平台的 zip**,不是 `git clone`。

| 你是谁 | 你要做的 |
|---|---|
| **Skill 用户**(用 LLM 排版 docx) | ↘ 下载 Release → 解压 → 拖进 `~/.claude/skills/` |
| **Skill 开发者**(改规则/文档) | `git clone` + 编辑 `SKILL.md` / `references/` |
| **引擎开发者**(改 Rust 编译产物) | `git clone` + `cd engine-rs && cargo build --release` |

---

## 安装(给 Skill 用户)

1. 到 **[Releases](https://github.com/zuoliangyu/docx-smart-format/releases/latest)** 下载对应你系统的 zip:

   | 平台 | 文件 |
   |---|---|
   | Windows | `docx-smart-format-<version>-win-x64.zip` |
   | Linux | `docx-smart-format-<version>-linux-x64.zip` |
   | macOS (Apple Silicon) | `docx-smart-format-<version>-osx-arm64.zip` |

2. 解压。会得到一个叫 `docx-smart-format/` 的文件夹。

3. 整个文件夹拖进你的 Skill 目录:
   - **Claude Code**:`~/.claude/skills/`(Windows:`C:\Users\<USER>\.claude\skills\`)
   - **Codex**:`~/.codex/skills/`

4. 完成。下次跟 Claude Code / Codex 对话时让它"用 docx-smart-format 给我整理论文",它会自动调用。

**zip 里有什么、没什么**:
```
docx-smart-format/
├── SKILL.md                              ← LLM 入口
├── references/                           ← 排版规则文档
├── agents/openai.yaml                    ← Codex 元数据
├── scripts/sample-format-plan.json       ← LLM 模板
├── examples/                             ← 离线测试样例
│   ├── 01-basic.json ... 09-...          ← 9 个场景
│   ├── template.docx, pixel.png
│   └── OMML-CHEATSHEET.md
└── engine/
    └── docx-auto-template-engine[.exe]   ← 引擎(编译产物)
```

**注意**:zip 里**没有 Rust 源码**。源码在仓库的 `engine-rs/` 下,只给开发者构建用。用户接触到的全是即用文件 + 一个编译好的 binary。

---

## 特性

- **单一引擎**:Rust 单文件二进制 ~520 KB,零运行时依赖,跨平台(无需 Word/WPS/Office/.NET)。
- **单一契约**:LLM 只输出一种 `FormatPlan`。块有 `ref` 即重排现有文档、无 `ref` 即从零生成,两种意图同形。
- **LLM 只出决策、不出 XML**:输出 `FormatPlan` JSON,引擎降级为内部表示后写回。
- **覆盖排版痛点**:分节、奇偶页眉、`PAGE` 字段、参考文献条目书签 + 上标 REF 域交叉引用、横向页竖排页码(`wps:wsp` + VML 双轨)、OMML 公式、毕业论文一键预设、模板化结构重排。
- **本地、离线**:Skill 不联网,所有规则在 `references/`。

---

## 直接命令行使用(不走 LLM)

Skill 装好后,引擎本身也是个独立命令行工具,可以脱离 LLM 直接跑:

```powershell
$skill = "$env:USERPROFILE\.claude\skills\docx-smart-format"
$engine = "$skill\engine\docx-auto-template-engine.exe"

# 从零生成
& $engine build --plan plan.json --output result.docx

# 重排现有 docx
& $engine analyze --input input.docx --output analysis.json
& $engine build --plan plan.json --source input.docx --output result.docx

# 毕业论文一键预设
& $engine build --plan examples\07-thesis-preset.json --output thesis.docx
```

Linux/macOS 把 `.exe` 去掉,路径分隔符换 `/` 即可。

`FormatPlan` 结构定义见 [`references/format-plan-schema.md`](references/format-plan-schema.md)。

---

## 给 Skill 开发者

如果你想改 LLM 看的规则(`SKILL.md` / `references/*.md`)或样例(`examples/`):

```bash
git clone https://github.com/zuoliangyu/docx-smart-format.git
cd docx-smart-format
# 改完文件,本地跑 examples 验证
$skill_engine = "C:\Users\<USER>\.claude\skills\docx-smart-format\engine\docx-auto-template-engine.exe"
& $skill_engine build --plan examples\01-basic.json --output out.docx
```

提 PR 即可。规则/文档变更**不需要装 Rust 工具链**。

## 给引擎开发者

如果你要动 Rust 引擎(OOXML 写回、analyze 解析、autoformat 等):

```bash
git clone https://github.com/zuoliangyu/docx-smart-format.git
cd docx-smart-format/engine-rs
cargo build --release
# 产物: target/release/docx-auto-template-engine[.exe]
```

详见 [`engine-rs/README.md`](engine-rs/README.md)(架构、能力清单、踩坑归档)。
需要 Rust 1.85+。

---

## 平台与依赖

- **跨平台**:`docx-auto-template-engine` 是 Rust 写的,默认 release 产 Windows x64,可按需 `cargo build --release` 出 Linux / macOS / 任意 RID。
- 引擎单文件二进制 (~520 KB),**不依赖** .NET / Office / WPS / 系统库。
- 编译需要 Rust 1.85+ (`cargo`)。

---

## 目录结构(仓库视角)

仓库里你会看到这些,但**普通 Skill 用户用 Release zip 看不到**完整的 `engine-rs/` 源码:

```
docx-smart-format/             ← 仓库根
├── SKILL.md                   ← LLM 入口
├── README.md                  ← 本文件
├── references/                ← 排版规则
│   ├── format-plan-schema.md  ← FormatPlan 字段定义
│   ├── template-catalog.md
│   ├── header-footer-odd-even.md
│   ├── reference-normalization.md
│   ├── landscape-vertical-pagenum.md
│   └── local-capability-map.md
├── agents/openai.yaml         ← Codex 元数据
├── scripts/
│   └── sample-format-plan.json   ← LLM 起手样例
├── examples/                  ← 离线场景样例(进 Release)
│   ├── 01-basic.json ... 09-firstline-scale-test.json
│   ├── template.docx, pixel.png
│   ├── OMML-CHEATSHEET.md
│   └── USAGE.md
├── engine-rs/                 ← Rust 源码(只给开发者,不进 Release)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
└── .github/workflows/
    ├── ci.yml
    └── release.yml             ← 自动打包 Release zip
```

## 文档导航

- LLM 怎么用:从 [`SKILL.md`](SKILL.md) 开始。
- 字段定义:[`references/format-plan-schema.md`](references/format-plan-schema.md)。
- 字体字号默认值:[`references/template-catalog.md`](references/template-catalog.md)。
- 奇偶页眉与页码:[`references/header-footer-odd-even.md`](references/header-footer-odd-even.md)。
- 参考文献交叉引用:[`references/reference-normalization.md`](references/reference-normalization.md)。
- 横向页竖排页码:[`references/landscape-vertical-pagenum.md`](references/landscape-vertical-pagenum.md)。
- 引擎可调用能力清单:[`references/local-capability-map.md`](references/local-capability-map.md)。
- 引擎实现细节、踩坑归档:[`engine-rs/README.md`](engine-rs/README.md)。
- OMML 公式写法速查:[`examples/OMML-CHEATSHEET.md`](examples/OMML-CHEATSHEET.md)。
- 离线样例怎么跑:[`examples/USAGE.md`](examples/USAGE.md)。

## 贡献

- 贡献规则 → [CONTRIBUTING.md](CONTRIBUTING.md)
- 行为准则 → [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- 安全问题 → [SECURITY.md](SECURITY.md)
- 变更历史 → [CHANGELOG.md](CHANGELOG.md)

## 致谢 / Acknowledgements

本项目最初由 **[@yaya200325](https://github.com/yaya200325)** 创立并贡献了 v0.1.0
版本(基于 .NET 8 + `DocumentFormat.OpenXml` 的引擎、初版 SKILL.md 与
references 文档体系、Release 流水线)。

本 fork 在其基础上做了:

- 架构重写为单一 `FormatPlan` 契约 + `analyze` + `build` 双命令面
- **引擎从 .NET 重写为 Rust**(单文件 ~520 KB、零运行时依赖、跨平台)
- 一系列真实 Word 渲染的实测修复(OMML / REF 字段 / 段落样式 / 字符缩进 / 大纲级 等)

原仓库:[github.com/yaya200325/docx-smart-format](https://github.com/yaya200325/docx-smart-format)。

## 许可证

Apache License 2.0。详见 [`LICENSE`](LICENSE) 与 [`NOTICE`](NOTICE)。
