using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Wordprocessing;
using System.Xml.Linq;
using System.Text.RegularExpressions;
using A = DocumentFormat.OpenXml.Drawing;
using Dw = DocumentFormat.OpenXml.Drawing.Wordprocessing;
using Pic = DocumentFormat.OpenXml.Drawing.Pictures;

namespace DocxWeb;

// Block-level builders: paragraph, image, equation, table.
internal static partial class DocxRenderer
{
    private static Paragraph BuildParagraph(RenderParagraphSpec? spec)
    {
        spec ??= new RenderParagraphSpec();

        var paragraph = new Paragraph();
        var properties = new ParagraphProperties();
        var headingLevel = spec.HeadingLevel;
        var styleId = spec.Style;
        var paragraphFormat = spec.Paragraph ?? new ParagraphFormatProfile();
        var align = spec.Align ?? paragraphFormat.Align;
        var defaultRunFormat = GetDefaultRunFormat(headingLevel, spec.Run);

        if (string.IsNullOrWhiteSpace(styleId) && headingLevel.HasValue)
            styleId = $"Heading{headingLevel.Value}";

        if (!string.IsNullOrWhiteSpace(styleId))
            properties.Append(new ParagraphStyleId { Val = styleId });

        if (headingLevel.HasValue)
            properties.Append(new OutlineLevel { Val = headingLevel.Value - 1 });

        var justification = ParseAlignment(align);
        if (justification.HasValue)
            properties.Append(new Justification { Val = justification.Value });

        AppendIndentation(properties, paragraphFormat);
        AppendSpacing(properties, paragraphFormat);
        AppendTabStops(properties, paragraphFormat);

        if (spec.PageBreakBefore || paragraphFormat.PageBreakBefore)
            properties.Append(new PageBreakBefore());

        if (properties.ChildElements.Count > 0)
            paragraph.Append(properties);

        AppendContentRuns(paragraph, spec, headingLevel);

        if (spec.PageNumberField)
        {
            if (!string.IsNullOrWhiteSpace(spec.PageNumberPrefix))
                paragraph.Append(BuildTextRun(spec.PageNumberPrefix, defaultRunFormat, headingLevel));

            paragraph.Append(BuildField(" PAGE ", defaultRunFormat, headingLevel));

            if (!string.IsNullOrWhiteSpace(spec.PageNumberSuffix))
                paragraph.Append(BuildTextRun(spec.PageNumberSuffix, defaultRunFormat, headingLevel));
        }

        if (spec.NumPagesField)
        {
            if (!string.IsNullOrEmpty(spec.Text) || spec.PageNumberField)
                paragraph.Append(BuildTextRun(" / ", defaultRunFormat, headingLevel));

            paragraph.Append(BuildField(" NUMPAGES ", defaultRunFormat, headingLevel));
        }

        if (!paragraph.Elements<Run>().Any() && !paragraph.Elements<SimpleField>().Any())
            paragraph.Append(new Run());

        return paragraph;
    }

