using System.Text.Json.Serialization;

namespace DocxWeb;

// FormatPlan: the single LLM-facing contract (R2, architecture rewrite).
//
// One block shape carries both intents:
//   - overlay  : block has `ref` -> reuse a source-document block, apply only
//                the formatting in `format` on top of it.
//   - generate : block has no `ref` -> author content from scratch.
//
// The Compiler lowers FormatPlan (+ optional analyzed source) into the
// internal RenderSpec IR, which DocxRenderer already knows how to emit.
public sealed class FormatPlan
{
    [JsonPropertyName("docType")]
    public string? DocType { get; set; }

    [JsonPropertyName("document")]
    public PlanDocument Document { get; set; } = new();

    [JsonPropertyName("sections")]
    public List<PlanSection> Sections { get; set; } = [];

    [JsonPropertyName("blocks")]
    public List<PlanBlock> Blocks { get; set; } = [];
}

public sealed class PlanDocument
{
    [JsonPropertyName("title")]
    public string? Title { get; set; }

    [JsonPropertyName("author")]
    public string? Author { get; set; }

    [JsonPropertyName("pageSize")]
    public string? PageSize { get; set; }

    [JsonPropertyName("orientation")]
    public string? Orientation { get; set; }

    [JsonPropertyName("margins")]
    public PlanMargins? Margins { get; set; }

    [JsonPropertyName("cnFont")]
    public string? CnFont { get; set; }

    [JsonPropertyName("enFont")]
    public string? EnFont { get; set; }

    // Base body font size in points (half-points handled by the Compiler).
    [JsonPropertyName("baseFontPt")]
    public double? BaseFontPt { get; set; }

    [JsonPropertyName("lineSpacing")]
    public double? LineSpacing { get; set; }

    [JsonPropertyName("headerFooter")]
    public PlanHeaderFooter? HeaderFooter { get; set; }
}

public sealed class PlanMargins
{
    [JsonPropertyName("top")]
    public string? Top { get; set; }

    [JsonPropertyName("bottom")]
    public string? Bottom { get; set; }

    [JsonPropertyName("left")]
    public string? Left { get; set; }

    [JsonPropertyName("right")]
    public string? Right { get; set; }
}

public sealed class PlanHeaderFooter
{
    [JsonPropertyName("oddEven")]
    public bool OddEven { get; set; }

    [JsonPropertyName("firstDifferent")]
    public bool FirstDifferent { get; set; }

    // none | continuous | center-page-number (R2 baseline set).
    [JsonPropertyName("pageNumber")]
    public string? PageNumber { get; set; }
}

public sealed class PlanSection
{
    [JsonPropertyName("key")]
    public string Key { get; set; } = "";

    [JsonPropertyName("type")]
    public string? Type { get; set; }

    [JsonPropertyName("orientation")]
    public string? Orientation { get; set; }

    [JsonPropertyName("margins")]
    public PlanMargins? Margins { get; set; }

    [JsonPropertyName("pageStart")]
    public int? PageStart { get; set; }

    [JsonPropertyName("pageNumFmt")]
    public string? PageNumFmt { get; set; }

    [JsonPropertyName("titlePage")]
    public bool TitlePage { get; set; }
}

public sealed class PlanBlock
{
    // Source block path/id for overlay intent; null/empty => generate.
    [JsonPropertyName("ref")]
    public string? Ref { get; set; }

    // title | heading1..heading3 | body | reference | caption |
    // figure | table | equation | pageNumber
    [JsonPropertyName("role")]
    public string Role { get; set; } = "body";

    // Key of the PlanSection this block belongs to. A change of section
    // key between consecutive blocks emits a section break.
    [JsonPropertyName("sectionKey")]
    public string? SectionKey { get; set; }

    [JsonPropertyName("text")]
    public string? Text { get; set; }

    [JsonPropertyName("caption")]
    public string? Caption { get; set; }

    [JsonPropertyName("format")]
    public PlanFormat? Format { get; set; }

    [JsonPropertyName("image")]
    public PlanImage? Image { get; set; }

    [JsonPropertyName("table")]
    public PlanTable? Table { get; set; }

    [JsonPropertyName("equation")]
    public PlanEquation? Equation { get; set; }
}

// Flattened formatting knobs (paragraph + run) so the LLM sets one object.
public sealed class PlanFormat
{
    [JsonPropertyName("bold")]
    public bool? Bold { get; set; }

    [JsonPropertyName("italic")]
    public bool? Italic { get; set; }

    [JsonPropertyName("underline")]
    public string? Underline { get; set; }

    [JsonPropertyName("fontColor")]
    public string? FontColor { get; set; }

    [JsonPropertyName("highlight")]
    public string? Highlight { get; set; }

    [JsonPropertyName("verticalAlign")]
    public string? VerticalAlign { get; set; }

    [JsonPropertyName("cnFont")]
    public string? CnFont { get; set; }

    [JsonPropertyName("enFont")]
    public string? EnFont { get; set; }

    [JsonPropertyName("fontPt")]
    public double? FontPt { get; set; }

    [JsonPropertyName("align")]
    public string? Align { get; set; }

    [JsonPropertyName("firstLineIndent")]
    public string? FirstLineIndent { get; set; }

    [JsonPropertyName("hangingIndent")]
    public string? HangingIndent { get; set; }

    [JsonPropertyName("lineSpacing")]
    public string? LineSpacing { get; set; }

    [JsonPropertyName("beforeSpacing")]
    public string? BeforeSpacing { get; set; }

    [JsonPropertyName("afterSpacing")]
    public string? AfterSpacing { get; set; }

    [JsonPropertyName("pageBreakBefore")]
    public bool? PageBreakBefore { get; set; }
}

public sealed class PlanImage
{
    [JsonPropertyName("path")]
    public string? Path { get; set; }

    [JsonPropertyName("contentType")]
    public string? ContentType { get; set; }

    [JsonPropertyName("widthEmu")]
    public long? WidthEmu { get; set; }

    [JsonPropertyName("heightEmu")]
    public long? HeightEmu { get; set; }

    [JsonPropertyName("altText")]
    public string? AltText { get; set; }
}

public sealed class PlanTable
{
    [JsonPropertyName("rows")]
    public List<List<string>> Rows { get; set; } = [];
}

public sealed class PlanEquation
{
    [JsonPropertyName("text")]
    public string? Text { get; set; }

    [JsonPropertyName("xml")]
    public string Xml { get; set; } = "";

    [JsonPropertyName("displayMode")]
    public string DisplayMode { get; set; } = "inline";
}
