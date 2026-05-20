# docx-smart-format

> LLM-driven Word document formatter — let the model decide the style; let a local engine write the XML.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-cross--platform-brightgreen)](#平台与依赖)
[![Skill: Claude Code / Codex](https://img.shields.io/badge/skill-Claude%20Code%20%2F%20Codex-8A2BE2)](SKILL.md)

中文为主、英文同步的 Skill。把"用 Word 写论文/报告/纪要时的排版判断"交给 LLM，把"操作 OOXML 的细节"交给本地引擎，避免 LLM 直接拼 XML。

## 特性

- **单一引擎**：Rust 单文件二进制 ~520 KB，零运行时依赖，跨平台（无需 Word/WPS/Office/.NET）。
- **单一契约**：LLM 只输出一种 `FormatPlan`。块有 `ref` 即重排现有文档、无 `ref` 即从零生成，两种意图同形。
- **LLM 只出决策、不出 XML**：输出 `FormatPlan` JSON，引擎降级为内部表示后写回。
- **覆盖排版痛点**：分节、奇偶页眉、`PAGE` 字段、参考文献条目书签 + 上标 REF 域交叉引用、横向页竖排页码（`wps:wsp` + VML 双轨）、OMML 公式、毕业论文一键预设、模板化结构重排。
- **本地、离线**：skill 不联网，所有规则在 `references/`。

## 平台与依赖

- **跨平台**：`engine-rs/` 用 Rust 写,默认产物 Windows x64,可按需 `cargo build --release` 出 Linux / macOS / 任意 RID。
- 引擎单文件二进制 (~520 KB)，**不依赖** .NET / Office / WPS / 系统库。
- 编译需要 Rust 1.85+ (`cargo`)。

## 安装

### 方式 A：下载发布包（推荐）

测试包 `docx-smart-engine-rs-alpha-win-x64.zip`（约 310 KB）含 exe + 8 个样例 + OMML 速查表,直接解压到任意目录即可使用。

### 方式 B：克隆仓库 + 本地编译

```powershell
git clone https://github.com/yaya200325/docx-smart-format.git
cd docx-smart-format/engine-rs
cargo build --release
# 产物: target/release/docx-auto-template-engine.exe (Windows)
#       target/release/docx-auto-template-engine     (Linux/macOS)
```

## 快速开始

两条命令：`analyze`（仅重排现有文档时需要）和 `build`（写回）。

### 重排现有 `.docx`

```powershell
# 1. 分析源文档（每个 body 块得到稳定 path）
.\docx-auto-template-engine.exe analyze --input input.docx --output analysis.json

# 2. LLM 据 analysis.json 生成 format-plan.json（块用 ref 指向源块 path，结构见 references/format-plan-schema.md）
# 3. 写回
.\docx-auto-template-engine.exe build `
    --plan format-plan.json `
    --source input.docx `
    --output result.docx `
    --template template.docx
```

`--template <docx>` 可选：从模板拷贝 `word/styles.xml`,FormatPlan 块用 `format.styleId` 引用模板样式。

### 从零生成

```powershell
.\docx-auto-template-engine.exe build `
    --plan format-plan.json `
    --output result.docx `
    --normalize-references
```

`--normalize-references` 触发参考文献书签 + REF 字段交叉引用规范化，传 `false`/`off`/`0` 关闭。详见 [`references/reference-normalization.md`](references/reference-normalization.md)。

### 毕业论文一键预设

```powershell
# FormatPlan 里 document.preset = "undergraduate-thesis"
# (thesisUniversity / thesisTitle 控制奇偶页眉文字)
.\docx-auto-template-engine.exe build `
    --plan thesis.json `
    --output thesis.docx
```

预设自动启用 normalize-references、奇偶页眉、20pt 固定行距、标题级别字号、首行缩进 2 字符等毕业论文规范。

## 目录结构

```
docx-smart-format/
├── SKILL.md                       # Skill 主文档（LLM 入口）
├── agents/openai.yaml             # Codex/OpenAI agent 元数据
├── references/                    # 排版规则、能力清单、深度规范
│   ├── format-plan-schema.md      # LLM 唯一契约
│   ├── template-catalog.md        # 字体/字号默认值
│   ├── local-capability-map.md
│   ├── header-footer-odd-even.md
│   ├── reference-normalization.md
│   └── landscape-vertical-pagenum.md
├── scripts/
│   └── sample-format-plan.json    # FormatPlan 样例
├── engine-rs/                     # Rust 引擎源码 (Cargo)
│   ├── README.md                  # 引擎设计 + 命令面 + 踩坑归档
│   ├── Cargo.toml
│   └── src/
└── dist/                          # 测试包源（README、样例、OMML 速查表）
    ├── README.txt
    └── samples/
```

## 文档导航

- LLM 怎么用：从 [`SKILL.md`](SKILL.md) 开始。
- 字段定义：[`references/format-plan-schema.md`](references/format-plan-schema.md)。
- 字体字号默认值：[`references/template-catalog.md`](references/template-catalog.md)。
- 奇偶页眉与页码：[`references/header-footer-odd-even.md`](references/header-footer-odd-even.md)。
- 参考文献交叉引用：[`references/reference-normalization.md`](references/reference-normalization.md)。
- 横向页竖排页码：[`references/landscape-vertical-pagenum.md`](references/landscape-vertical-pagenum.md)。
- 引擎可调用能力清单：[`references/local-capability-map.md`](references/local-capability-map.md)。
- 引擎实现细节、踩坑归档：[`engine-rs/README.md`](engine-rs/README.md)。
- OMML 公式写法速查：[`dist/samples/OMML-CHEATSHEET.md`](dist/samples/OMML-CHEATSHEET.md)。

## 贡献

欢迎 Issue 与 PR：

- 贡献规则 → [CONTRIBUTING.md](CONTRIBUTING.md)
- 行为准则 → [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- 安全问题 → [SECURITY.md](SECURITY.md)（请不要在公开 Issue 中提交安全漏洞）
- 变更历史 → [CHANGELOG.md](CHANGELOG.md)

## 许可证

Apache License 2.0。详见 [`LICENSE`](LICENSE) 与 [`NOTICE`](NOTICE)。
