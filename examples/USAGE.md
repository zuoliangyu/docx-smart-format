# Examples — 离线样例集

这个目录里的 9 个 JSON 是**离线场景测试**,你可以脱离 LLM 直接喂给引擎跑通,
验证 Skill 装得对、引擎能干活。每个样例聚焦一个能力点。

## 前置:引擎在哪

你装好 Skill 后(参考仓库根 README 的"安装"章节),引擎在:

- **Windows**: `C:\Users\<USER>\.claude\skills\docx-smart-format\engine\docx-auto-template-engine.exe`
- **Linux/macOS**: `~/.claude/skills/docx-smart-format/engine/docx-auto-template-engine`

下面的命令假设你 `cd` 进了 Skill 目录(`<skill-dir>`):

```powershell
$skill = "$env:USERPROFILE\.claude\skills\docx-smart-format"
cd $skill
```

## 9 个样例

| 文件 | 演示能力 | 是否需要额外参数 |
|---|---|---|
| `01-basic.json` | 综合:分节 / 标题 / 表格 / 参考文献 / 公式 | — |
| `02-autoformat.json` | 化学式 H2O、单位指数 m²、上下标 `^/_` 标记 | — |
| `03-references.json` | 参考文献规范化 + 上标 REF 域交叉引用 | **`--normalize-references`** |
| `04-vertical-pagenum.json` | 横向页 + 左侧竖排页码 (VML 双轨) | — |
| `05-omml-equation.json` | 4 个真 OMML 公式(上标/下标/分数/嵌套) | — |
| `06-figure.json` | 图片插入(用 `pixel.png` 当素材) | — |
| `07-thesis-preset.json` | **毕业论文一键预设**(奇偶页眉 + 字号字距 + 缩进规范) | — |
| `08-template-apply.json` | 模板化样式应用:`format.styleId` 引用模板里的样式 | **`--template examples\template.docx`** |
| `09-firstline-scale-test.json` | "首行缩进 2 字符"随字号伸缩的对照 | — |

## 怎么跑

### 最简单的(从零生成):

```powershell
$engine = ".\engine\docx-auto-template-engine.exe"
& $engine build --plan examples\01-basic.json --output out-01.docx
```

### 三种带参的:

```powershell
# 03 — 参考文献,要 --normalize-references
& $engine build `
    --plan examples\03-references.json `
    --output out-03.docx `
    --normalize-references

# 08 — 模板化样式,要 --template(指向模板 docx)
& $engine build `
    --plan examples\08-template-apply.json `
    --template examples\template.docx `
    --output out-08.docx

# 重排现有文档,要 --source
& $engine build `
    --plan plan.json `
    --source existing.docx `
    --output reformatted.docx
```

### 一键跑全部:

```powershell
foreach ($f in Get-ChildItem examples\0*.json) {
    $name = $f.BaseName
    $extra = @()
    if ($name -eq "03-references") { $extra = @("--normalize-references") }
    if ($name -eq "08-template-apply") { $extra = @("--template", "examples\template.docx") }
    & $engine build --plan $f.FullName --output "out-$name.docx" @extra
    Write-Host "OK $name"
}
```

Linux/macOS bash:

```bash
engine=./engine/docx-auto-template-engine
for f in examples/0*.json; do
  name=$(basename "$f" .json)
  extra=()
  [ "$name" = "03-references" ] && extra=(--normalize-references)
  [ "$name" = "08-template-apply" ] && extra=(--template examples/template.docx)
  "$engine" build --plan "$f" --output "out-$name.docx" "${extra[@]}"
  echo "OK $name"
done
```

## 命令面(全)

```
analyze --input <docx> --output <analysis.json>
build --plan <format-plan.json> --output <result.docx>
      [--source <docx>] [--template <docx>] [--normalize-references]
```

- `analyze`:读 docx,解析有效样式,输出结构 + 每块的 path / text / heading-level / 字体字号 / per-run 数组。
- `build`:FormatPlan → docx。有 `--source` 即按 `block.ref` 重排;无即从零生成。
- `--template <docx>`:复制模板的 `word/styles.xml`,让 plan 的 `format.styleId` 能解析。
- `--normalize-references`:reference 块加书签 + 正文 `[n]` 上标转 REF 复杂字段。

## 关于公式

公式必须以 OMML XML 写在 `equation.xml` 字段。引擎原样嵌入,Word/WPS
按数学排版渲染。**写法见 [`OMML-CHEATSHEET.md`](OMML-CHEATSHEET.md)**——
里面有每个 `<m:r>` 必带的 Cambria Math 字体提示这条死活差异。

`05-omml-equation.json` 内 4 个公式(`E=mc²` / 函数下标 / 分数 / 嵌套)可
直接对照学。

## 反馈问题

如果某个样例产出的 docx 在 Word 里看着不对:

- **优先发产物 docx 文件本身**(不是截图),社区/维护者看 XML 比看截图准 10 倍
- 同时报你打开用的是 Word / WPS / LibreOffice、版本号