    private static Paragraph? BuildImageParagraph(MainDocumentPart mainPart, RenderImageSpec? spec)
    {
        if (spec == null)
            return null;

        var imagePath = spec.Path;
        if (string.IsNullOrWhiteSpace(imagePath) || !File.Exists(imagePath))
            return null;

        var imagePartType = GetImagePartType(imagePath, spec.ContentType);
        var imagePart = mainPart.AddImagePart(imagePartType);
        using (var stream = File.OpenRead(imagePath))
        {
            imagePart.FeedData(stream);
        }

        var relationshipId = mainPart.GetIdOfPart(imagePart);
        var width = spec.WidthEmu.GetValueOrDefault(4_000_000L);
        var height = spec.HeightEmu.GetValueOrDefault(3_000_000L);
        var name = Path.GetFileName(imagePath);
        var altText = spec.AltText ?? name;

        var element =
            new Drawing(
                new Dw.Inline(
                    new Dw.Extent { Cx = width, Cy = height },
                    new Dw.EffectExtent
                    {
                        LeftEdge = 0L,
                        TopEdge = 0L,
                        RightEdge = 0L,
                        BottomEdge = 0L
                    },
                    new Dw.DocProperties
                    {
                        Id = (UInt32Value)1U,
                        Name = name,
                        Description = altText
                    },
                    new Dw.NonVisualGraphicFrameDrawingProperties(
                        new A.GraphicFrameLocks { NoChangeAspect = true }),
                    new A.Graphic(
                        new A.GraphicData(
                            new Pic.Picture(
                                new Pic.NonVisualPictureProperties(
                                    new Pic.NonVisualDrawingProperties
                                    {
                                        Id = (UInt32Value)0U,
                                        Name = name,
                                        Description = altText
                                    },
                                    new Pic.NonVisualPictureDrawingProperties()),
                                new Pic.BlipFill(
                                    new A.Blip { Embed = relationshipId },
                                    new A.Stretch(new A.FillRectangle())),
                                new Pic.ShapeProperties(
                                    new A.Transform2D(
                                        new A.Offset { X = 0L, Y = 0L },
                                        new A.Extents { Cx = width, Cy = height }),
                                    new A.PresetGeometry(new A.AdjustValueList())
                                    {
                                        Preset = A.ShapeTypeValues.Rectangle
                                    })))
                        {
                            Uri = "http://schemas.openxmlformats.org/drawingml/2006/picture"
                        }))
                {
                    DistanceFromTop = 0U,
                    DistanceFromBottom = 0U,
                    DistanceFromLeft = 0U,
                    DistanceFromRight = 0U
                });

        return new Paragraph(new Run(element));
    }

    private static Paragraph? BuildEquationParagraph(RenderEquationSpec? spec)
    {
        if (spec == null)
            return null;

        if (string.IsNullOrWhiteSpace(spec.Xml) && !string.IsNullOrWhiteSpace(spec.Text))
        {
            var paragraph = new Paragraph();
            ApplyEquationParagraphProperties(paragraph, spec);

            AppendFormattedText(paragraph, spec.Text, GetDefaultEquationRunFormat(spec.Run), null);
            return paragraph;
        }

        try
        {
            var paragraph = new Paragraph();
            ApplyEquationParagraphProperties(paragraph, spec);

            var sanitizedXml = SanitizeEquationXml(spec.Xml);
            if (sanitizedXml.Contains("oMathPara", StringComparison.OrdinalIgnoreCase))
            {
                paragraph.Append(new DocumentFormat.OpenXml.Math.Paragraph(sanitizedXml));
            }
            else
            {
                paragraph.Append(new DocumentFormat.OpenXml.Math.OfficeMath(sanitizedXml));
            }
            return paragraph;
        }
        catch
        {
            var paragraph = new Paragraph();
            ApplyEquationParagraphProperties(paragraph, spec);

            var label = string.Equals(spec.DisplayMode, "display", StringComparison.OrdinalIgnoreCase)
                ? "[Equation]"
                : "[Inline Equation]";
            var text = string.IsNullOrWhiteSpace(spec.Text) ? label : spec.Text;
            AppendFormattedText(paragraph, text, GetDefaultEquationRunFormat(spec.Run), null);
            return paragraph;
        }
    }

