# 参考文献与正文交叉引用规范化

毕业论文 / 正式技术报告里，正文中的引用上标（如 `^[1]^`、`^[1,3]^`、`^[1-3]^`）必须与参考文献条目一一对应。本地引擎提供**参考文献规范化**能力。

> **契约说明**：LLM 侧只写 [FormatPlan](format-plan-schema.md)（`role: reference` 块 + 正文 `[n]` 用 `format.verticalAlign: "superscript"`）。下文出现的 “RenderSpec” 指引擎内部降级形式，LLM 不直接编写；书签 / 域 / Tab / 悬挂缩进全部由引擎执行。

## 开启方式

- 显式开关：`build --normalize-references`
- 显式关闭：`build --normalize-references false`

## 规范化行为

### 第一步：参考文献条目改写

对所有 `kind="reference"`、或样式名为 `Reference`、或正文以 `[数字]` 开头的段落：

1. 解析首部 `[n]`，把 `[n]` 与正文之间的所有空白替换为单个 `<w:tab/>`，确保形如 `[1]<TAB>条目内容`。
2. 在 `[n]` 外层插入书签 `_Ref_ref_<n>`（`<w:bookmarkStart>` + `<w:bookmarkEnd>`），bookmarkId 从 1000 开始自增。
3. 若段落未显式设置 `hangingChars` / `hangingIndent`，自动写入 `hangingChars="200"`（约 2 字符）。
4. 若段落未显式设置 `tabStops`，自动追加一个左对齐 tab，位置 `480` 缇（对应正文小四 2 字符宽度）。

### 第二步：正文上标交叉引用改写

对非参考文献段落里的 run，若同时满足：

- run 的 `format.verticalAlign == "superscript"`
- run 的 `text` 含 `[数字]` / `[n,m]` / `[n-m]` 这类引用 token

则把这一个 run 拆成：

- 字面 `[`（仍是上标）
- 每个数字 `n`：若参考文献里存在 `_Ref_ref_<n>`，写成 `<w:fldSimple w:instr=" REF _Ref_ref_n \h ">…<w:t>n</w:t>…</w:fldSimple>`，否则保留为普通上标数字。
- 分隔符 `,` 或 `-`：保留为字面上标字符。
- 字面 `]`（仍是上标）

## 约定与边界

- **只动上标引用**。LLM 在生成 RenderSpec 时负责把正文里所有学术引用打上 `verticalAlign: "superscript"`；非上标的 `[1]` 视为普通文字，不做任何替换。
- **区间引用 `[1-3]` 保留字面 `-`**，只在两端（1 和 3）打 REF 字段，中间的 `-` 不展开成 `1,2,3`，避免突然加长引用列表。
- **多重引用 `[1,3,5]`** 拆成 `[` + REF(1) + `,` + REF(3) + `,` + REF(5) + `]`，每个数字独立指向自己的书签。
- **重复运行幂等**：若段落里已经存在 `bookmarkStart`，跳过重复规范化。
- **缺失目标**：若正文上标里出现 `[7]` 而参考文献只有 `[1]-[3]`，`7` 保留为普通上标字符，不会强制生成 REF。
- 书签命名：`_Ref_ref_<n>`，与 Word 自动生成的书签前缀一致，便于交叉兼容。

## RenderSpec 示例

```json
{
  "body": [
    {
      "kind": "paragraph",
      "paragraph": {
        "runs": [
          { "text": "Smith 等人首先提出该方法" },
          { "text": "[1]", "format": { "verticalAlign": "superscript" } },
          { "text": "。综述" },
          { "text": "[1-3]", "format": { "verticalAlign": "superscript" } },
          { "text": "总结了相关结果。" }
        ]
      }
    },
    {
      "kind": "reference",
      "paragraph": { "text": "[1] Smith J. A foundational method. Journal A, 2019." }
    },
    {
      "kind": "reference",
      "paragraph": { "text": "[2] Zhao L. Multimodal extension. Conf. B, 2021." }
    },
    {
      "kind": "reference",
      "paragraph": { "text": "[3] Liu X. A survey. Journal C, 2023." }
    }
  ]
}
```

引擎按上述规则规范化后，正文里的 `[1]` 与 `[1-3]` 各自变成 `<w:fldSimple w:instr=" REF _Ref_ref_n \h ">` 域；参考文献条目变成 `[1]<TAB>Smith J. …`，并带上 hanging indent 2 字符的悬挂缩进，使第二行整齐对齐到条目正文首字。

## 自检要求（毕业论文 / 正式报告）

无论是模式一（重排现有 docx）还是模式二（直接生成新 docx），输出之前都必须把所有形如 `[n]` 的内容检查一遍：

- 凡是参考文献条目，自动改写为 `[n]<TAB>内容` 并加书签
- 凡是正文里被 LLM 标记为上标的 `[n]`，自动替换为指向 `_Ref_ref_n` 的 REF 字段
- 区间 `[1-3]` 保留字面 `-`，两端各打一个 REF
- 多重 `[1,3]` 每个数字独立打 REF

LLM 自身的判断只决定"哪些 `[n]` 是上标"，其余的书签 / 域 / Tab / 悬挂缩进由本地引擎统一执行，不要把这部分能力写进 RenderSpec。
