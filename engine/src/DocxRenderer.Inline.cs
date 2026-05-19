using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Wordprocessing;
using System.Xml.Linq;
using System.Text.RegularExpressions;
using A = DocumentFormat.OpenXml.Drawing;
using Dw = DocumentFormat.OpenXml.Drawing.Wordprocessing;
using Pic = DocumentFormat.OpenXml.Drawing.Pictures;

namespace DocxWeb;

// Run-level rendering: content runs, run properties, fonts, indentation,
// tab stops, spacing, fields, and run-format defaults.
internal static partial class DocxRenderer
{
    private static void AppendContentRuns(Paragraph paragraph, RenderParagraphSpec spec, int? headingLevel)
    {
        if (spec.Runs.Count > 0)
        {
            foreach (var run in spec.Runs)
            {
                var kind = run.Kind?.ToLowerInvariant();
                switch (kind)
                {
                    case "bookmarkstart":
                        if (!string.IsNullOrWhiteSpace(run.BookmarkId) && !string.IsNullOrWhiteSpace(run.BookmarkName))
                        {
                            paragraph.Append(new BookmarkStart
                            {
                                Id = run.BookmarkId,
                                Name = run.BookmarkName
                            });
                        }
                        continue;
                    case "bookmarkend":
                        if (!string.IsNullOrWhiteSpace(run.BookmarkId))
                            paragraph.Append(new BookmarkEnd { Id = run.BookmarkId });
                        continue;
                    case "tab":
                    {
                        var tabRun = new Run();
                        var tabProps = BuildRunProperties(run.Format, headingLevel);
                        if (tabProps.ChildElements.Count > 0)
                            tabRun.Append(tabProps);
                        tabRun.Append(new TabChar());
                        paragraph.Append(tabRun);
                        continue;
                    }
                    case "reffield":
                    {
                        var field = new SimpleField { Instruction = run.FieldInstruction ?? string.Empty };
                        var fieldRun = new Run();
                        var fieldRunProps = BuildRunProperties(run.Format, headingLevel);
                        if (fieldRunProps.ChildElements.Count > 0)
                            fieldRun.Append(fieldRunProps);
                        AppendRunTextContent(fieldRun, run.FieldDisplayText ?? string.Empty);
                        field.Append(fieldRun);
                        paragraph.Append(field);
                        continue;
                    }
                }

                if (run.Segments.Count > 0)
                {
                    foreach (var segment in run.Segments)
                    {
                        if (string.IsNullOrEmpty(segment.Text))
                            continue;

                        AppendFormattedText(paragraph, segment.Text, segment.Format, headingLevel);
                    }
                    continue;
                }

                if (!string.IsNullOrEmpty(run.Text))
                    AppendFormattedText(paragraph, run.Text, run.Format, headingLevel);
            }

            return;
        }

        if (!string.IsNullOrEmpty(spec.Text))
            AppendFormattedText(paragraph, spec.Text, spec.Run, headingLevel);
    }

    private static Run BuildTextRun(string text, RunFormatProfile? format, int? headingLevel)
    {
        var run = new Run();
        var properties = BuildRunProperties(format, headingLevel);

        if (properties.ChildElements.Count > 0)
            run.Append(properties);

        AppendRunTextContent(run, text);
        return run;
    }

    private static void AppendFormattedText(Paragraph paragraph, string text, RunFormatProfile? format, int? headingLevel)
    {
        foreach (var segment in SplitAutoFormatSegments(text, format))
            paragraph.Append(BuildTextRun(segment.Text, segment.Format, headingLevel));
    }

    private static void AppendRunTextContent(Run run, string text)
    {
        var parts = text.Split('\t');
        for (var i = 0; i < parts.Length; i++)
        {
            if (parts[i].Length > 0)
                run.Append(new Text(parts[i]) { Space = SpaceProcessingModeValues.Preserve });

            if (i < parts.Length - 1)
                run.Append(new TabChar());
        }
    }

    private static RunProperties BuildRunProperties(RunFormatProfile? format, int? headingLevel)
    {
        format = GetDefaultRunFormat(headingLevel, format);
        var properties = new RunProperties();
        var asciiFont = NormalizeAsciiFont(format.AsciiFont);
        var eastAsiaFont = NormalizeEastAsiaFont(format.EastAsiaFont);

        if (!string.IsNullOrWhiteSpace(asciiFont) || !string.IsNullOrWhiteSpace(eastAsiaFont))
        {
            properties.Append(new RunFonts
            {
                Ascii = asciiFont,
                HighAnsi = asciiFont,
                EastAsia = eastAsiaFont,
                ComplexScript = eastAsiaFont ?? asciiFont
            });
        }

        if (format.Bold || headingLevel.HasValue)
            properties.Append(new Bold());

        if (format.Italic)
            properties.Append(new Italic());

        if (!string.IsNullOrWhiteSpace(format.Underline)
            && TryParseUnderline(format.Underline, out var underline))
        {
            properties.Append(new Underline { Val = underline });
        }

        if (!string.IsNullOrWhiteSpace(format.FontColor))
            properties.Append(new Color { Val = format.FontColor });

        if (!string.IsNullOrWhiteSpace(format.Highlight)
            && TryParseHighlight(format.Highlight, out var highlight))
        {
            properties.Append(new Highlight { Val = highlight });
        }

        if (TryParseVerticalAlign(format.VerticalAlign, out var verticalAlign))
            properties.Append(new VerticalTextAlignment { Val = verticalAlign });

        var fontSize = format.FontSize ?? GetFallbackFontSize(headingLevel);
        if (!string.IsNullOrWhiteSpace(fontSize))
        {
            properties.Append(new FontSize { Val = fontSize });
            properties.Append(new FontSizeComplexScript { Val = fontSize });
        }

        return properties;
    }

