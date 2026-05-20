# 贡献指南

感谢你愿意为 docx-smart-format 贡献代码、文档或想法。这里集中放对贡献者最有用的信息。请先粗读完再开始动手。

## 仓库结构速记

- `SKILL.md`：LLM 入口。规则修改优先在这里及 `references/` 中体现。
- `references/`：规则与能力清单。**修改这些会直接改变 LLM 行为**，请同步考虑是否影响样例 JSON。
- `references/format-plan-schema.md`：LLM 唯一契约 FormatPlan 的字段定义。
- `engine-rs/`：Rust 引擎源码 (Cargo)；详见 `engine-rs/README.md`。
- `engine-rs/target/`：本地 `cargo build --release` 出来的产物,**不要提交到 git**(已在 `.gitignore`)。
- `dist/`：测试包源（README、samples、OMML 速查表）。`dist/docx-auto-template-engine.exe` 和 `dist/output/` 是构建产物,gitignored。
- `agents/openai.yaml`：Codex/OpenAI agent 元数据。

## 本地环境

| 工具       | 用途                                          | 推荐版本    |
|------------|-----------------------------------------------|-------------|
| Rust       | 构建引擎 (`cargo build`)                      | 1.85+       |
| Git        | 版本控制                                       | 任意近期版本|
| PowerShell | Windows 上测试运行                             | 5.1+ / 7    |
| Word / WPS | 人工校验产物 docx 渲染                         | 任意现代版  |

**不需要**装 .NET / Office / Python 来构建或运行引擎。

## 构建引擎

```powershell
cd engine-rs
cargo build --release
# 产物: target/release/docx-auto-template-engine.exe (Windows)
#       target/release/docx-auto-template-engine     (Linux/macOS)
```

`Cargo.toml` 已开 `opt-level = "z"` + LTO + strip + panic=abort，release 体积 ~520 KB。
跨平台编译: `cargo build --release --target x86_64-unknown-linux-gnu` 等。

## 跑测试与校验

引擎当前没有自动化测试套件。变更后请:

1. **构建检查**:`cargo build --release` 必须无 error。
2. **结构良构性 smoke**:对 `dist/samples/0*.json` 全部生成 docx,Python 解压检查关键 OOXML 元素计数 (参考各 RS 提交 message 的 "Verified" 段落)。
3. **真实 Word 渲染回归**:打开生成的 docx 人工确认。已踩过/修复的渲染问题见 `engine-rs/README.md` 的"设计要点(踩坑后归档)"。

## 提交规范

### 分支与 PR

- 从 `main` 切出工作分支，命名建议 `feat/<topic>` / `fix/<topic>` / `docs/<topic>` / `chore/<topic>`。
- PR 描述请说明:**做了什么**、**为什么这么做**、**怎么验证**(尤其是真 Word 上验过哪些样例)。
- 影响 LLM 行为(`SKILL.md` / `references/`)的改动,请说明对 FormatPlan 字段的兼容性影响;必要时一并更新 `dist/samples/*.json` 与 `CHANGELOG.md` 的 `[Unreleased]` 节。

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

`scope` 建议：`skill` / `references` / `engine` / `dist` / `ci` / `release`。

### 代码风格

- Rust:跟随 `engine-rs/src/` 现有风格;`cargo fmt` + `cargo clippy -- -D warnings`。
- Markdown:中文为主、英文术语保留原文;标题层级与现有 `references/*.md` 一致。

## Issue 规范

模板见 `.github/ISSUE_TEMPLATE/`。提交前请确认：

- 已搜索过相似 issue。
- 列出能复现的最小步骤;附 FormatPlan JSON 时务必脱敏。
- 若涉及生成 `.docx` 行为异常,请说明用 Word / WPS / LibreOffice 哪个打开、版本号,**最好把生成的 docx 文件本身一并发上来**(看 XML 比看截图准确)。

安全相关 issue 走 [SECURITY.md](SECURITY.md) 私下渠道，**不要公开提**。

## 行为准则

参与者须遵守 [行为准则](CODE_OF_CONDUCT.md)。

## 许可证

提交 PR 即视为同意你的贡献按本项目根 [LICENSE](LICENSE) Apache 2.0 授权。
