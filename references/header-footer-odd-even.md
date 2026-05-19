# 页眉页脚深度规则

> **契约说明（含已知缺口）**：LLM 侧契约是 [FormatPlan](format-plan-schema.md)。文档级开关经 `document.headerFooter`（`oddEven` / `firstDifferent` / `pageNumber`）表达。
>
> 本文档描述的**逐部件页眉控制**（按 `decisionKey` 注册多条 header、`sourcePath` 复用模板内特定 header XML、奇偶页分别绑定不同内容、STYLEREF 取值部件）属引擎已有能力，但**当前 FormatPlan 尚未把这套逐部件覆盖完整暴露出来**——这是架构重写引入的已知缺口，待后续在 FormatPlan 增补 `headerFooter.parts` 一类结构。下文中的 `headerFooterDecisions` 指引擎内部降级形式，供理解原理与定位问题，暂不可由 FormatPlan 直接驱动其全部细节。

## OOXML `type` 语义（强约束，禁止望文生义）

`<w:headerReference>` / `<w:footerReference>` 的 `w:type` 属性是 OOXML 强约定：

| `type` 取值 | 实际作用 |
|---|---|
| `default` | **主页眉/页脚**。当 `<w:evenAndOddHeaders/>` 开启时，等同于"奇数页"。 |
| `even` | **偶数页**页眉/页脚，仅在 `<w:evenAndOddHeaders/>` 开启时生效。 |
| `first` | **首页**页眉/页脚，仅在该节启用 `<w:titlePg/>` 时生效。 |

不要把 `default` 误读为"通用"或"偶数页"。在奇偶模式下 `default` 就是奇数页。

引擎接受的 `headerType` / `footerType` 字符串：

- `"default"` —— OOXML 标准写法，**推荐**
- `"odd"` —— 等价于 `"default"`；引擎会在见到这个值时自动写入 `<w:evenAndOddHeaders/>`，明确表达"奇偶页不同"的意图
- `"even"` —— 偶数页
- `"first"` —— 首页（必须在该节 `SectionDecision` 中同时设置 `titlePage: true`）

## 奇偶页眉的最小决策结构

**一条 reference 是不够的。** 如果只输出 `headerType: "default"` 一条，文档全部页面共用同一个页眉，等于没开启奇偶模式。要做出"奇数页论文标题、偶数页章节标题"的效果，每一个需要奇偶差异的 `SectionDecision` 必须**配对**输出：

- 一条奇数页内容（`"odd"` 或 `"default"`）
- 一条偶数页内容（`"even"`）
- 可选一条首页内容（`"first"` + `titlePage: true`）

由于 `SectionDecision` 自身只能携带一个 `headerType`，配对方式是：

- 在 `headerFooterDecisions` 中按 `decisionKey` 注册多条 header 内容（每条带 `type`）
- 在 `SectionDecision` 里通过 `headerDecisionKeys`（复数）同时引用多条
- 引擎按 `type` 去重，不按出现顺序

实际操作上，最稳的做法是**让 apply 引擎复用模板里已经存在的、内容正确的 header XML 文件**：通过 `headerFooterDecisions` 的 `sourcePath` 指向模板里"奇数页论文标题"、"偶数页 STYLEREF"的两个 XML，让引擎在 sectPr 中按 type 同时挂上即可。

## STYLEREF 字段：必须由模板提供，且禁止使用 `\s`

引擎本身不会从决策 JSON 凭空生成 STYLEREF 字段（grep `fldChar` / `instrText` / `StyleRef` 在引擎源码内零命中）。任何"偶数页显示当前一级章节标题"的页眉，**必须靠 `--template` 提供的模板 docx**——模板里那个 header XML 已经手写了正确的 STYLEREF 字段，引擎只负责把它绑定到正确的 sectPr/type 上。

### 模板里 STYLEREF 字段的正确写法

```
STYLEREF "标题 1" \* MERGEFORMAT
```

或英文样式名：

```
STYLEREF "Heading 1" \* MERGEFORMAT
```

完整 OOXML 形式：

```xml
<w:fldChar w:fldCharType="begin"/>
<w:instrText xml:space="preserve"> STYLEREF "Heading 1" \* MERGEFORMAT </w:instrText>
<w:fldChar w:fldCharType="separate"/>
<w:t></w:t>
<w:fldChar w:fldCharType="end"/>
```

- `"Heading 1"` 是模板里一级标题的 styleId，不一定等于显示名"标题 1"。如果模板里用的是中文样式 ID，需要把这里替换成实际 styleId。

### 禁止使用 `\s` 开关

- `\s` 返回的是段落编号（如 "1.1"），不是段落文字
- 当一级标题没有用 Word 自动编号功能编排（很多论文里"第一章"或"1 绪论"的数字是手敲的）时，`\s` 永远返回空白或仅返回编号字符，**不会显示标题文字**
- 正常的"章节标题页眉"几乎从来不该用 `\s` —— 默认就该用 `\* MERGEFORMAT`

