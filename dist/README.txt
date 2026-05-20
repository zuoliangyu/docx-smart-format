docx-smart-engine (Rust) — 测试包
====================================

  这是一个用 Rust 重写的 docx 排版引擎，单文件 exe (~520 KB)，
  不需要安装 .NET，不需要 Word/WPS/Office，不联网，跨平台
  (本包是 Windows x64 版本)。

  状态：alpha。结构良构性已自测通过；Word/LibreOffice 真渲染
  保真程度（尤其公式 / 横向页竖排页码）需要你协助人工验证。


包内容
------
  docx-auto-template-engine.exe   引擎主程序
  samples/                        6 份 FormatPlan 样例（输入 JSON）
    01-basic.json                 综合：分节、标题、表格、参考文献
    02-autoformat.json            化学式 / 单位指数 / 上下标自动分词
    03-references.json            参考文献规范化 + 上标 REF 域交叉引用
    04-vertical-pagenum.json      横向页 + 左侧竖排页码 (VML 双轨)
    05-omml-equation.json         4 个真 OMML 公式 (上标/下标/分数/嵌套)
    06-figure.json                图片插入（用 samples/pixel.png）
    pixel.png                     图片样例
    OMML-CHEATSHEET.md            OMML 公式写法速查表
  README.txt                      本文件


怎么跑（PowerShell 或 cmd 都行）
------------------------------

  1. 解压本 zip 到任意目录，进入该目录（含 exe）。

  2. 跑一个最完整的样例：

     .\docx-auto-template-engine.exe build `
         --plan samples\01-basic.json `
         --output out-basic.docx

     生成 out-basic.docx，用 Word/WPS 打开看排版。

  3. 试参考文献规范化：

     .\docx-auto-template-engine.exe build `
         --plan samples\03-references.json `
         --output out-refs.docx `
         --normalize-references

     正文里的上标 [1] / [2,3] / [1-3] 应该变成可以 Ctrl+点击
     跳转到参考文献条目的 REF 域。

  4. 横向页 + 竖排页码：

     .\docx-auto-template-engine.exe build `
         --plan samples\04-vertical-pagenum.json `
         --output out-vml.docx

  5. 其它样例同样替换 --plan 参数即可。


命令面（唯二命令）
------------------

  analyze --input <docx> --output <analysis.json>
      读取已有 docx，输出结构 + 有效样式 JSON（重排模式用）。

  build --plan <format-plan.json> --output <result.docx>
        [--source <docx>] [--normalize-references]
      生成新 docx。带 --source 即重排该 docx；不带即从零生成。
      --normalize-references 触发参考文献书签 + REF 域规范化。

  无 --source 且不带 --normalize-references 时引擎不联网、不读
  任何外部文件，纯函数式。


帮我重点看哪些
--------------

  请你打开生成的 docx，重点反馈：

  □ 中英文字体是否生效（默认中文宋体 / 英文 Times New Roman）。
  □ 标题是否粗体并显示在大纲。
  □ 表格三线表（顶线 + 表头下线 + 底线）是否对齐。
  □ 化学式 H2O / m^2 / x^2 上下标是否正确分段（02-autoformat）。
  □ 参考文献 [1] 后是否有 Tab 对齐（不是空格）。正文里 Ctrl+点击
    上标 [1] 能否跳转到对应条目（03-references）。
  □ 横向页左侧竖排页码是否显示且可见（04-vertical-pagenum）。
  □ 公式 E = mc^2 是否正确显示为上标 2（05-omml-equation）。
  □ 图片是否居中显示（06-figure）。

  如果任何一项不对，告诉我具体现象（最好截图 + 哪个样例），
  以及你的 Word/WPS 版本。


关于公式 (OMML)
---------------

  公式必须以 OMML XML 形式写在 plan 块的 equation.xml 字段里。
  引擎原样嵌入到 docx，Word/WPS 按真实数学排版渲染。

  ❗ 引擎不做"数学语法 -> OMML"的翻译。LaTeX 风格的 x^2 / x_1
     写在普通文本里会被自动分词上下标，但那不是真 OMML 排版，
     正式公式不要走文本退化路径。

  写法见 samples/OMML-CHEATSHEET.md，里面有：
  • 上标 / 下标 / 上下都有
  • 分数（含嵌套）
  • 根号 / 求和 / 积分 / 极限 / 自动伸缩括号 / 矩阵
  • 从 Word 反推 OMML 的调试技巧

  samples/05-omml-equation.json 内含 4 个真 OMML 公式样例，
  其中第 4 个就是变压器电压-匝数比 U₁/U₂ = n₁/n₂，可直接对照。


已知局限
--------

  • analyze 只在本引擎自产的 docx 上做过 100% 字段比对。
    野生 Word/WPS 文件可能踩到没覆盖的样式继承边界。
  • 横向竖排页码：只暴露了固定默认位置 (左侧装订边)，
    暂未开放自定义坐标。


联系
----
  问题/截图直接发回来即可。
