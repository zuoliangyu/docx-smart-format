---
name: docx-smart-format
description: 读取 `.docx` 或从零生成 `.docx`：LLM 判断文档类型、结构与行内语义，输出单一 FormatPlan 决策，本地引擎执行写回。用于自动整理论文/报告/纪要排版、统一字体字号缩进行距分页、处理图片表格公式、配置页眉页脚页码。
---

# DOCX Smart Format

## 平台与运行依赖

- **跨平台**:Rust 单文件 `docx-auto-template-engine.exe`(~520 KB),零运行时依赖,**不需要 Word / WPS / Office / .NET**,不联网。
- 默认产物 Windows x64;Linux / macOS 可在 `engine-rs/` 下 `cargo build --release` 自行编译。

## 核心模型

LLM 输出**唯一一种契约**：`FormatPlan`。引擎把它降级为内部表示并写回 docx。LLM **不直接拼 Word XML**。

一个 `FormatPlan` 描述整篇文档；`blocks[]` 里每个块有两种意图，**靠有没有 `ref` 区分**：

- **覆盖（重排现有文档）**：块有 `ref`（指向源文档某个块的 path），只写要改的 `format`，原文内容由引擎从源文档取回。
- **生成（从零造）**：块没有 `ref`，自带 `text` / `image` / `table` / `equation`。

两种意图同形、可混用。LLM 永远只学这一套 schema。

## 两条命令

引擎位于 Skill 包内 `<skill-dir>/engine/docx-auto-template-engine[.exe]`,LLM 调用时用完整路径。

```powershell
# 1. 重排现有文档时，先分析源 docx（从零生成可跳过）
<skill-dir>/engine/docx-auto-template-engine.exe analyze --input <input.docx> --output <analysis.json>

# 2. 写回：有 --source 即重排，无 --source 即从零生成
<skill-dir>/engine/docx-auto-template-engine.exe build --plan <format-plan.json> --output <result.docx> `
    [--source <input.docx>] [--template <template.docx>] [--normalize-references]
```

其中 `<skill-dir>` 通常是 `~/.claude/skills/docx-smart-format/`(Windows: `C:\Users\<USER>\.claude\skills\docx-smart-format\`) 或 `~/.codex/skills/docx-smart-format/`。

`analyze` 输出里每个 body 块有稳定 `path`（如 `/body/paragraph[2]`），就是 `FormatPlan` 块里 `ref` 要填的值。

**额外能力**：
- 毕业论文一键预设:`document.preset: "undergraduate-thesis"` + `thesisUniversity` / `thesisTitle`,引擎自动注入字体字号缩进 + 奇偶页眉 + 页脚页码 + 自动开 normalize-references。
- 模板化结构重排:`build --template <docx>` 把模板的 `word/styles.xml` 拷进输出,FormatPlan 块用 `format.styleId` 引用模板里定义的样式。

## 模式判定（机械规则）

```
if 用户提供了源 .docx:
    analyze 源文档 → LLM 用 ref 块写覆盖型 FormatPlan → build --source
elif 用户提供了主题/提纲/章节结构 + 内容:
    LLM 写生成型 FormatPlan（块无 ref）→ build
else:
    向用户澄清缺哪类输入，不要猜