LLM 在选用模板前必须打开候选 header XML 检查其中的 `<w:instrText>`：

- 看到 `STYLEREF 1 \s` 或 `STYLEREF "标题 1" \s` 必须警告用户："此模板的章节页眉字段使用了 `\s` 开关，会显示编号而不是标题文字，需要在模板里改成 `\* MERGEFORMAT`，否则偶数页页眉会空白。"
- 不要自作主张照搬包含 `\s` 的页眉去 apply。

## apply 模式：必须先盘点全文 headerReference 与文件内容

`apply` 模式下源 docx / 模板 docx 都可能携带遗留的硬写页眉（例如 `<w:t>摘要</w:t>` 硬编码在 header4.xml 里）。LLM 在生成决策前必须：

1. 从 `analyze` 输出里列出每个 `sectPr` 现有的 `headerReference` 与对应的 `r:id`、`type`。
2. 对照每个 `r:id` 指向的 header XML 文件，列出当前内容（包括硬写文字、字段、空段落三种情况）。
3. 标注哪些是"空段落，可安全覆写"、哪些是"硬写文字，需要决策里显式清空或替换"。
4. 决策 JSON 中：
   - 涉及奇偶页眉的所有节都要显式输出两条 reference（不能依赖某一节继承前一节）
   - 涉及前置部分的节，如果不希望显示原模板的"摘要 / 封面 / 目录"等硬写文字，必须在 `headerFooterDecisions` 里用 `action: "clear"` 或 `action: "replace"` 显式处理

### 常见踩坑（已验证过的真实失败）

**踩坑一：前置节遗留页眉**
LLM 只为正文主体节配置了奇偶页眉，没有处理前置节里早就绑定到 `header4.xml`（内容是硬写"摘要"二字）的旧引用。结果整本前置部分（封面后、摘要、目录、英文摘要、目录页）奇数页全显示"摘要"。修法：要么在前置节的 `headerFooterDecisions` 里把对应 header 文件清空，要么改写前置节的 `headerReference` 指向空文件。

**踩坑二：按 r:id 顺序判断绑定**
不要假设 `header1.xml` 就是"主页眉"、`header2.xml` 就是"副页眉"。`r:id` 与 header 文件名之间是任意映射，**关键是该文件在 sectPr 中被引用为哪个 `type`**。LLM 在决策中不应该说"在 header1 里写论文标题"，应该说"在 type=default 的 header 里写论文标题，让引擎选择对应的 XML 文件"。

## 页码（pgNumType + PAGE 字段两层结构）

页码涉及两个独立的设置，必须区分：

1. **`<w:pgNumType>`**（写在 `sectPr` 里）—— 决定该节的页码**编号格式与起始值**。例如：
   - `<w:pgNumType w:fmt="upperRoman" w:start="1"/>` —— 罗马数字 I, II, III 从 1 开始
   - `<w:pgNumType w:start="1"/>` —— 阿拉伯数字从 1 重新开始（无 `fmt` 即 decimal）
2. **`<w:fldChar> PAGE` 字段**（写在 `footer*.xml` 或 `header*.xml` 里）—— 决定页码**在哪显示**。引擎通过 `pageNumberField: true` 写入。

只设 `pgNumType` 不设 `PAGE` 字段：**编号体系存在但页面上看不到任何页码**（这是常见 bug——pgNumType="upperRoman" 但 footer 是空段落，前置部分根本不显示页码）。

### "从正文开始加页码" 的标准实现

用户说"从正文开始加页码"通常意味着：

1. 封面、摘要、目录页**不显示任何页码**
2. 正文从阿拉伯数字 1 重新开始计数

实现要点：

- 前置节：不要写 `pgNumType`（或写了也无意义），footer 内容必须不含 `PAGE` 字段
- 正文起始节：`pgNumType w:start="1"`（不要带 fmt 属性，让它默认 decimal），footer 用一个新的 footer 文件（带 `pageNumberField: true`）

### "前置罗马数字、正文阿拉伯数字"标准学术论文样式

- 前置节：`pgNumType w:fmt="upperRoman" w:start="1"`，footer 必须含 `pageNumberField: true`
- 正文起始节：`pgNumType w:start="1"`（无 fmt，自动 decimal），footer 也含 `pageNumberField: true`，并且要换一个不同的 footer 文件以避免延续前置节的内容

### 禁止做法

- 不要让 LLM 直接写 "I"、"II"、"1" 等普通文本数字代替字段
- 不要只设 `pgNumType` 不写 `PAGE` 字段
- 不要让正文沿用前置节的 footer 文件——必须新建/选择不同的 footer，否则 `pgNumType="start=1"` 会被覆盖或视觉上跟前置混淆
