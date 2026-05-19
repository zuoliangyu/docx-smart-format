# 横向页面 + 竖向页码文本框

> **契约说明**：本文档展示引擎内部 RenderSpec / OOXML 结构以解释竖排页码原理。LLM 侧请用 [FormatPlan](format-plan-schema.md)（横向节用 `sections[].orientation: "landscape"`），引擎自动降级生成下述 `wps:wsp` + VML 双轨结构。

## 适用场景

当某一节是横向页面（landscape），且页码需要竖向排布在页面左侧（装订边方向）时，调用页眉/页脚的 `textBoxes` 能力：

- 含横向插页（宽表、宽图、流程图、大版面附图）的论文 / 报告
- 横向页与正文页在同一文档内交替出现，需要保持页码可读
- 用户要求"横向页页码竖向放在左侧"、"侧边页码"、"装订边页码"

## 实现方式

1. 在节定义中将 `orientation` 设为 `landscape`，互换 `pageWidth` 与 `pageHeight`。
2. 在该节绑定的页脚（或页眉）`RenderHeaderFooterSpec` 上，填入 `textBoxes` 数组。每个文本框：
   - 用 `posXEmu` / `posYEmu` 指定相对页面（默认 `relativeFromH=page`, `relativeFromV=page`）的偏移。
   - 用 `widthEmu` / `heightEmu` 控制竖条尺寸。
   - 用 `textDirection: "vert270"` 表示文字旋转 270°（横向页面 CCW 旋转后可正常阅读）。
   - 用 `paragraphs[*].pageNumberField: true` 写入 `PAGE` 字段。
   - 用 `borderStyle: "none"` 与 `fillColor: "none"` 隐藏边框与底色。
3. 引擎会在 `<w:ftr>` 中生成 `<mc:AlternateContent>`：`mc:Choice Requires="wps"` 走现代 `wps:wsp`，`mc:Fallback` 走 VML `<v:rect><v:textbox>`，老版本 Word / WPS 也能渲染。
4. 页脚自身的 `paragraphs` 可以留一个空段（或保留居中页码用于纵向页），文本框始终额外绘制。

## 默认值（A4 横向、文本框贴左边 1/2 英寸）

| 字段 | 默认值（EMU） | 说明 |
|------|--------------|------|
| `posXEmu` | `457200` | 距页面左边约 0.5 英寸 |
| `posYEmu` | `1828800` | 距页面顶部约 2 英寸（垂直居中视觉） |
| `widthEmu` | `457200` | 文本框宽约 0.5 英寸 |
| `heightEmu` | `7772400` | 文本框高约 8.5 英寸（A4 横向高度） |
| `textDirection` | `vert270` | 文字反向 270° 旋转，CCW 翻页后正读 |
| `anchor` | `ctr` | 文本框内文字居中 |

## RenderSpec 示例

`footers[*].textBoxes`：

```json
{
  "type": "default",
  "paragraphs": [
    { "text": "", "paragraph": { "align": "center" } }
  ],
  "textBoxes": [
    {
      "posXEmu": 457200,
      "posYEmu": 1828800,
      "widthEmu": 457200,
      "heightEmu": 7772400,
      "relativeFromH": "page",
      "relativeFromV": "page",
      "textDirection": "vert270",
      "borderStyle": "none",
      "fillColor": "none",
      "anchor": "ctr",
      "name": "LandscapePageNumber",
      "paragraphs": [
        {
          "align": "center",
          "pageNumberField": true,
          "run": {
            "eastAsiaFont": "宋体",
            "asciiFont": "Times New Roman",
            "fontSize": "24"
          }
        }
      ]
    }
  ]
}
```

## 生成的 OOXML 结构示意（节选）