```

## 生成前用户确认（必须执行）

开始前**主动询问字体偏好**：

> 生成前确认一下字体设置，推荐默认：中文宋体、英文 Times New Roman、正文小四。您有特别要求吗？

默认值见 [references/template-catalog.md](references/template-catalog.md) 的“通用硬规则”。满足任一可跳过：用户已明确字体/字号；用户说“用默认就行”；重排时用户要求“保持原样”（则不覆盖原字体）。

确认后写入 `document.cnFont` / `document.enFont` / `document.baseFontPt`，全篇统一。

## LLM 要做的判断

1. **文档类型**：论文 / 实验报告 / 公司汇报 / 会议纪要 / 通用正式文档 → 写入 `docType`（仅语义提示）。
2. **块级结构与角色**：每个块的 `role`（`title` / `heading1..3` / `body` / `reference` / `caption` / `figure` / `table` / `equation` / `pageNumber`）。
3. **页面与分节**：是否分节、节间页码格式是否不同、首页是否单独、是否奇偶页不同、章节是否新页起 → `sections[]` + 块的 `sectionKey`。
4. **行内语义**：哪些加粗 / 斜体 / 上下标 / 需要 Tab。粗体斜体**必须先判断语义**，不要原样保留。
5. **对象关系**：图注属于哪张图、表注属于哪个表、公式是否编号且被正文引用。

## 何时调用何种能力（判据）

- **分节**：封面/摘要/目录/正文/附录之间存在页码/页眉页脚/方向/边距差异时分节；不要只因视觉换页就分节。
- **字体字号**：中英文必须分设（`document.cnFont` / `document.enFont`，或块 `format.cnFont` / `format.enFont`）。层级字号见 template-catalog。
- **加粗 / 斜体**：标题、强调词、术语、表头、关键结论加粗；英文术语、变量名、学术记号斜体。
- **上下标**：`H2O` / `x^2` / `CO2` / `m/s^2` 按语义；正文强调数字、编号、页码**不要**误判；不确定回退基线。引擎侧自动处理 `^_` 标记与化学式/单位指数，LLM 只需保证文本里写对。
- **缩进**：正文用首行缩进（`format.firstLineIndent`）；参考文献/编号列表用悬挂缩进（`format.hangingIndent`）；标题/图注/表注/独立公式通常不缩进。**不要用 Tab 代替首行缩进**。
- **行距段距**：用 `document.lineSpacing` 或块 `format.lineSpacing` / `beforeSpacing` / `afterSpacing`；**不要用空行模拟段距**。
- **分页**：一级标题前换页用 `format.pageBreakBefore`；**不用空段落顶页**。
- **页眉页脚页码**：`document.headerFooter`（`oddEven` / `firstDifferent` / `pageNumber`）；页码用字段不用纯数字。深度规则见 [references/header-footer-odd-even.md](references/header-footer-odd-even.md)。
- **图片**：`role: figure` + `image`（普通插图版心 60%–80%，宽图 85%–100%；默认居中，图注默认图下）。公式截图按图片处理，不伪装成原生公式。
- **表格**：`role: table` + `table.rows`（默认三线表、表头加粗、表注默认表上）。
- **公式**：`role: equation` + `equation`（`text` 或 `xml`，`displayMode`）。**是否编号由 LLM 按文档类型/引用关系判断**，不是所有公式都编号。
- **参考文献交叉引用**：`build --normalize-references` 时引擎做书签 + REF 字段交叉引用 + `[n]<TAB>` 规范化。LLM 只决定“哪些 `[n]` 是上标”。规则见 [references/reference-normalization.md](references/reference-normalization.md)。
- **横向页竖排页码**：含横向插页且页码需竖排装订边时使用，深度结构见 [references/landscape-vertical-pagenum.md](references/landscape-vertical-pagenum.md)。

## FormatPlan 结构速览

```jsonc
{
  "docType": "academic-thesis",
  "document": {
    "title": "...", "author": "...",
    "pageSize": "a4", "orientation": "portrait",
    "margins": { "top": "1440", "bottom": "1440", "left": "1800", "right": "1800" },
    "cnFont": "宋体", "enFont": "Times New Roman", "baseFontPt": 12, "lineSpacing": 1.5,
    "headerFooter": { "oddEven": false, "firstDifferent": true, "pageNumber": "center-page-number" }
  },
  "sections": [ { "key": "main", "pageStart": 1, "pageNumFmt": "decimal" } ],
  "blocks": [
    { "ref": "/body/paragraph[1]", "role": "heading1", "format": { "bold": true } },   // 覆盖
    { "role": "body", "sectionKey": "main", "text": "正文……", "format": { "firstLineIndent": "480" } }  // 生成
  ]
}
```

完整字段见 [references/format-plan-schema.md](references/format-plan-schema.md)。样例见 [scripts/sample-format-plan.json](scripts/sample-format-plan.json)。

## 禁止做法

- 不要让 LLM 直接输出 Word XML。
- 不要用空行模拟段距、不要用空段落顶页。
- 不要用纯文本数字代替页码字段。
- 不要用 Tab 代替首行缩进。
- 不要把所有粗体斜体原样保留——先判断语义。
- 不要把图片公式伪装成原生公式。
- 不要把所有独立公式一律自动编号。

## 失败回退（保守策略）

- 文档类型不确定 → `general-formal-doc`
- 块角色不确定 → `body`
- 行内样式不确定 → 保留原样（覆盖块只写确定要改的 `format`）
- 图注表注归属不确定 → 保留相邻顺序
- 是否分节不确定 → 不新建分节
- 是否编号不确定 → 不编号，除非模板明确强制

## 资源索引

- [references/format-plan-schema.md](references/format-plan-schema.md) — FormatPlan 字段定义与能力映射
- [references/template-catalog.md](references/template-catalog.md) — 内置模板与默认字体字号（**字体默认值唯一权威**）
- [references/header-footer-odd-even.md](references/header-footer-odd-even.md) — 奇偶页眉 / STYLEREF / pgNumType / 页码
- [references/reference-normalization.md](references/reference-normalization.md) — 参考文献书签 + REF 字段交叉引用
- [references/landscape-vertical-pagenum.md](references/landscape-vertical-pagenum.md) — 横向页左侧竖排页码
- [scripts/sample-format-plan.json](scripts/sample-format-plan.json) — FormatPlan 样例
