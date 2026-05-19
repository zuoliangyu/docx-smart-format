using System.Globalization;

namespace DocxWeb;

// Lowers the unified FormatPlan contract into the internal RenderSpec IR.
// DocxRenderer is unchanged: the golden baseline still gauges rendering.
//
//   overlay  block (has Ref + source): reuse source content, apply format.
//   generate block (no Ref):           author content from role + text.
internal static class FormatPlanCompiler
{
    public static RenderSpec Compile(FormatPlan plan, AnalysisReport? source = null)
    {
        var spec = new RenderSpec
        {
            Document = BuildDocument(plan.Document)
        };

        var sourceById = BuildSourceIndex(source);
        var sectionsByKey = plan.Sections.ToDictionary(s => s.Key, StringComparer.Ordinal);

        if (NeedsPageNumberFooter(plan.Document.HeaderFooter))
        {
            spec.Footers.Add(new RenderHeaderFooterSpec
            {
                Type = "default",
                Kind = "footer",
                Paragraphs =
                [
                    new RenderParagraphSpec
                    {
                        Align = "center",
                        PageNumberField = true
                    }
                ]
            });
        }

        string? currentSectionKey = null;
        var first = true;
        foreach (var block in plan.Blocks)
        {
            if (block.SectionKey != null && (first || block.SectionKey != currentSectionKey))
            {
                if (sectionsByKey.TryGetValue(block.SectionKey, out var planSection))
                    spec.Body.Add(new RenderBodyBlock
                    {
                        Kind = "section",
                        Section = BuildSection(planSection)
                    });
                currentSectionKey = block.SectionKey;
            }

            first = false;
            EmitBlock(spec, plan, block, sourceById);
        }

        return spec;
    }

    private static void EmitBlock(
        RenderSpec spec,
        FormatPlan plan,
        PlanBlock block,
        IReadOnlyDictionary<string, BodyBlockSummary> sourceById)
    {
        BodyBlockSummary? source = null;
        if (!string.IsNullOrWhiteSpace(block.Ref))
            sourceById.TryGetValue(block.Ref!, out source);

        var role = (block.Role ?? "body").Trim().ToLowerInvariant();

        // Object roles (generate-only; overlay objects fall through to source kind).
        if (source == null)
        {
            switch (role)
            {
                case "figure":
                    if (block.Image != null)
                    {
                        spec.Body.Add(new RenderBodyBlock
                        {
                            Kind = "image",
                            Image = new RenderImageSpec
                            {
                                Path = block.Image.Path,
                                ContentType = block.Image.ContentType,
                                WidthEmu = block.Image.WidthEmu,
                                HeightEmu = block.Image.HeightEmu,
                                AltText = block.Image.AltText
                            }
                        });
                    }
                    AddCaption(spec, plan, block);
                    return;
                case "table":
                    spec.Body.Add(new RenderBodyBlock
                    {
                        Kind = "table",
                        Table = new RenderTableSpec
                        {
                            Rows = block.Table?.Rows ?? []
                        }
                    });
                    AddCaption(spec, plan, block);
                    return;
                case "equation":
                    spec.Body.Add(new RenderBodyBlock
                    {
                        Kind = "equation",
                        Equation = new RenderEquationSpec
                        {
                            Text = block.Equation?.Text,
                            Xml = block.Equation?.Xml ?? "",
                            DisplayMode = block.Equation?.DisplayMode ?? "inline",
                            Run = DocumentRun(plan.Document)
                        }
                    });
                    return;
            }
        }

        // Overlay objects: preserve source object structure, ignore plan text.
        if (source is { HasTable: true })
        {
            spec.Body.Add(new RenderBodyBlock { Kind = "table", Table = new RenderTableSpec { Rows = source.TableRows } });
            return;
        }
        if (source is { HasEquation: true } && source.Equation != null)
        {
            spec.Body.Add(new RenderBodyBlock
            {
                Kind = "equation",
                Equation = new RenderEquationSpec
                {
                    Text = source.Equation.Text,
                    Xml = source.Equation.Xml,
                    DisplayMode = source.Equation.DisplayMode,
                    Run = DocumentRun(plan.Document)
                }
            });
            return;
        }

        var paragraph = new RenderParagraphSpec();
        var kind = "paragraph";

        if (source != null)
        {
            paragraph.Text = source.Text;
            paragraph.HeadingLevel = source.HeadingLevel;
            paragraph.Runs = source.Runs;
            if (source.HeadingLevel.HasValue) kind = "heading";
            if (string.Equals(source.Kind, "reference", StringComparison.OrdinalIgnoreCase)) kind = "reference";
        }
        else
        {
            switch (role)
            {
                case "heading1": paragraph.HeadingLevel = 1; kind = "heading"; break;
                case "heading2": paragraph.HeadingLevel = 2; kind = "heading"; break;
                case "heading3": paragraph.HeadingLevel = 3; kind = "heading"; break;
                case "reference": kind = "reference"; break;
                case "pagenumber": paragraph.PageNumberField = true; break;
                case "caption": paragraph.Text = block.Caption ?? block.Text; break;
            }
            if (paragraph.Text == null && role != "pagenumber")
                paragraph.Text = block.Text;
        }

        ApplyDocumentDefaults(paragraph, plan.Document);
        ApplyFormat(paragraph, block.Format);

        spec.Body.Add(new RenderBodyBlock { Kind = kind, Paragraph = paragraph });

        if (role is "figure" or "table")
            AddCaption(spec, plan, block);
    }

