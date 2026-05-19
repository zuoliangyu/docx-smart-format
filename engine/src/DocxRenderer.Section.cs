using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Wordprocessing;
using System.Xml.Linq;
using System.Text.RegularExpressions;
using A = DocumentFormat.OpenXml.Drawing;
using Dw = DocumentFormat.OpenXml.Drawing.Wordprocessing;
using Pic = DocumentFormat.OpenXml.Drawing.Pictures;

namespace DocxWeb;

// Body-to-section segmentation, section properties, document settings,
// template style copy.
internal static partial class DocxRenderer
{
    private static List<BodySegment> SplitSegments(RenderSpec spec)
    {
        var segments = new List<BodySegment>
        {
            new()
            {
                Section = ToDefaultSection(spec.Document)
            }
        };

        var current = segments[0];

        foreach (var block in spec.Body)
        {
            if (string.Equals(block.Kind, "section", StringComparison.OrdinalIgnoreCase))
            {
                if (current.Blocks.Count == 0 && segments.Count == 1)
                {
                    current.Section = MergeSection(current.Section, block.Section);
                    continue;
                }

                current = new BodySegment
                {
                    Section = MergeSection(ToDefaultSection(spec.Document), block.Section)
                };
                segments.Add(current);
                continue;
            }

            current.Blocks.Add(block);
        }

        return segments;
    }

    private static RenderSectionSpec ToDefaultSection(RenderDocumentSpec spec)
    {
        return new RenderSectionSpec
        {
            PageWidth = spec.PageWidth,
            PageHeight = spec.PageHeight,
            Orientation = spec.Orientation,
            MarginTop = spec.MarginTop,
            MarginBottom = spec.MarginBottom,
            MarginLeft = spec.MarginLeft,
            MarginRight = spec.MarginRight,
            PageNumFmt = null,
            Headers = [],
            Footers = []
        };
    }

    private static RenderSectionSpec MergeSection(RenderSectionSpec? fallback, RenderSectionSpec? overrideSpec)
    {
        fallback ??= new RenderSectionSpec();
        if (overrideSpec == null)
            return fallback;

        return new RenderSectionSpec
        {
            Type = overrideSpec.Type ?? fallback.Type,
            PageWidth = overrideSpec.PageWidth ?? fallback.PageWidth,
            PageHeight = overrideSpec.PageHeight ?? fallback.PageHeight,
            Orientation = overrideSpec.Orientation ?? fallback.Orientation,
            MarginTop = overrideSpec.MarginTop ?? fallback.MarginTop,
            MarginBottom = overrideSpec.MarginBottom ?? fallback.MarginBottom,
            MarginLeft = overrideSpec.MarginLeft ?? fallback.MarginLeft,
            MarginRight = overrideSpec.MarginRight ?? fallback.MarginRight,
            PageStart = overrideSpec.PageStart ?? fallback.PageStart,
            PageNumFmt = overrideSpec.PageNumFmt ?? fallback.PageNumFmt,
            TitlePage = overrideSpec.TitlePage || fallback.TitlePage,
            HeaderType = overrideSpec.HeaderType ?? fallback.HeaderType,
            FooterType = overrideSpec.FooterType ?? fallback.FooterType,
            HeaderSourcePath = overrideSpec.HeaderSourcePath ?? fallback.HeaderSourcePath,
            FooterSourcePath = overrideSpec.FooterSourcePath ?? fallback.FooterSourcePath,
            Headers = overrideSpec.Headers.Count > 0 ? [.. overrideSpec.Headers] : [.. fallback.Headers],
            Footers = overrideSpec.Footers.Count > 0 ? [.. overrideSpec.Footers] : [.. fallback.Footers]
        };
    }

    private static void AttachSectionBreak(Body body, SectionProperties sectionProperties)
    {
        Paragraph hostParagraph;
        if (body.LastChild is Paragraph lastParagraph)
        {
            hostParagraph = lastParagraph;
        }
        else
        {
            hostParagraph = new Paragraph(new Run());
            body.Append(hostParagraph);
        }

        var properties = hostParagraph.GetFirstChild<ParagraphProperties>();
        if (properties == null)
        {
            properties = new ParagraphProperties();
            hostParagraph.PrependChild(properties);
        }

        properties.RemoveAllChildren<SectionProperties>();
        properties.Append(sectionProperties);
    }

