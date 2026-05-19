using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Wordprocessing;
using System.Xml.Linq;
using System.Text.RegularExpressions;
using A = DocumentFormat.OpenXml.Drawing;
using Dw = DocumentFormat.OpenXml.Drawing.Wordprocessing;
using Pic = DocumentFormat.OpenXml.Drawing.Pictures;

namespace DocxWeb;

// Small value parsers shared across the renderer partials.
internal static partial class DocxRenderer
{
    private static JustificationValues? ParseAlignment(string? value)
    {
        return value?.Trim().ToLowerInvariant() switch
        {
            "center" => JustificationValues.Center,
            "right" => JustificationValues.Right,
            "both" => JustificationValues.Both,
            "justify" => JustificationValues.Both,
            "distribute" => JustificationValues.Distribute,
            "left" => JustificationValues.Left,
            _ => null
        };
    }

    private static bool TryParseOrientation(string? value, out PageOrientationValues orientation)
    {
        switch (value?.Trim().ToLowerInvariant())
        {
            case "landscape":
                orientation = PageOrientationValues.Landscape;
                return true;
            case "portrait":
                orientation = PageOrientationValues.Portrait;
                return true;
            default:
                orientation = default;
                return false;
        }
    }

    private static bool TryParseSectionType(string? value, out SectionMarkValues sectionType)
    {
        switch (value?.Trim().ToLowerInvariant())
        {
            case "continuous":
                sectionType = SectionMarkValues.Continuous;
                return true;
            case "evenpage":
                sectionType = SectionMarkValues.EvenPage;
                return true;
            case "oddpage":
                sectionType = SectionMarkValues.OddPage;
                return true;
            case "nextcolumn":
                sectionType = SectionMarkValues.NextColumn;
                return true;
            case "nextpage":
                sectionType = SectionMarkValues.NextPage;
                return true;
            default:
                sectionType = default;
                return false;
        }
    }

    private static bool TryParseUnderline(string? value, out UnderlineValues underline)
    {
        switch (value?.Trim().ToLowerInvariant())
        {
            case "single":
                underline = UnderlineValues.Single;
                return true;
            case "double":
                underline = UnderlineValues.Double;
                return true;
            case "none":
                underline = UnderlineValues.None;
                return true;
            default:
                underline = default;
                return false;
        }
    }

    private static bool TryParseLineRule(string? value, out LineSpacingRuleValues lineRule)
    {
        switch (value?.Trim().ToLowerInvariant())
        {
            case "auto":
                lineRule = LineSpacingRuleValues.Auto;
                return true;
            case "exact":
                lineRule = LineSpacingRuleValues.Exact;
                return true;
            case "atleast":
            case "at-least":
                lineRule = LineSpacingRuleValues.AtLeast;
                return true;
            default:
                lineRule = default;
                return false;
        }
    }

    private static bool TryParseHighlight(string? value, out HighlightColorValues highlight)
    {
        switch (value?.Trim().ToLowerInvariant())
        {
            case "yellow":
                highlight = HighlightColorValues.Yellow;
                return true;
            case "green":
                highlight = HighlightColorValues.Green;
                return true;
            case "cyan":
                highlight = HighlightColorValues.Cyan;
                return true;
            case "magenta":
                highlight = HighlightColorValues.Magenta;
                return true;
            case "blue":
                highlight = HighlightColorValues.Blue;
                return true;
            case "red":
                highlight = HighlightColorValues.Red;
                return true;
            case "darkyellow":
                highlight = HighlightColorValues.DarkYellow;
                return true;
            case "darkblue":
                highlight = HighlightColorValues.DarkBlue;
                return true;
            case "darkcyan":
                highlight = HighlightColorValues.DarkCyan;
                return true;
            case "darkgreen":
                highlight = HighlightColorValues.DarkGreen;
                return true;
            case "darkmagenta":
                highlight = HighlightColorValues.DarkMagenta;
                return true;
            case "darkred":
                highlight = HighlightColorValues.DarkRed;
                return true;
            case "darkgray":
                highlight = HighlightColorValues.DarkGray;
                return true;
            case "lightgray":
                highlight = HighlightColorValues.LightGray;
                return true;
            case "black":
                highlight = HighlightColorValues.Black;
                return true;
            case "white":
                highlight = HighlightColorValues.White;
                return true;
            default:
                highlight = default;
                return false;
        }
    }

    private static bool TryParseVerticalAlign(string? value, out VerticalPositionValues verticalAlign)
    {
        switch (value?.Trim().ToLowerInvariant())
        {
            case "superscript":
            case "super":
            case "上标":
                verticalAlign = VerticalPositionValues.Superscript;
                return true;
            case "subscript":
            case "sub":
            case "下标":
                verticalAlign = VerticalPositionValues.Subscript;
                return true;
            case "baseline":
            case "normal":
            case "基线":
                verticalAlign = VerticalPositionValues.Baseline;
                return true;
            default:
                verticalAlign = default;
                return false;
        }
    }

    private static bool TryParseStringValue(string? value, out string result)
    {
        if (!string.IsNullOrWhiteSpace(value))
        {
            result = value;
            return true;
        }

        result = string.Empty;
        return false;
    }

    private static bool TryParseUInt(string? value, out UInt32Value result)
    {
        if (uint.TryParse(value, out var parsed))
        {
            result = parsed;
            return true;
        }

        result = default!;
        return false;
    }

    private static bool TryParseInt(string? value, out Int32Value result)
    {
        if (int.TryParse(value, out var parsed))
        {
            result = parsed;
            return true;
        }

        result = default!;
        return false;
    }

    private static bool TryParseInt32String(string? value, out Int32Value result)
    {
        if (int.TryParse(value, out var parsed))
        {
            result = parsed;
            return true;
        }

        result = default!;
        return false;
    }
}
