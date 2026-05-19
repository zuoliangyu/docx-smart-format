using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Wordprocessing;
using System.Xml.Linq;
using System.Text.RegularExpressions;
using A = DocumentFormat.OpenXml.Drawing;
using Dw = DocumentFormat.OpenXml.Drawing.Wordprocessing;
using Pic = DocumentFormat.OpenXml.Drawing.Pictures;

namespace DocxWeb;

// Semantic text auto-formatting: ^/_ script markers, chemistry subscripts,
// unit-exponent superscripts. Self-contained tokenizer.
internal static partial class DocxRenderer
{
    private static List<AutoFormatSegment> SplitAutoFormatSegments(string text, RunFormatProfile? baseFormat)
    {
        baseFormat ??= new RunFormatProfile();
        if (string.IsNullOrEmpty(text))
            return [new AutoFormatSegment(text, CloneRunFormat(baseFormat))];

        if (!string.IsNullOrWhiteSpace(baseFormat.VerticalAlign))
            return [new AutoFormatSegment(text, CloneRunFormat(baseFormat))];

        var result = new List<AutoFormatSegment>();
        var buffer = "";
        var index = 0;

        while (index < text.Length)
        {
            if (TryReadScriptMarker(text, ref index, out var markerText, out var markerAlign))
            {
                FlushBuffer(result, ref buffer, baseFormat);
                result.Add(new AutoFormatSegment(markerText, CloneRunFormat(baseFormat, markerAlign)));
                continue;
            }

            if (TryReadToken(text, ref index, out var token))
            {
                if (TrySplitChemistryToken(token, baseFormat, out var chemicalSegments)
                    || TrySplitUnitExponentToken(token, baseFormat, out chemicalSegments))
                {
                    FlushBuffer(result, ref buffer, baseFormat);
                    result.AddRange(chemicalSegments);
                }
                else
                {
                    buffer += token;
                }

                continue;
            }

            buffer += text[index];
            index++;
        }

        FlushBuffer(result, ref buffer, baseFormat);
        return result.Count == 0
            ? [new AutoFormatSegment(text, CloneRunFormat(baseFormat))]
            : result;
    }

    private static bool TryReadScriptMarker(string text, ref int index, out string markerText, out string markerAlign)
    {
        markerText = "";
        markerAlign = "";

        if (index >= text.Length)
            return false;

        var marker = text[index];
        if (marker != '^' && marker != '_')
            return false;

        var start = index + 1;
        if (start >= text.Length)
            return false;

        if (text[start] == '{')
        {
            var endBrace = text.IndexOf('}', start + 1);
            if (endBrace <= start + 1)
                return false;

            markerText = text[(start + 1)..endBrace];
            markerAlign = marker == '^' ? "superscript" : "subscript";
            index = endBrace + 1;
            return markerText.Length > 0;
        }

        var cursor = start;
        while (cursor < text.Length && IsScriptChar(text[cursor]))
            cursor++;

        if (cursor == start)
            return false;

        markerText = text[start..cursor];
        markerAlign = marker == '^' ? "superscript" : "subscript";
        index = cursor;
        return true;
    }

    private static bool TryReadToken(string text, ref int index, out string token)
    {
        token = "";
        if (index >= text.Length || !IsTokenChar(text[index]))
            return false;

        var start = index;
        while (index < text.Length && IsTokenChar(text[index]))
            index++;

        token = text[start..index];
        return token.Length > 0;
    }

    private static bool TrySplitChemistryToken(string token, RunFormatProfile baseFormat, out List<AutoFormatSegment> segments)
    {
        segments = [];
        if (string.IsNullOrWhiteSpace(token))
            return false;

        var hasUpper = token.Any(char.IsUpper);
        var hasDigit = token.Any(char.IsDigit);
        if (!hasUpper || (!hasDigit && !token.Contains('+') && !token.Contains('-')))
            return false;

        var chargeMatch = Regex.Match(token, @"^(?<base>.*?)(?<charge>\d*[+-]+)$");
        var core = chargeMatch.Success ? chargeMatch.Groups["base"].Value : token;
        var charge = chargeMatch.Success ? chargeMatch.Groups["charge"].Value : null;

        var plain = "";
        for (var i = 0; i < core.Length; i++)
        {
            var ch = core[i];
            if (char.IsDigit(ch) && i > 0 && IsChemicalSubscriptAnchor(core[i - 1]))
            {
                FlushBuffer(segments, ref plain, baseFormat);
                var start = i;
                while (i < core.Length && char.IsDigit(core[i]))
                    i++;

                segments.Add(new AutoFormatSegment(core[start..i], CloneRunFormat(baseFormat, "subscript")));
                i--;
                continue;
            }

            plain += ch;
        }

        FlushBuffer(segments, ref plain, baseFormat);

        if (!string.IsNullOrEmpty(charge))
            segments.Add(new AutoFormatSegment(charge, CloneRunFormat(baseFormat, "superscript")));

        return segments.Any(x => !string.IsNullOrWhiteSpace(x.Format.VerticalAlign));
    }

    private static bool TrySplitUnitExponentToken(string token, RunFormatProfile baseFormat, out List<AutoFormatSegment> segments)
    {
        segments = [];
        var match = Regex.Match(token, @"^(?<base>(?:mm|cm|dm|m|km|nm|um|μm|µm|L|mL|μL|µL|g|kg|mg|s|min|h|Hz|kHz|MHz|GHz|Pa|kPa|MPa|V|mV|A|mA|W|kW|J|kJ|N|mol|℃|°C|K))(?<exp>[23])$");
        if (!match.Success)
            return false;

        segments.Add(new AutoFormatSegment(match.Groups["base"].Value, CloneRunFormat(baseFormat)));
        segments.Add(new AutoFormatSegment(match.Groups["exp"].Value, CloneRunFormat(baseFormat, "superscript")));
        return true;
    }

    private static void FlushBuffer(List<AutoFormatSegment> segments, ref string buffer, RunFormatProfile baseFormat)
    {
        if (buffer.Length == 0)
            return;

        segments.Add(new AutoFormatSegment(buffer, CloneRunFormat(baseFormat)));
        buffer = "";
    }

    private static bool IsChemicalSubscriptAnchor(char ch)
    {
        return char.IsLetter(ch) || ch == ')' || ch == ']' || ch == '}' ;
    }

    private static bool IsTokenChar(char ch)
    {
        return char.IsLetterOrDigit(ch)
            || ch == '('
            || ch == ')'
            || ch == '['
            || ch == ']'
            || ch == '{'
            || ch == '}'
            || ch == '+'
            || ch == '-'
            || ch == '·'
            || ch == '°'
            || ch == 'μ'
            || ch == 'µ';
    }

    private static bool IsScriptChar(char ch)
    {
        return char.IsLetterOrDigit(ch) || ch == '+' || ch == '-';
    }

    private static RunFormatProfile CloneRunFormat(RunFormatProfile source, string? verticalAlign = null)
    {
        return new RunFormatProfile
        {
            FontSize = source.FontSize,
            AsciiFont = source.AsciiFont,
            EastAsiaFont = source.EastAsiaFont,
            Bold = source.Bold,
            Italic = source.Italic,
            Underline = source.Underline,
            FontColor = source.FontColor,
            Highlight = source.Highlight,
            VerticalAlign = verticalAlign ?? source.VerticalAlign
        };
    }

    private sealed record AutoFormatSegment(string Text, RunFormatProfile Format);
}