    private static void AddCaption(RenderSpec spec, FormatPlan plan, PlanBlock block)
    {
        if (string.IsNullOrWhiteSpace(block.Caption))
            return;

        var caption = new RenderParagraphSpec { Text = block.Caption, Align = "center" };
        ApplyDocumentDefaults(caption, plan.Document);
        spec.Body.Add(new RenderBodyBlock { Kind = "paragraph", Paragraph = caption });
    }

    private static RenderDocumentSpec BuildDocument(PlanDocument doc)
    {
        var (width, height) = PageSizeTwips(doc.PageSize);
        return new RenderDocumentSpec
        {
            Title = doc.Title,
            Author = doc.Author,
            PageWidth = width,
            PageHeight = height,
            Orientation = doc.Orientation,
            MarginTop = doc.Margins?.Top,
            MarginBottom = doc.Margins?.Bottom,
            MarginLeft = doc.Margins?.Left,
            MarginRight = doc.Margins?.Right
        };
    }

    private static RenderSectionSpec BuildSection(PlanSection s)
    {
        return new RenderSectionSpec
        {
            Type = s.Type,
            Orientation = s.Orientation,
            MarginTop = s.Margins?.Top,
            MarginBottom = s.Margins?.Bottom,
            MarginLeft = s.Margins?.Left,
            MarginRight = s.Margins?.Right,
            PageStart = s.PageStart,
            PageNumFmt = s.PageNumFmt,
            TitlePage = s.TitlePage
        };
    }

    private static void ApplyDocumentDefaults(RenderParagraphSpec p, PlanDocument doc)
    {
        if (!string.IsNullOrWhiteSpace(doc.EnFont))
            p.Run.AsciiFont ??= doc.EnFont;
        if (!string.IsNullOrWhiteSpace(doc.CnFont))
            p.Run.EastAsiaFont ??= doc.CnFont;
        if (doc.BaseFontPt.HasValue && string.IsNullOrWhiteSpace(p.Run.FontSize))
            p.Run.FontSize = HalfPoint(doc.BaseFontPt.Value);
        if (doc.LineSpacing.HasValue && string.IsNullOrWhiteSpace(p.Paragraph.LineSpacing))
        {
            p.Paragraph.LineSpacing = ((int)Math.Round(240 * doc.LineSpacing.Value)).ToString(CultureInfo.InvariantCulture);
            p.Paragraph.LineSpacingRule = "auto";
        }
    }

    private static void ApplyFormat(RenderParagraphSpec p, PlanFormat? f)
    {
        if (f == null) return;

        if (f.Bold.HasValue) p.Run.Bold = f.Bold.Value;
        if (f.Italic.HasValue) p.Run.Italic = f.Italic.Value;
        if (f.Underline != null) p.Run.Underline = f.Underline;
        if (f.FontColor != null) p.Run.FontColor = f.FontColor;
        if (f.Highlight != null) p.Run.Highlight = f.Highlight;
        if (f.VerticalAlign != null) p.Run.VerticalAlign = f.VerticalAlign;
        if (f.EnFont != null) p.Run.AsciiFont = f.EnFont;
        if (f.CnFont != null) p.Run.EastAsiaFont = f.CnFont;
        if (f.FontPt.HasValue) p.Run.FontSize = HalfPoint(f.FontPt.Value);

        if (f.Align != null) p.Align = f.Align;
        if (f.FirstLineIndent != null) p.Paragraph.FirstLineIndent = f.FirstLineIndent;
        if (f.HangingIndent != null) p.Paragraph.HangingIndent = f.HangingIndent;
        if (f.LineSpacing != null) { p.Paragraph.LineSpacing = f.LineSpacing; p.Paragraph.LineSpacingRule = "auto"; }
        if (f.BeforeSpacing != null) p.Paragraph.BeforeSpacing = f.BeforeSpacing;
        if (f.AfterSpacing != null) p.Paragraph.AfterSpacing = f.AfterSpacing;
        if (f.PageBreakBefore.HasValue) p.Paragraph.PageBreakBefore = f.PageBreakBefore.Value;
    }

    private static RunFormatProfile DocumentRun(PlanDocument doc)
    {
        return new RunFormatProfile
        {
            AsciiFont = doc.EnFont,
            EastAsiaFont = doc.CnFont,
            FontSize = doc.BaseFontPt.HasValue ? HalfPoint(doc.BaseFontPt.Value) : null
        };
    }

    private static IReadOnlyDictionary<string, BodyBlockSummary> BuildSourceIndex(AnalysisReport? source)
    {
        var map = new Dictionary<string, BodyBlockSummary>(StringComparer.Ordinal);
        if (source == null) return map;
        foreach (var b in source.Body)
            if (!string.IsNullOrEmpty(b.Path))
                map[b.Path] = b;
        return map;
    }

    private static bool NeedsPageNumberFooter(PlanHeaderFooter? hf)
    {
        var v = hf?.PageNumber?.Trim().ToLowerInvariant();
        return v is "continuous" or "center-page-number";
    }

    private static string HalfPoint(double pt)
        => ((int)Math.Round(pt * 2)).ToString(CultureInfo.InvariantCulture);

    private static (string width, string height) PageSizeTwips(string? size)
    {
        return (size?.Trim().ToLowerInvariant()) switch
        {
            "letter" => ("12240", "15840"),
            "legal" => ("12240", "20160"),
            "a5" => ("8391", "11906"),
            _ => ("11906", "16838") // a4 default
        };
    }
}
