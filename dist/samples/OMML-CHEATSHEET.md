# OMML 公式速查表

引擎对公式的处理: `equation.xml` 字段里你给什么 OMML XML，引擎就原样嵌入到 docx
里。Word/WPS 会按真实数学排版渲染。**引擎不做"数学语法 → OMML"翻译**——这是
调用方(通常是 LLM)的工作。

m: 命名空间已经在 document 根上声明 (`xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"`)，
所以你直接写 `<m:oMath>...</m:oMath>` 就能用。

------------------------------------------------------------------------

## ⚠️ 关键: 每个 `<m:r>` 必须带 Cambria Math 字体提示

这是实测踩坑得到的硬性要求。Word/WPS 对 OMML 的渲染依赖 `m:r` 上的字体提示;
**不带的话 Word 把整段公式当普通文本渲染**(就算 m:oMathPara / m:sSup 结构完全
正确也不行)。

**正确写法** (每个 m:r 都这样):

```xml
<m:r>
  <w:rPr>
    <w:rFonts w:ascii="Cambria Math" w:hAnsi="Cambria Math"/>
  </w:rPr>
  <m:t>x</m:t>
</m:r>
```

**错误写法** (会被当文本渲染):

```xml
<m:r><m:t>x</m:t></m:r>     ❌  缺字体提示
```

下文所有示例都按正确写法给。为了节省篇幅,后面用 `[MR]` 代指
`<w:rPr><w:rFonts w:ascii="Cambria Math" w:hAnsi="Cambria Math"/></w:rPr>`
这一段——你照抄时把 `[MR]` 替换成那串就行。

------------------------------------------------------------------------

## 容器

| 用法 | 写法 |
|---|---|
| 行内公式 | `<m:oMath>...</m:oMath>` |
| 独立公式（一行单独显示） | `<m:oMathPara><m:oMathParaPr><m:jc m:val="centerGroup"/></m:oMathParaPr><m:oMath>...</m:oMath></m:oMathPara>` + plan 里 `displayMode: "display"` |

独立公式建议带上 `m:oMathParaPr` 的 `centerGroup` 居中,与 Word 自身导出一致。

------------------------------------------------------------------------

## 文本 run

```xml
<m:r>[MR]<m:t>E</m:t></m:r>
<m:r>[MR]<m:t xml:space="preserve">E = m</m:t></m:r>   <!-- 带空格 -->
```

XML 转义: `>` 写 `&gt;`，`<` 写 `&lt;`，`&` 写 `&amp;`。

------------------------------------------------------------------------

## 上标 / 下标

| 数学式 | OMML |
|---|---|
| `x²` | `<m:sSup><m:e><m:r>[MR]<m:t>x</m:t></m:r></m:e><m:sup><m:r>[MR]<m:t>2</m:t></m:r></m:sup></m:sSup>` |
| `x₁` | `<m:sSub><m:e><m:r>[MR]<m:t>x</m:t></m:r></m:e><m:sub><m:r>[MR]<m:t>1</m:t></m:r></m:sub></m:sSub>` |
| `x₁²` (上下都有) | `<m:sSubSup><m:e>...</m:e><m:sub>...</m:sub><m:sup>...</m:sup></m:sSubSup>` |

`<m:e>` 是基,`<m:sup>`/`<m:sub>` 是上下标内容,各自里面再放 `<m:r>[MR]<m:t>`。

------------------------------------------------------------------------

## 分数

```xml
<m:f>
  <m:num><m:r>[MR]<m:t>a</m:t></m:r></m:num>
  <m:den><m:r>[MR]<m:t>b</m:t></m:r></m:den>
</m:f>
```

分子分母里可以再嵌套任何 OMML(包括下标、根号、嵌套分数)。例:`U₁ / U₂`:

```xml
<m:f>
  <m:num>
    <m:sSub>
      <m:e><m:r>[MR]<m:t>U</m:t></m:r></m:e>
      <m:sub><m:r>[MR]<m:t>1</m:t></m:r></m:sub>
    </m:sSub>
  </m:num>
  <m:den>
    <m:sSub>
      <m:e><m:r>[MR]<m:t>U</m:t></m:r></m:e>
      <m:sub><m:r>[MR]<m:t>2</m:t></m:r></m:sub>
    </m:sSub>
  </m:den>
</m:f>
```

------------------------------------------------------------------------

## 根号

