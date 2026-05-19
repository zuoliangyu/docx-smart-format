# Changelog

本文件记录 docx-smart-format 的所有重要变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 1.1.0；版本号采用 [语义化版本](https://semver.org/lang/zh-CN/) 2.0.0。

## [Unreleased]

### Added
- 统一 LLM 契约 `FormatPlan`（覆盖/生成同形，靠块 `ref` 区分）与 `FormatPlanCompiler`。
- `build` 命令：有 `--source` 即重排、无即从零生成。
- `references/format-plan-schema.md`；`scripts/sample-format-plan.json`。
- `tests/golden/` 行为回归网（record/verify，规范化 + 易变量清洗）。

### Changed
- 架构重写（分阶段 R0–R4，全程 golden 守护，逐字节零回归）：
  - `DocxRenderer` 1754 行单体拆为 7 个 feature partial。
  - CLI 收敛为 `analyze` + `build`；SKILL.md 257→~150 行（认知瘦身）。
  - 发布改单文件 + 压缩，自包含产物 ~60-70MB → 单个 38MB exe，仍零安装（不需 Office/WPS/.NET）。
- `decision-schema.md` 移除，由 `format-plan-schema.md` 取代。

### Deferred / Known gaps
- 逐部件页眉控制、`builtin-undergraduate-thesis` 预设、模板化结构重排
  尚未由 FormatPlan 覆盖；`apply`/`render` 作为遗留命令**保留且仍支持**，
  待 FormatPlan 达到能力对等后再收敛删除（零能力损失约束）。
- FormatPlan 专用校验器待补；`validate_decision.py` 仍只校验旧 decision。

### Fixed
- 待修复内容入此节。

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