    private static string GetFallbackFontSize(int? headingLevel)
    {
        return headingLevel switch
        {
            1 => "36",
            2 => "32",
            3 => "28",
            _ => "24"
        };
    }

    private static void AppendIndentation(ParagraphProperties properties, ParagraphFormatProfile format)
    {
        var indentation = new Indentation();
        var hasIndentation = false;

        if (TryParseStringValue(format.LeftIndent, out var leftIndent))
        {
            indentation.Left = leftIndent;
            hasIndentation = true;
        }

        if (TryParseStringValue(format.RightIndent, out var rightIndent))
        {
            indentation.Right = rightIndent;
            hasIndentation = true;
        }

        if (TryParseStringValue(format.FirstLineIndent, out var firstLineIndent))
        {
            indentation.FirstLine = firstLineIndent;
            hasIndentation = true;
        }

        if (TryParseInt32String(format.FirstLineChars, out var firstLineChars))
        {
            indentation.FirstLineChars = firstLineChars;
            hasIndentation = true;
        }

        if (TryParseStringValue(format.HangingIndent, out var hangingIndent))
        {
            indentation.Hanging = hangingIndent;
            hasIndentation = true;
        }

        if (TryParseInt32String(format.HangingChars, out var hangingChars))
        {
            indentation.HangingChars = hangingChars;
            hasIndentation = true;
        }

        if (hasIndentation)
            properties.Append(indentation);
    }

    private static void AppendTabStops(ParagraphProperties properties, ParagraphFormatProfile format)
    {
        if (format.TabStops == null || format.TabStops.Count == 0)
            return;

        var tabs = new Tabs();
        foreach (var stop in format.TabStops)
        {
            if (!TryParseInt32String(stop.Position, out var position))
                continue;
            var tabStop = new TabStop
            {
                Val = ParseTabStopAlignment(stop.Alignment),
                Position = position
            };
            tabs.Append(tabStop);
        }

        if (tabs.HasChildren)
            properties.Append(tabs);
    }

    private static TabStopValues ParseTabStopAlignment(string? alignment)
    {
        return (alignment ?? "left").Trim().ToLowerInvariant() switch
        {
            "center" => TabStopValues.Center,
            "right" => TabStopValues.Right,
            "decimal" => TabStopValues.Decimal,
            "bar" => TabStopValues.Bar,
            "clear" => TabStopValues.Clear,
            _ => TabStopValues.Left
        };
    }

    private static void AppendSpacing(ParagraphProperties properties, ParagraphFormatProfile format)
    {
        var spacing = new SpacingBetweenLines();
        var hasSpacing = false;

        if (TryParseStringValue(format.BeforeSpacing, out var beforeSpacing))
        {
            spacing.Before = beforeSpacing;
            hasSpacing = true;
        }

        if (TryParseStringValue(format.AfterSpacing, out var afterSpacing))
        {
            spacing.After = afterSpacing;
            hasSpacing = true;
        }

        if (TryParseStringValue(format.LineSpacing, out var lineSpacing))
        {
            spacing.Line = lineSpacing;
            hasSpacing = true;
        }

        if (TryParseLineRule(format.LineSpacingRule, out var lineRule))
        {
            spacing.LineRule = lineRule;
            hasSpacing = true;
        }

        if (hasSpacing)
            properties.Append(spacing);
    }

    private static SimpleField BuildField(string instruction, RunFormatProfile? format, int? headingLevel)
    {
        var field = new SimpleField
        {
            Instruction = instruction
        };

        var run = new Run();
        var properties = BuildRunProperties(format, headingLevel);
        if (properties.ChildElements.Count > 0)
            run.Append(properties);

        run.Append(new Text(string.Empty));
        field.Append(run);
        return field;
    }

    private static RunFormatProfile GetDefaultRunFormat(int? headingLevel, RunFormatProfile? format)
    {
        format ??= new RunFormatProfile();
        return new RunFormatProfile
        {
            FontSize = format.FontSize ?? GetFallbackFontSize(headingLevel),
            AsciiFont = string.IsNullOrWhiteSpace(format.AsciiFont) ? "Times New Roman" : format.AsciiFont,
            EastAsiaFont = string.IsNullOrWhiteSpace(format.EastAsiaFont) ? "宋体" : format.EastAsiaFont,
            Bold = format.Bold,
            Italic = format.Italic,
            Underline = format.Underline,
            FontColor = NormalizeFontColor(format.FontColor),
            Highlight = format.Highlight,
            VerticalAlign = format.VerticalAlign
        };
    }

    private static string? NormalizeAsciiFont(string? font)
    {
        if (string.IsNullOrWhiteSpace(font))
            return "Times New Roman";

        return font;
    }

    private static string? NormalizeEastAsiaFont(string? font)
    {
        if (string.IsNullOrWhiteSpace(font))
            return "宋体";

        return font.Contains("瀹", StringComparison.Ordinal)
            || font.Contains("嬩", StringComparison.Ordinal)
            ? "宋体"
            : font;
    }

    private static RunFormatProfile GetDefaultEquationRunFormat(RunFormatProfile? format)
    {
        return GetDefaultRunFormat(null, format);
    }

    private static string? NormalizeFontColor(string? color)
    {
        if (string.IsNullOrWhiteSpace(color))
            return null;

        return color.Equals("auto", StringComparison.OrdinalIgnoreCase)
            || color.Equals("FFFFFF", StringComparison.OrdinalIgnoreCase)
            || color.Equals("white", StringComparison.OrdinalIgnoreCase)
            ? "000000"
            : color;
    }
}
