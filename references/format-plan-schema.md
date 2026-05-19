# FormatPlan Schema

`docx-smart-format` 中 LLM 输出的**唯一契约**。引擎把 `FormatPlan` 经 Compiler 降级为内部 RenderSpec 再写回 docx —— LLM 不接触 OOXML，也不接触 RenderSpec。

## 顶层

```jsonc
{
  "docType": "academic-thesis",   // 语义提示，引擎不强依赖；不确定填 general-formal-doc
  "document": { ... },            // 文档级规则
  "sections": [ ... ],            // 命名分节定义（可空）
  "blocks": [ ... ]               // 正文块序列
}
```

## `document`

| 字段 | 类型 | 说明 |
|---|---|---|
| `title` / `author` | string | 包属性 |
| `pageSize` | string | `a4`(默认) / `letter` / `legal` / `a5` |
| `orientation` | string | `portrait` / `landscape` |
| `margins` | object | `{ top, bottom, left, right }`，值为 twips 字符串 |
| `cnFont` / `enFont` | string | 中文 / 英文字体；中英文必须分设 |
| `baseFontPt` | number | 正文字号（磅，引擎转半磅） |
| `lineSpacing` | number | 倍数，如 `1.5`（引擎转 `240*n` auto 行距） |
| `headerFooter` | object | 见下 |

`headerFooter`：
- `oddEven` (bool)：奇偶页眉页脚不同。
- `firstDifferent` (bool)：首页不同。
- `pageNumber` (string)：`none` / `continuous` / `center-page-number`（居中 PAGE 字段页脚）。

> 文档级 `cnFont` / `enFont` / `baseFontPt` / `lineSpacing` 作为缺省，自动下沉到每个块；块 `format` 同名字段优先。

## `sections`

命名分节，块通过 `sectionKey` 引用。**相邻块 `sectionKey` 变化时引擎插入分节符。**

| 字段 | 说明 |
|---|---|
| `key` | 稳定 ID，供块 `sectionKey` 引用 |
| `type` | OOXML 节类型：`nextPage` / `continuous` / `evenPage` / `oddPage` / `nextColumn` |
| `orientation` | 该节方向（横向插页用） |
| `margins` | 该节页边距 |
| `pageStart` | 该节页码起始值 |
| `pageNumFmt` | `decimal` / `upperRoman` / `lowerRoman` |
| `titlePage` | 该节首页单独 |

## `blocks`

每个块**靠有没有 `ref` 区分意图**：

- **覆盖型**：有 `ref`（`analyze` 输出里某 body 块的 `path`，如 `/body/paragraph[2]`）。原文内容/行内 run 由引擎从源文档取回，只应用本块 `format`。`text` 被忽略。
- **生成型**：无 `ref`。自带 `text` / `image` / `table` / `equation`。

| 字段 | 说明 |
|---|---|
| `ref` | 源块 path；有=覆盖，无=生成 |
| `role` | `title` / `heading1`..`heading3` / `body` / `reference` / `caption` / `figure` / `table` / `equation` / `pageNumber` |
| `sectionKey` | 所属分节 key |
| `text` | 生成型正文文本（含 `^`/`_` 上下标标记、化学式、单位指数，引擎侧自动分段） |
| `caption` | `figure`/`table` 的题注（自动在对象后追加一段居中文字） |
| `format` | 见下，扁平格式（段落+行内合一） |
| `image` | `{ path, contentType, widthEmu, heightEmu, altText }` |
| `table` | `{ rows: string[][] }` |
| `equation` | `{ text, xml, displayMode }`，`displayMode`=`inline`/`display` |

`role` 映射：`heading1..3`→对应标题级别；`reference`→参考文献条目块；`pageNumber`→插 PAGE 字段段；`figure`/`table`/`equation`→对象块（生成型需对应对象字段）。

## `format`（扁平格式）

| 字段 | 作用 |
|---|---|
| `bold` / `italic` (bool) | 粗 / 斜 |
| `underline` | `single` / `double` / `none` |
| `fontColor` | 十六进制，如 `FF0000` |
| `highlight` | `yellow` / `green` / … |
| `verticalAlign` | `superscript` / `subscript` / `baseline` |
| `cnFont` / `enFont` | 覆盖文档级字体 |
| `fontPt` (number) | 覆盖字号（磅） |
| `align` | `left` / `center` / `right` / `both` / `distribute` |
| `firstLineIndent` | 首行缩进 twips |
| `hangingIndent` | 悬挂缩进 twips（参考文献/编号列表） |
| `lineSpacing` | 行距值（twips，auto 规则） |
| `beforeSpacing` / `afterSpacing` | 段前 / 段后 twips |
| `pageBreakBefore` (bool) | 段前分页 |

> 覆盖型块只写**确定要改的** `format` 字段；未写字段保留源文档原样。

## 参考文献规范化

`build --normalize-references`（或显式不传 false）时，引擎对 `role: reference` 块做书签 + 正文 REF 字段交叉引用 + `[n]<TAB>` 规范化。LLM 只需：把正文里学术引用的 `[n]` 用 `format.verticalAlign: "superscript"` 标上标，**不要**自己写书签/域/Tab。详见 [reference-normalization.md](reference-normalization.md)。

## 校验

`scripts/validate_decision.py` 仍只校验**旧版 decision 结构**，不识别 FormatPlan；FormatPlan 校验器待 R4 提供。当前以 [../scripts/sample-format-plan.json](../scripts/sample-format-plan.json) 为结构参照。
