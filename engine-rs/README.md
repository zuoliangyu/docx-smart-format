# docx-smart-engine (Rust)

`docx-smart-format` 的 Rust 重写引擎。**单文件二进制 (~0.4 MB)，零运行时依赖，
跨平台**——同样的 `FormatPlan` JSON 契约，同样的 OOXML 输出能力，但比 .NET
版本小 95 倍且不依赖 Office / WPS / .NET。

状态：**alpha**。`build` / `analyze` 已覆盖 SKILL.md 描述的大部分能力并在真
Word 上实测渲染通过；`apply` / `render` 这两条 legacy 命令以及它们承载的
"毕业论文预设" / "模板化结构重排" 暂未在 Rust 引擎实现，仍走 .NET 引擎。

------------------------------------------------------------------------

## 命令面

```
docx-auto-template-engine analyze --input <docx> --output <analysis.json>
docx-auto-template-engine build   --plan <format-plan.json> --output <docx>
                                  [--source <docx>] [--normalize-references]
```

- `analyze`：读 docx，解析有效样式继承（docDefaults → pStyle basedOn 链 → 直接
  rPr）；输出块结构 + 每块的有效字体/字号/粗体/对齐/标题级 + per-run 数组（含
  bold/italic/verticalAlign）。
- `build`：FormatPlan → docx。
  - 无 `--source`：从零生成。
  - 有 `--source`：重排模式（overlay）。`block.ref` 指向源文档块路径
    （如 `/body/paragraph[3]`），引擎取回源段文本与每个 run 的内联格式（粗/斜
    /上下标/Tab），再叠加 `block.format` 的段级覆盖。
- `--normalize-references`：触发参考文献规范化（每个 `[n]` 条目加书签 + 正文
  上标 `[n]` 转 REF 字段交叉引用 + `[n]<Tab>` 悬挂缩进）。

------------------------------------------------------------------------

## 已覆盖能力

| 能力 | 实现切片 |
|---|---|
| 段落：标题级、对齐、缩进、行距段距、首行/悬挂、字体/字号、颜色、粗斜下划线、上下标 | RS0 |
| 多分节：sectPr/pgSz/pgMar/orientation/pgNumType/titlePg/section type | RS1 |
| OPC 部件 + 关系机制 + 居中 PAGE 字段页脚 | RS2 |
| 表格：三线表、表头加粗 + 下框线、自适应宽度 | RS3 |
| 图片：DrawingML inline picture + Package（媒体部件 + rels + content-types） | RS4 |
| analyze：有效样式继承解析 + per-run 提取 | RS5 / RS7 |
| overlay 重排：源块文本 + 标题级继承 + 内联格式逐 run 还原 | RS6 / RS7 |
| AutoFormat：化学式 (H2O)、单位指数 (m², cm³)、^/_ 上下标标记 | RS8 |
| 参考文献规范化：书签 + REF 复杂字段 + `[n]<Tab>` 悬挂缩进 | RS9 |
| OMML 公式：原样嵌入调用方提供的 OMML XML，自动外裹 `<m:oMathPara>` 并加 centerGroup | RS10 |
| VML 竖排页码：`mc:AlternateContent` + wps:wsp + VML v:rect 双轨 | RS11 |

------------------------------------------------------------------------

## 待办 / 已知缺口

| | 内容 | 风险 |
|---|---|---|
| 1 | analyze 鲁棒化：当前只在引擎自产 docx 上做过 100% 字段比对；野生 Word/WPS 文档（含 latentStyles / style 别名 / 复杂 basedOn 链等）未测 | 中 |
| 2 | 毕业论文预设（`builtin-undergraduate-thesis`）：FormatPlan 未暴露 | 中 |
| 3 | 模板化结构重排（apply --template <template.docx>）：FormatPlan overlay 暂只做段级文本+格式重用，无模板结构映射 | 中 |
| 4 | 逐部件页眉控制（不同节绑定不同 header XML 文件）：FormatPlan 只暴露 `document.headerFooter` 全局开关 | 中 |
| 5 | 横向竖排页码：只暴露 `headerFooter.pageNumber: "left-vertical"`，坐标/方向/边框等定制项未开放 | 低 |

