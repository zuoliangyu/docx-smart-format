using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Wordprocessing;
using System.Xml.Linq;
using System.Text.RegularExpressions;
using A = DocumentFormat.OpenXml.Drawing;
using Dw = DocumentFormat.OpenXml.Drawing.Wordprocessing;
using Pic = DocumentFormat.OpenXml.Drawing.Pictures;

namespace DocxWeb;

// Header/footer parts and the landscape vertical-pagenum textbox (wps:wsp + VML
// v:rect dual track). Isolated here as the future trim/plug-in seam.
internal static partial class DocxRenderer
{
    private static Dictionary<string, string> CreateHeaderParts(
        MainDocumentPart mainPart,
        IEnumerable<RenderHeaderFooterSpec> headers)
    {
        var result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var header in headers)
        {
            var type = NormalizeHeaderFooterType(header.Type);
            var part = mainPart.AddNewPart<HeaderPart>();
            part.Header = new Header();
            foreach (var paragraph in header.Paragraphs)
            {
                paragraph.Paragraph.FirstLineChars = "0";
                paragraph.Paragraph.FirstLineIndent = "0";
                part.Header.Append(BuildParagraph(paragraph));
            }

            AppendTextBoxParagraphs(part.Header, header.TextBoxes, ensureHostParagraph: header.Paragraphs.Count == 0);

            var partId = mainPart.GetIdOfPart(part);
            if (!result.ContainsKey($"type:{type}"))
                result[$"type:{type}"] = partId;
            if (!string.IsNullOrWhiteSpace(header.SourcePath))
                result[$"path:{header.SourcePath}"] = partId;
            if (!string.IsNullOrWhiteSpace(header.RelationshipId))
                result[$"rel:{header.RelationshipId}"] = partId;
        }

