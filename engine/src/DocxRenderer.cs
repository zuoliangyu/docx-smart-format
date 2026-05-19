using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Wordprocessing;
using System.Xml.Linq;
using System.Text.RegularExpressions;
using A = DocumentFormat.OpenXml.Drawing;
using Dw = DocumentFormat.OpenXml.Drawing.Wordprocessing;
using Pic = DocumentFormat.OpenXml.Drawing.Pictures;

namespace DocxWeb;

// DocxRenderer is split into feature-focused partials (R1, architecture rewrite):
//   DocxRenderer.cs              - render orchestration + package props
//   DocxRenderer.Section.cs      - segment splitting, section properties, settings
//   DocxRenderer.HeaderFooter.cs - header/footer parts + vertical-pagenum textbox (VML dual track)
//   DocxRenderer.Blocks.cs       - paragraph / table / image / equation blocks
//   DocxRenderer.Inline.cs       - runs, fonts, indentation, tabs, spacing, fields
//   DocxRenderer.AutoFormat.cs   - chemistry / unit-exponent / sub-superscript tokenizer
//   DocxRenderer.Parsers.cs      - small value parsers shared across the partials
// This split is behavior-preserving: golden snapshots must stay byte-identical.
internal static partial class DocxRenderer
{
    public static void Render(RenderSpec spec, string outputPath, string? templatePath = null)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outputPath))!);
        if (File.Exists(outputPath))
            File.Delete(outputPath);

        using var document = WordprocessingDocument.Create(outputPath, WordprocessingDocumentType.Document);
        var mainPart = document.AddMainDocumentPart();
        mainPart.Document = new Document(new Body());
        if (!string.IsNullOrWhiteSpace(templatePath) && File.Exists(templatePath))
            CopyTemplateStyles(templatePath, mainPart);

        EnsureDocumentSettings(mainPart, spec);

        ApplyPackageProperties(document, spec.Document);

        var headerRefs = CreateHeaderParts(mainPart, spec.Headers);
        var footerRefs = CreateFooterParts(mainPart, spec.Footers);

        var body = mainPart.Document.Body!;
        var segments = SplitSegments(spec);
        if (segments.Count == 0)
        {
            segments.Add(new BodySegment
            {
                Section = ToDefaultSection(spec.Document)
            });
        }

        for (var index = 0; index < segments.Count; index++)
        {
            var segment = segments[index];
            foreach (var block in segment.Blocks)
            {
                switch (block.Kind.ToLowerInvariant())
                {
                    case "paragraph":
                    case "heading":
                    case "reference":
                        body.Append(BuildParagraph(block.Paragraph));
                        break;
                    case "table":
                        body.Append(BuildTable(block.Table));
                        break;
                    case "image":
                        var imageParagraph = BuildImageParagraph(mainPart, block.Image);
                        if (imageParagraph != null)
                            body.Append(imageParagraph);
                        break;
                    case "equation":
                        var equationParagraph = BuildEquationParagraph(block.Equation);
                        if (equationParagraph != null)
                            body.Append(equationParagraph);
                        break;
                }
            }

            if (index < segments.Count - 1)
            {
                AttachSectionBreak(body, BuildSectionProperties(
                    segment.Section ?? ToDefaultSection(spec.Document),
                    headerRefs,
                    footerRefs));
            }
        }

        body.Append(BuildSectionProperties(
            segments[^1].Section ?? ToDefaultSection(spec.Document),
            headerRefs,
            footerRefs));

        mainPart.Document.Save();
    }

    private static void ApplyPackageProperties(WordprocessingDocument document, RenderDocumentSpec spec)
    {
        if (!string.IsNullOrWhiteSpace(spec.Title))
            document.PackageProperties.Title = spec.Title;

        if (!string.IsNullOrWhiteSpace(spec.Author))
            document.PackageProperties.Creator = spec.Author;
    }

    private sealed class BodySegment
    {
        public RenderSectionSpec? Section { get; set; }

        public List<RenderBodyBlock> Blocks { get; } = [];
    }
}
