# Changelog

本文件记录 docx-smart-format 的所有重要变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 1.1.0；版本号采用 [语义化版本](https://semver.org/lang/zh-CN/) 2.0.0。

## [Unreleased]

### Added
- 统一 LLM 契约 `FormatPlan`（覆盖/生成同形，靠块 `ref` 区分）与 `FormatPlanCompiler`。
- `build` 命令：有 `--source` 即重排、无即从零生成。
- `references/format-plan-schema.md`；`scripts/sample-format-plan.json`。
- `tests/golden/` 行为回归网（record/verify，规范化 + 易变量清洗）。
- **Rust 引擎（alpha）`engine-rs/`**：单文件 ~0.4MB 二进制，零运行时依赖，跨平台。
  - RS0 段落（FormatPlan generate 全部段级保真：标题级、对齐、缩进、间距、字体、上下标、粗斜下划线、颜色等）
  - RS1 多分节 + sectPr（页面大小/方向/边距/pgNumType/titlePg/section type）
  - RS2 OPC 部件 + 关系机制 + 居中 PAGE 字段页脚
  - RS3 表格（三线表 + 表头加粗下框线 + caption 在表上方）
  - RS4 图片（DrawingML inline + 媒体部件 + rels + content-types Default 扩展）
  - RS5 `analyze`：有效样式继承解析（docDefaults → pStyle basedOn 链 → 直接 rPr）+ 块结构
  - RS6 `build --source` overlay：源块文本 + 标题级继承 + 段级 format 覆盖
  - RS7 overlay 内联保真：per-run 提取 + 重排时还原源文档每个 run 的粗斜/上下标/Tab
  - RS8 AutoFormat 分词器：化学式 H2O、单位指数 m²/cm³、`^X`/`_X`/`^{XX}`/`_{XX}` 标记自动拆 run
  - RS9 参考文献规范化（`--normalize-references`）：reference 块 `[n]<Tab>` + 书签；正文上标 `[n]` / `[n,m]` / `[n-m]` 转 REF 复杂字段交叉引用
  - RS10 OMML 公式：原样嵌入调用方的 `<m:oMath>`/`<m:oMathPara>`；display 模式自动外裹 `<m:oMathPara>` + `centerGroup`
  - RS11 VML 双轨竖排页码：`mc:AlternateContent` + `wps:wsp` + VML `v:rect` 双轨
  - **RS12 毕业论文预设**：`document.preset: "undergraduate-thesis"` 一键填充
    A4 + 中文宋体/英文 TNR + 12pt + 20pt exact 行距 + 标题字号(18/16/14pt 粗 +
    pageBreakBefore) + 正文首行缩进 2 字符 + 参考文献悬挂缩进 + 奇偶页眉
    (`document.thesisUniversity` / `thesisTitle` 控制文本) + 居中 PAGE 字段
    页脚 + 自动启用 `--normalize-references`。`PlanFormat` 新增 `lineSpacingRule`
    字段(auto/exact/atLeast)；docx.rs hf 部件机制泛化为多 HfPart + 自动
    emit `word/settings.xml` 含 `<w:evenAndOddHeaders/>`。完成后 Rust 引擎对
    "毕业论文重排"场景与 .NET 完全等价,**用户可不装 .NET**。
  - **RS13 模板化结构重排**:`build --template <docx>` 从 template 复制
    `word/styles.xml` 到输出 + 注册 styles 关系。`PlanFormat.styleId` 块用
    `<w:pStyle w:val="..."/>` 引用模板样式。完成后 .NET 全部独有能力均已迁移,
    .NET 引擎从仓库移除(见 Removed)。
- `engine-rs/README.md`：Rust 引擎说明、命令面、能力清单、踩坑归档。
- `dist/samples/OMML-CHEATSHEET.md`：OMML 公式写法速查表 + Cambria Math 字体提示硬性要求。
- 顶层 README 加"引擎实现：.NET（稳定） 与 Rust（alpha）"对照表。

### Changed
- 架构重写（分阶段 R0–R4，全程 golden 守护，逐字节零回归）：
  - `DocxRenderer` 1754 行单体拆为 7 个 feature partial。
  - CLI 收敛为 `analyze` + `build`；SKILL.md 257→~150 行（认知瘦身）。
  - 发布改单文件 + 压缩，自包含产物 ~60-70MB → 单个 38MB exe，仍零安装（不需 Office/WPS/.NET）。
- `decision-schema.md` 移除，由 `format-plan-schema.md` 取代。

### Removed
- **.NET 引擎完全移除**(`engine/src/` 全部 9 个 .cs 文件 + `build.ps1`/`build.sh` +
  旧 decision schema 样例 + `validate_decision.py` + `tests/golden/` 回归网)。
  Rust 引擎在 RS0–RS13 之间已完成 100% 能力 parity 并经真实 Word 渲染验证,
  .NET 路径不再需要保留。仓库从此**只有一个引擎**:Rust。
- 顶层 README 移除"双引擎对照"段、安装方式 .NET 部分;CONTRIBUTING 改为
  Rust-only,不再要求贡献者安装 .NET / Python。
- 用户层面:不再有 `apply` / `render` 命令(全部迁移到 `build` + FormatPlan);
  不再有 `--template-preset` 参数(改为 `document.preset` 字段)。

### Deferred / Known gaps
- Rust 引擎 `analyze` 鲁棒性:仅在引擎自产 docx 上验过 100% 字段比对;
  野生 Word/WPS 输入(含 latentStyles / style 别名 / 复杂 basedOn 链)未测。
- 逐部件页眉/页脚的任意定制(单节多 default 文件、首页不同的内容差异等):
  FormatPlan 暴露面有限;毕业论文预设的 4 部件是 hardcoded,通用情景待扩展。
- FormatPlan 专用校验器待补。

### Fixed
- Rust 引擎在真 Word 渲染中暴露的 4 处问题（已修复并通过实测）：
  - REF 字段数字未上标：`<w:fldSimple>` 在部分 Word 版本里丢失缓存 run 的 rPr。
    改用复杂字段（begin / separate / end），每个 run 各带 vertAlign superscript。
  - 表注 caption 在表下：与 SKILL 规定"表注默认放表上方"不符。调换 caption emit 顺序。
  - OMML 公式被当文本渲染：每个 `<m:r>` 必须带 `<w:rFonts w:ascii="Cambria Math"/>`
    字体提示；缺失则 Word 走文本回退。样例与速查表已统一带上。
  - F9 更新后参考文献变 `[[1]]`：原书签包了整个 `[1]`，更新时 REF 拉回整段加上外面的
    括号变成双括号。改为书签只包数字、方括号在书签外作为兄弟 run。

## [0.1.0] - 2026-05-19

首个公开发布版本。

### Added
- Skill 主文档 `SKILL.md`，覆盖两种工作模式（基于现有 docx 重排 / 从零生成）。
- 决策 JSON 四层结构（文档级 / 块级 / 行内 / 对象级），见 `references/decision-schema.md`。
- 内置模板目录：`academic-thesis` / `experiment-report` / `company-report` / `meeting-minutes` / `general-formal-doc`，规则集中在 `references/template-catalog.md`。
- 本地引擎 `docx-auto-template-engine`（.NET 8 self-contained，Windows x64），命令：`analyze` / `apply` / `render`。
- 高阶能力：
  - 奇偶页眉 + STYLEREF 字段（`\* MERGEFORMAT`，见 `references/header-footer-odd-even.md`）。
  - 参考文献条目书签 `_Ref_ref_<n>` + 正文上标 REF 字段交叉引用（见 `references/reference-normalization.md`）。
  - 横向页竖向排列页码（`wps:wsp` + VML `v:rect` 双轨，见 `references/landscape-vertical-pagenum.md`）。
- 决策校验脚本 `scripts/validate_decision.py`，检查 `documentRules` / `headerFooterDecisions` / `sectionDecisions` 配对完整性。
- 样例：`scripts/sample-decision.json` / `scripts/sample-render-spec.json` / `scripts/sample-chem-render-spec.json`。
- Codex agent 元数据 `agents/openai.yaml`。

### Notes
- 当前仅 Windows x64 提供官方预编译产物；其他平台请自行构建（`build.ps1` / `build.sh`）。

[Unreleased]: https://github.com/yaya200325/docx-smart-format/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yaya200325/docx-smart-format/releases/tag/v0.1.0