        return result;
    }

    private static Dictionary<string, string> CreateFooterParts(
        MainDocumentPart mainPart,
        IEnumerable<RenderHeaderFooterSpec> footers)
    {
        var result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var footer in footers)
        {
            var type = NormalizeHeaderFooterType(footer.Type);
            var part = mainPart.AddNewPart<FooterPart>();
            part.Footer = new Footer();
            foreach (var paragraph in footer.Paragraphs)
            {
                paragraph.Paragraph.FirstLineChars = "0";
                paragraph.Paragraph.FirstLineIndent = "0";
                part.Footer.Append(BuildParagraph(paragraph));
            }

            AppendTextBoxParagraphs(part.Footer, footer.TextBoxes, ensureHostParagraph: footer.Paragraphs.Count == 0);

            var partId = mainPart.GetIdOfPart(part);
            if (!result.ContainsKey($"type:{type}"))
                result[$"type:{type}"] = partId;
            if (!string.IsNullOrWhiteSpace(footer.SourcePath))
                result[$"path:{footer.SourcePath}"] = partId;
            if (!string.IsNullOrWhiteSpace(footer.RelationshipId))
                result[$"rel:{footer.RelationshipId}"] = partId;
        }

        return result;
    }

    private static void AppendTextBoxParagraphs(
        OpenXmlElement container,
        IList<RenderTextBoxSpec> textBoxes,
        bool ensureHostParagraph)
    {
        if (textBoxes.Count == 0)
        {
            if (ensureHostParagraph && !container.Elements<Paragraph>().Any())
                container.Append(new Paragraph(new Run()));
            return;
        }

        var hostParagraph = container.Elements<Paragraph>().LastOrDefault();
        if (hostParagraph == null)
        {
            hostParagraph = new Paragraph();
            var properties = new ParagraphProperties();
            properties.Append(new SpacingBetweenLines
            {
                Before = "0",
                After = "0",
                Line = "240",
                LineRule = LineSpacingRuleValues.Auto
            });
            hostParagraph.Append(properties);
            container.Append(hostParagraph);
        }

        foreach (var textBox in textBoxes)
        {
            var run = BuildTextBoxRun(textBox);
            if (run != null)
                hostParagraph.Append(run);
        }
    }

    private static Run? BuildTextBoxRun(RenderTextBoxSpec textBox)
    {
        var xml = BuildTextBoxAlternateContentXml(textBox);
        if (string.IsNullOrEmpty(xml))
            return null;

        var run = new Run();
        try
        {
            run.InnerXml = xml;
        }
        catch
        {
            return null;
        }

        return run;
    }

    private static string BuildTextBoxAlternateContentXml(RenderTextBoxSpec textBox)
    {
        var posX = textBox.PosXEmu ?? 457200L;
        var posY = textBox.PosYEmu ?? 1828800L;
        var width = textBox.WidthEmu ?? 457200L;
        var height = textBox.HeightEmu ?? 7772400L;
        var textDirection = string.IsNullOrWhiteSpace(textBox.TextDirection) ? "vert270" : textBox.TextDirection!;
        var relativeFromH = string.IsNullOrWhiteSpace(textBox.RelativeFromH) ? "page" : textBox.RelativeFromH!;
        var relativeFromV = string.IsNullOrWhiteSpace(textBox.RelativeFromV) ? "page" : textBox.RelativeFromV!;
        var anchor = string.IsNullOrWhiteSpace(textBox.Anchor) ? "ctr" : textBox.Anchor!;
        var name = string.IsNullOrWhiteSpace(textBox.Name) ? "VerticalTextBox" : textBox.Name!;

        var isFilled = !string.IsNullOrWhiteSpace(textBox.FillColor)
            && !string.Equals(textBox.FillColor, "none", StringComparison.OrdinalIgnoreCase);
        var fill = isFilled
            ? $"<a:solidFill><a:srgbClr val=\"{textBox.FillColor}\"/></a:solidFill>"
            : "<a:noFill/>";

        var isStroked = !string.IsNullOrWhiteSpace(textBox.BorderStyle)
            && !string.Equals(textBox.BorderStyle, "none", StringComparison.OrdinalIgnoreCase);
        var border = isStroked
            ? "<a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill></a:ln>"
            : "<a:ln><a:noFill/></a:ln>";

        var paragraphXml = string.Concat(textBox.Paragraphs.Select(p => BuildParagraph(p).OuterXml));
        if (string.IsNullOrEmpty(paragraphXml))
            paragraphXml = "<w:p xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>";

        var culture = System.Globalization.CultureInfo.InvariantCulture;
        var vmlLeft = (posX / 12700.0).ToString("0.##", culture);
        var vmlTop = (posY / 12700.0).ToString("0.##", culture);
        var vmlWidth = (width / 12700.0).ToString("0.##", culture);
        var vmlHeight = (height / 12700.0).ToString("0.##", culture);

        var vmlLayoutFlow = textDirection switch
        {
            "vert270" => "vertical;mso-layout-flow-alt:bottom-to-top",
            "vert" => "vertical",
            _ => "vertical"
        };

        var fillVml = isFilled ? $"filled=\"t\" fillcolor=\"#{textBox.FillColor}\"" : "filled=\"f\"";
        var strokedVml = isStroked ? "stroked=\"t\"" : "stroked=\"f\"";

        return $"<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">"
            + "<mc:Choice Requires=\"wps\">"
            + "<w:drawing>"
            + "<wp:anchor distT=\"0\" distB=\"0\" distL=\"114300\" distR=\"114300\" simplePos=\"0\" relativeHeight=\"251659264\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">"
            + "<wp:simplePos x=\"0\" y=\"0\"/>"
            + $"<wp:positionH relativeFrom=\"{relativeFromH}\"><wp:posOffset>{posX}</wp:posOffset></wp:positionH>"
            + $"<wp:positionV relativeFrom=\"{relativeFromV}\"><wp:posOffset>{posY}</wp:posOffset></wp:positionV>"
            + $"<wp:extent cx=\"{width}\" cy=\"{height}\"/>"
            + "<wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>"
            + "<wp:wrapNone/>"
            + $"<wp:docPr id=\"1\" name=\"{name}\"/>"
            + "<wp:cNvGraphicFramePr/>"
            + "<a:graphic>"
            + "<a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">"
            + "<wps:wsp>"
            + "<wps:cNvSpPr txBox=\"1\"/>"
            + "<wps:spPr>"
            + $"<a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm>"
            + "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>"
            + fill
            + border
            + "</wps:spPr>"
            + "<wps:txbx>"
            + $"<w:txbxContent>{paragraphXml}</w:txbxContent>"
            + "</wps:txbx>"
            + $"<wps:bodyPr rot=\"0\" spcFirstLastPara=\"0\" vertOverflow=\"visible\" horzOverflow=\"visible\" vert=\"{textDirection}\" wrap=\"square\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" numCol=\"1\" spcCol=\"0\" rtlCol=\"0\" fromWordArt=\"0\" anchor=\"{anchor}\" anchorCtr=\"0\" forceAA=\"0\" compatLnSpc=\"1\"/>"
            + "</wps:wsp>"
            + "</a:graphicData>"
            + "</a:graphic>"
            + "</wp:anchor>"
            + "</w:drawing>"
            + "</mc:Choice>"
            + "<mc:Fallback>"
            + "<w:pict xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:w10=\"urn:schemas-microsoft-com:office:word\">"
            + $"<v:rect id=\"_x0000_s1026\" style=\"position:absolute;margin-left:{vmlLeft}pt;margin-top:{vmlTop}pt;width:{vmlWidth}pt;height:{vmlHeight}pt;z-index:251659264;mso-position-horizontal-relative:{relativeFromH};mso-position-vertical-relative:{relativeFromV}\" {strokedVml} {fillVml}>"
            + $"<v:textbox style=\"layout-flow:{vmlLayoutFlow}\" inset=\"7.2pt,3.6pt,7.2pt,3.6pt\">"
            + $"<w:txbxContent>{paragraphXml}</w:txbxContent>"
            + "</v:textbox>"
            + "</v:rect>"
            + "</w:pict>"
            + "</mc:Fallback>"
            + "</mc:AlternateContent>";
    }

    private static HeaderFooterValues ParseHeaderFooterType(string? value)
    {
        return value?.Trim().ToLowerInvariant() switch
        {
            "first" => HeaderFooterValues.First,
            "even" => HeaderFooterValues.Even,
            _ => HeaderFooterValues.Default
        };
    }

    private static string NormalizeHeaderFooterType(string? value)
    {
        return value?.Trim().ToLowerInvariant() switch
        {
            "first" => "first",
            "even" => "even",
            _ => "default"
        };
    }

    private static IEnumerable<KeyValuePair<string, string>> SelectHeaderFooterRefs(
        Dictionary<string, string> refs,
        IReadOnlyList<RenderHeaderFooterReferenceSpec> explicitRefs,
        string? preferredType,
        string? preferredSourcePath)
    {
        if (explicitRefs.Count > 0)
        {
            var explicitResult = new List<KeyValuePair<string, string>>();
            foreach (var item in explicitRefs)
            {
                var type = NormalizeHeaderFooterType(item.Type);
                if (!string.IsNullOrWhiteSpace(item.SourcePath)
                    && refs.TryGetValue($"path:{item.SourcePath}", out var partByPath))
                {
                    explicitResult.Add(new KeyValuePair<string, string>(type, partByPath));
                    continue;
                }

                if (!string.IsNullOrWhiteSpace(item.SourcePath)
                    && refs.TryGetValue($"rel:{item.SourcePath}", out var partByRel))
                {
                    explicitResult.Add(new KeyValuePair<string, string>(type, partByRel));
                    continue;
                }

                if (refs.TryGetValue($"type:{type}", out var partByType))
                    explicitResult.Add(new KeyValuePair<string, string>(type, partByType));
            }

            if (explicitResult.Count > 0)
                return explicitResult;
        }

        if (!string.IsNullOrWhiteSpace(preferredSourcePath)
            && refs.TryGetValue($"path:{preferredSourcePath}", out var preferredPath))
        {
            return [new KeyValuePair<string, string>(NormalizeHeaderFooterType(preferredType), preferredPath)];
        }

        if (!string.IsNullOrWhiteSpace(preferredSourcePath)
            && refs.TryGetValue($"rel:{preferredSourcePath}", out var preferredRel))
        {
            return [new KeyValuePair<string, string>(NormalizeHeaderFooterType(preferredType), preferredRel)];
        }

        var normalizedPreferred = NormalizeHeaderFooterType(preferredType);
        if (refs.TryGetValue($"type:{normalizedPreferred}", out var preferred))
            return [new KeyValuePair<string, string>(normalizedPreferred, preferred)];

        if (refs.TryGetValue("type:default", out var fallback))
            return [new KeyValuePair<string, string>("default", fallback)];

        return refs
            .Where(x => x.Key.StartsWith("type:", StringComparison.OrdinalIgnoreCase))
            .Select(x => new KeyValuePair<string, string>(x.Key["type:".Length..], x.Value));
    }
}