    private static Table BuildTable(RenderTableSpec? spec)
    {
        spec ??= new RenderTableSpec();
        var rows = spec.Rows.ToList();

        var table = new Table();
        table.AppendChild(new TableProperties(
            new TableWidth { Width = "5000", Type = TableWidthUnitValues.Pct },
            new TableJustification { Val = TableRowAlignmentValues.Center },
            new TableBorders(
                new TopBorder { Val = BorderValues.Single, Size = 12U },
                new BottomBorder { Val = BorderValues.Single, Size = 12U },
                new LeftBorder { Val = BorderValues.None, Size = 0U },
                new RightBorder { Val = BorderValues.None, Size = 0U },
                new InsideHorizontalBorder { Val = BorderValues.None, Size = 0U },
                new InsideVerticalBorder { Val = BorderValues.None, Size = 0U })));

        for (var rowIndex = 0; rowIndex < rows.Count; rowIndex++)
        {
            var isHeaderRow = rowIndex == 0 && rows.Count > 1;
            var tableRow = new TableRow();

            foreach (var cellText in rows[rowIndex])
            {
                var cellParagraph = new Paragraph();
                var cellFormat = isHeaderRow ? new RunFormatProfile { Bold = true } : null;
                AppendFormattedText(cellParagraph, cellText ?? string.Empty, cellFormat, null);
                if (!cellParagraph.Elements<Run>().Any())
                    cellParagraph.Append(new Run());

                var cellProperties = new TableCellProperties(
                    new TableCellWidth { Type = TableWidthUnitValues.Auto });

                if (isHeaderRow)
                    cellProperties.Append(new TableCellBorders(
                        new BottomBorder { Val = BorderValues.Single, Size = 6U }));

                var cell = new TableCell(cellProperties, cellParagraph);
                tableRow.Append(cell);
            }

            table.Append(tableRow);
        }

        if (rows.Count == 0)
        {
            table.Append(new TableRow(
                new TableCell(
                    new TableCellProperties(new TableCellWidth { Type = TableWidthUnitValues.Auto }),
                    new Paragraph(new Run(new Text(string.Empty))))));
        }

        return table;
    }

    private static void ApplyEquationParagraphProperties(Paragraph paragraph, RenderEquationSpec spec)
    {
        var properties = new ParagraphProperties();
        var paragraphFormat = spec.Paragraph ?? new ParagraphFormatProfile();
        var align = string.Equals(spec.DisplayMode, "display", StringComparison.OrdinalIgnoreCase)
            ? "center"
            : paragraphFormat.Align;

        var justification = ParseAlignment(align);
        if (justification.HasValue)
            properties.Append(new Justification { Val = justification.Value });

        AppendIndentation(properties, paragraphFormat);

        // OMML 公式高度由内容决定，不能用 exact 行高，否则高公式（分式、积分等）被截断
        var isOmml = !string.IsNullOrWhiteSpace(spec.Xml);
        var spacingFormat = isOmml
            ? new ParagraphFormatProfile
              {
                  BeforeSpacing = paragraphFormat.BeforeSpacing,
                  AfterSpacing = paragraphFormat.AfterSpacing
              }
            : paragraphFormat;
        AppendSpacing(properties, spacingFormat);

        if (properties.ChildElements.Count > 0)
            paragraph.Append(properties);
    }

    private static string SanitizeEquationXml(string xml)
    {
        if (string.IsNullOrWhiteSpace(xml))
            return xml;

        try
        {
            var element = XElement.Parse(xml, LoadOptions.PreserveWhitespace);
            foreach (var runProperties in element.Descendants().Where(x => x.Name.LocalName == "rPr"))
            {
                runProperties.Elements()
                    .Where(x => x.Name.LocalName is "color" or "highlight" or "shd")
                    .Remove();
            }

            return element.ToString(SaveOptions.DisableFormatting);
        }
        catch
        {
            return xml;
        }
    }

    private static PartTypeInfo GetImagePartType(string path, string? contentType)
    {
        return (contentType?.ToLowerInvariant(), Path.GetExtension(path).ToLowerInvariant()) switch
        {
            ("image/png", _) or (_, ".png") => ImagePartType.Png,
            ("image/jpeg", _) or ("image/jpg", _) or (_, ".jpg") or (_, ".jpeg") => ImagePartType.Jpeg,
            ("image/gif", _) or (_, ".gif") => ImagePartType.Gif,
            ("image/bmp", _) or (_, ".bmp") => ImagePartType.Bmp,
            ("image/tiff", _) or (_, ".tif") or (_, ".tiff") => ImagePartType.Tiff,
            ("image/x-emf", _) or (_, ".emf") => ImagePartType.Emf,
            ("image/x-wmf", _) or (_, ".wmf") => ImagePartType.Wmf,
            _ => ImagePartType.Png
        };
    }
}