    private static SectionProperties BuildSectionProperties(
        RenderSectionSpec section,
        Dictionary<string, string> headerRefs,
        Dictionary<string, string> footerRefs)
    {
        var properties = new SectionProperties();

        foreach (var pair in SelectHeaderFooterRefs(headerRefs, section.Headers, section.HeaderType, section.HeaderSourcePath))
        {
            properties.Append(new HeaderReference
            {
                Type = ParseHeaderFooterType(pair.Key),
                Id = pair.Value
            });
        }

        foreach (var pair in SelectHeaderFooterRefs(footerRefs, section.Footers, section.FooterType, section.FooterSourcePath))
        {
            properties.Append(new FooterReference
            {
                Type = ParseHeaderFooterType(pair.Key),
                Id = pair.Value
            });
        }

        var pageSize = new PageSize();
        var hasPageSize = false;

        if (TryParseUInt(section.PageWidth, out var width))
        {
            pageSize.Width = width;
            hasPageSize = true;
        }

        if (TryParseUInt(section.PageHeight, out var height))
        {
            pageSize.Height = height;
            hasPageSize = true;
        }

        if (TryParseOrientation(section.Orientation, out var orientation))
        {
            pageSize.Orient = orientation;
            hasPageSize = true;
        }

        if (hasPageSize)
            properties.Append(pageSize);

        var pageMargin = new PageMargin();
        var hasMargin = false;

        if (TryParseInt(section.MarginTop, out var top))
        {
            pageMargin.Top = top;
            hasMargin = true;
        }

        if (TryParseInt(section.MarginBottom, out var bottom))
        {
            pageMargin.Bottom = bottom;
            hasMargin = true;
        }

        if (TryParseUInt(section.MarginLeft, out var left))
        {
            pageMargin.Left = left;
            hasMargin = true;
        }

        if (TryParseUInt(section.MarginRight, out var right))
        {
            pageMargin.Right = right;
            hasMargin = true;
        }

        if (hasMargin)
            properties.Append(pageMargin);

        if (section.PageStart.HasValue)
        {
            var pageNumberType = new PageNumberType
            {
                Start = section.PageStart.Value
            };

            if (TryParsePageNumberFormat(section.PageNumFmt, out var pageNumberFormat))
                pageNumberType.Format = pageNumberFormat;

            properties.Append(pageNumberType);
        }
        else if (TryParsePageNumberFormat(section.PageNumFmt, out var pageNumberFormat))
        {
            properties.Append(new PageNumberType
            {
                Format = pageNumberFormat
            });
        }

        if (section.TitlePage)
            properties.Append(new TitlePage());

        if (TryParseSectionType(section.Type, out var sectionType))
        {
            properties.Append(new SectionType
            {
                Val = sectionType
            });
        }

        return properties;
    }

    private static bool TryParsePageNumberFormat(string? value, out NumberFormatValues format)
    {
        switch (value?.Trim().ToLowerInvariant())
        {
            case "decimal":
                format = NumberFormatValues.Decimal;
                return true;
            case "upperroman":
                format = NumberFormatValues.UpperRoman;
                return true;
            case "lowerroman":
                format = NumberFormatValues.LowerRoman;
                return true;
            default:
                format = default;
                return false;
        }
    }

    private static void CopyTemplateStyles(string templatePath, MainDocumentPart targetMainPart)
    {
        using var templateDocument = WordprocessingDocument.Open(templatePath, false);
        var templateMainPart = templateDocument.MainDocumentPart;
        if (templateMainPart?.StyleDefinitionsPart?.Styles == null)
            return;

        var stylePart = targetMainPart.AddNewPart<StyleDefinitionsPart>();
        stylePart.Styles = (Styles)templateMainPart.StyleDefinitionsPart.Styles.CloneNode(true);

        if (templateMainPart.StylesWithEffectsPart?.Styles != null)
        {
            var effectsPart = targetMainPart.AddNewPart<StylesWithEffectsPart>();
            effectsPart.Styles = (DocumentFormat.OpenXml.Wordprocessing.Styles)templateMainPart.StylesWithEffectsPart.Styles.CloneNode(true);
        }
    }

    private static void EnsureDocumentSettings(MainDocumentPart mainPart, RenderSpec spec)
    {
        var needsOddEvenHeaders = spec.Headers.Any(x =>
            string.Equals(x.Type, "odd", StringComparison.OrdinalIgnoreCase)
            || string.Equals(x.Type, "even", StringComparison.OrdinalIgnoreCase))
            || spec.Footers.Any(x =>
                string.Equals(x.Type, "odd", StringComparison.OrdinalIgnoreCase)
                || string.Equals(x.Type, "even", StringComparison.OrdinalIgnoreCase))
            || spec.Body
                .Where(x => x.Section != null)
                .SelectMany(x => x.Section!.Headers.Concat(x.Section!.Footers))
                .Any(x =>
                    string.Equals(x.Type, "odd", StringComparison.OrdinalIgnoreCase)
                    || string.Equals(x.Type, "even", StringComparison.OrdinalIgnoreCase));

        if (!needsOddEvenHeaders)
            return;

        var settingsPart = mainPart.DocumentSettingsPart ?? mainPart.AddNewPart<DocumentSettingsPart>();
        settingsPart.Settings ??= new Settings();

        if (!settingsPart.Settings.Elements<EvenAndOddHeaders>().Any())
            settingsPart.Settings.Append(new EvenAndOddHeaders());

        settingsPart.Settings.Save();
    }
}