带能力损失的全替代 .NET 需要补齐 #2–#4。在那之前 `apply` / `render` 必须保留。

------------------------------------------------------------------------

## 架构

```
FormatPlan JSON  ─→  src/plan.rs       (serde 模型)
                     │
                     ├─→ src/refs.rs   (参考文献规范化：书签 + 复杂 REF 字段)
                     ├─→ src/autoformat.rs  (化学式/单位指数/^_ 分词)
                     └─→ src/docx.rs   (raw-OOXML 写出)
                          │
docx 输入 ─→ src/analyze.rs (quick-xml 流式读 + 有效样式解析)  ─→ build overlay
                                                                  ├─→ src/refs.rs
                                                                  ├─→ src/autoformat.rs
                                                                  └─→ src/docx.rs
```

**raw-OOXML 优先**：未使用 `docx-rs` 等 docx 库。`mc:AlternateContent` + VML
双轨、OMML、模板样式拷贝、域代码等没有成熟 Rust 库覆盖，全部手写 XML 反而最
可控。依赖只有 `zip`（OPC 包）、`serde` / `serde_json`（plan）、`quick-xml`
（analyze 读 docx）。

------------------------------------------------------------------------

## 构建

```bash
cd engine-rs
cargo build --release
# 产物: target/release/docx-auto-template-engine.exe (Windows)
#       target/release/docx-auto-template-engine     (Linux/macOS)
```

`Cargo.toml` 已开 `opt-level = "z"` + LTO + strip + panic=abort，release 体积
~0.4 MB。

------------------------------------------------------------------------

## 测试

仓库根的 `tests/golden/` 是 .NET 引擎的 golden 网（架构重写时建立，见
`tests/golden/README.md`）。Rust 引擎不复用这套（两个引擎的 OOXML 输出必然不同
但等价，逐字节比对没意义），改用：

1. **结构良构性测试**：每个切片提交时都跑过 6 份样例（`dist/samples/0*.json`），
   解压产物、检查关键 OOXML 元素计数与位置（参考各 RS 提交 message 的 "Verified"
   段落）。
2. **真实 Word 渲染回归**：由测试者打开 docx 人工确认。已踩过/修复的渲染问题：
   - REF 字段中数字未上标 → 改 `<w:fldSimple>` 为复杂字段 + 每 run 带 rPr
   - 表注在表下 → SKILL 规定表注默认在表上，调换 caption emit 顺序
   - OMML 当文本渲染 → 每个 `<m:r>` 必须带 `<w:rFonts w:ascii="Cambria Math"/>`
     字体提示（详见 `dist/samples/OMML-CHEATSHEET.md`）
   - F9 更新后参考文献变 `[[1]]` → bookmark 改为只包数字，方括号在外

------------------------------------------------------------------------

## 设计要点（踩坑后归档）

- **`m:oMathPara` 写在 `<w:p>` 内**（虽然 schema 允许 body 级，但 Word 自身导出
  公式时把它放在 `<w:p>` 里，照搬最稳）。
- **每个 `<m:r>` 必须带 Cambria Math 字体提示**，否则 Word 当文本渲染。
- **`<w:fldSimple>` 不可靠**：在某些 Word 版本里它会丢掉缓存 run 的 rPr。
  REF / 域代码全部用 begin/separate/end 复杂字段，每个 run 各带 rPr。
- **section 子元素顺序硬约束**：sectPr 内必须 `headerReference` / `footerReference`
  在前，`w:type` 次之，再 `pgSz` / `pgMar` / `pgNumType` / `titlePg`。
- **OpenXML 关系 id 是随机字符串**（rId + 16 hex），跨引擎逐字节比对没意义。
- **OOXML 表格 (`<w:tbl>`) 是 `<w:p>` 的兄弟，不能挂 sectPr**。closing
  section 末元素是表格时要追加一个空段落承载 sectPr。