```xml
<m:rad>
  <m:deg><m:r>[MR]<m:t>3</m:t></m:r></m:deg>   <!-- 度数,可省即开平方 -->
  <m:e><m:r>[MR]<m:t>x</m:t></m:r></m:e>
</m:rad>
```

省略 `<m:deg>` 就是平方根 √x;留空 `<m:deg/>` 也是平方根。

------------------------------------------------------------------------

## 求和 / 积分 / 极限 (nary 运算符)

```xml
<!-- ∑ 从 i=1 到 n 的 x_i -->
<m:nary>
  <m:naryPr><m:chr m:val="∑"/></m:naryPr>
  <m:sub><m:r>[MR]<m:t>i=1</m:t></m:r></m:sub>
  <m:sup><m:r>[MR]<m:t>n</m:t></m:r></m:sup>
  <m:e>
    <m:sSub>
      <m:e><m:r>[MR]<m:t>x</m:t></m:r></m:e>
      <m:sub><m:r>[MR]<m:t>i</m:t></m:r></m:sub>
    </m:sSub>
  </m:e>
</m:nary>
```

把 `m:chr` 改成 `∫` / `∏` / `⋃` 等,就是积分/累乘/并集。极限用 lim 不用 chr:

```xml
<m:func>
  <m:fName>
    <m:limLow>
      <m:e><m:r>[MR]<m:t>lim</m:t></m:r></m:e>
      <m:lim><m:r>[MR]<m:t>x→0</m:t></m:r></m:lim>
    </m:limLow>
  </m:fName>
  <m:e>...</m:e>
</m:func>
```

------------------------------------------------------------------------

## 括号 (自动伸缩)

```xml
<m:d>                              <!-- delimiter -->
  <m:dPr>
    <m:begChr m:val="("/>
    <m:endChr m:val=")"/>
  </m:dPr>
  <m:e>...内容...</m:e>
</m:d>
```

不用 `m:d` 而直接写 `(` 和 `)` 也能跑,但不会随内容高度伸缩。

------------------------------------------------------------------------

## 矩阵 / 多行

```xml
<m:m>
  <m:mr>
    <m:e>...单元1...</m:e>
    <m:e>...单元2...</m:e>
  </m:mr>
  <m:mr>
    <m:e>...单元3...</m:e>
    <m:e>...单元4...</m:e>
  </m:mr>
</m:m>
```

------------------------------------------------------------------------

## 一个完整 plan 块

`[MR]` 已展开成完整字体提示,这就是最终交给引擎的样子:

```json
{
  "role": "equation",
  "equation": {
    "xml": "<m:oMathPara><m:oMathParaPr><m:jc m:val=\"centerGroup\"/></m:oMathParaPr><m:oMath><m:f><m:num><m:sSub><m:e><m:r><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:hAnsi=\"Cambria Math\"/></w:rPr><m:t>U</m:t></m:r></m:e><m:sub><m:r><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:hAnsi=\"Cambria Math\"/></w:rPr><m:t>1</m:t></m:r></m:sub></m:sSub></m:num><m:den><m:sSub><m:e><m:r><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:hAnsi=\"Cambria Math\"/></w:rPr><m:t>U</m:t></m:r></m:e><m:sub><m:r><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:hAnsi=\"Cambria Math\"/></w:rPr><m:t>2</m:t></m:r></m:sub></m:sSub></m:den></m:f></m:oMath></m:oMathPara>",
    "displayMode": "display"
  }
}
```

------------------------------------------------------------------------

## 退化路径(不推荐)

如果 `equation.xml` 不填,只填 `equation.text`,引擎会按普通文本渲染。文本里
`x^2` / `x_1` 这种标记会被自动分词器(generate 侧)转成真上下标,但**不是真
OMML 数学排版**,字间距、字体、对齐都不会按数学样式来。**正式公式必须用 xml**。

------------------------------------------------------------------------

## 调试技巧

1. 在 Word 里手写一个公式 (Alt+= 进数学区,敲完后保存)。
2. 把 docx 解压,看 `word/document.xml` 里的 `<m:oMath>...</m:oMath>` 块,
   照着抄到你的 `equation.xml` 里。这种从 Word 反推的 OMML 一定带 Cambria Math
   提示,可放心抄。
3. 用 [`samples/05-omml-equation.json`](05-omml-equation.json) 里的 4 个
   样例作为起点——里面 41 个 `m:r` 全带字体提示,Word 实测渲染正确。