```xml
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p>
    <w:r>
      <mc:AlternateContent
          xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
          xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
          xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
          xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
        <mc:Choice Requires="wps">
          <w:drawing>
            <wp:anchor distT="0" distB="0" distL="114300" distR="114300"
                       simplePos="0" relativeHeight="251659264"
                       behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">
              <wp:simplePos x="0" y="0"/>
              <wp:positionH relativeFrom="page"><wp:posOffset>457200</wp:posOffset></wp:positionH>
              <wp:positionV relativeFrom="page"><wp:posOffset>1828800</wp:posOffset></wp:positionV>
              <wp:extent cx="457200" cy="7772400"/>
              <wp:effectExtent l="0" t="0" r="0" b="0"/>
              <wp:wrapNone/>
              <wp:docPr id="1" name="LandscapePageNumber"/>
              <wp:cNvGraphicFramePr/>
              <a:graphic>
                <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
                  <wps:wsp>
                    <wps:cNvSpPr txBox="1"/>
                    <wps:spPr>
                      <a:xfrm>
                        <a:off x="0" y="0"/>
                        <a:ext cx="457200" cy="7772400"/>
                      </a:xfrm>
                      <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                      <a:noFill/>
                      <a:ln><a:noFill/></a:ln>
                    </wps:spPr>
                    <wps:txbx>
                      <w:txbxContent>
                        <w:p>
                          <w:pPr><w:jc w:val="center"/></w:pPr>
                          <w:r>
                            <w:fldChar w:fldCharType="begin"/>
                          </w:r>
                          <w:r>
                            <w:instrText xml:space="preserve"> PAGE </w:instrText>
                          </w:r>
                          <w:r>
                            <w:fldChar w:fldCharType="end"/>
                          </w:r>
                        </w:p>
                      </w:txbxContent>
                    </wps:txbx>
                    <wps:bodyPr rot="0" spcFirstLastPara="0"
                                vertOverflow="visible" horzOverflow="visible"
                                vert="vert270" wrap="square"
                                lIns="91440" tIns="45720"
                                rIns="91440" bIns="45720"
                                numCol="1" spcCol="0" rtlCol="0"
                                fromWordArt="0" anchor="ctr" anchorCtr="0"
                                forceAA="0" compatLnSpc="1"/>
                  </wps:wsp>
                </a:graphicData>
              </a:graphic>
            </wp:anchor>
          </w:drawing>
        </mc:Choice>
        <mc:Fallback>
          <w:pict xmlns:v="urn:schemas-microsoft-com:vml"
                  xmlns:w10="urn:schemas-microsoft-com:office:word">
            <v:rect id="_x0000_s1026"
                    style="position:absolute;margin-left:36pt;margin-top:144pt;
                           width:36pt;height:612pt;z-index:251659264;
                           mso-position-horizontal-relative:page;
                           mso-position-vertical-relative:page"
                    stroked="f" filled="f">
              <v:textbox style="layout-flow:vertical;mso-layout-flow-alt:bottom-to-top"
                         inset="7.2pt,3.6pt,7.2pt,3.6pt">
                <w:txbxContent>
                  <w:p>
                    <w:pPr><w:jc w:val="center"/></w:pPr>
                    <w:r>
                      <w:fldChar w:fldCharType="begin"/>
                    </w:r>
                    <w:r>
                      <w:instrText xml:space="preserve"> PAGE </w:instrText>
                    </w:r>
                    <w:r>
                      <w:fldChar w:fldCharType="end"/>
                    </w:r>
                  </w:p>
                </w:txbxContent>
              </v:textbox>
            </v:rect>
          </w:pict>
        </mc:Fallback>
      </mc:AlternateContent>
    </w:r>
  </w:p>
</w:ftr>
```

## 关键约定

- `vert="vert270"` 是横向页面左侧竖排页码的标配，文字从下到上读，把页面 CCW 翻转后等同于正常正文方向。
- 必须同时输出 `mc:Choice (wps)` 与 `mc:Fallback (VML)`：早期 Word / WPS / 部分 Mac Word 不支持 wps 命名空间，需要 VML 兜底。
- 文本框位置必须相对页面 (`relativeFrom="page"`)，而不是相对页边距，否则横纵向切换时定位会漂移。
- 页脚自身仍可以放一个普通的居中页码段落用于纵向页，横向页通过节绑定不同的 footer 引用（`footerType: "default"` 或独立 footer 部件）切换为带 textBox 的版本。
- 不要把文本框做成 inline drawing：anchored + wrapNone 才能脱离页脚高度的影响。
- 不要用空白字符 + 旋转字体伪造竖向页码，必须走真正的 `wps:wsp` / VML `v:rect` 文本框。
