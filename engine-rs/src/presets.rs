//! FormatPlan preset application.
//!
//! Ports .NET BuiltinTemplateFactory.CreateUndergraduateThesisTemplate.
//! When `document.preset == "undergraduate-thesis"`, this module fills
//! all missing format fields with the thesis defaults (font/size per role,
//! firstLineChars=200 for body, hangingChars=200 for references, 20pt
//! fixed line spacing, A4 portrait + standard margins, oddEven headers,
//! center-page-number footer).
//!
//! The header/footer parts themselves are NOT created here — docx.rs emits
//! them based on the preset name when building the package.

use crate::plan::{FormatPlan, PlanBlock, PlanDocument, PlanFormat, PlanHeaderFooter, PlanMargins};

pub const UNDERGRADUATE_THESIS: &str = "undergraduate-thesis";

pub fn is_undergraduate_thesis(doc: &PlanDocument) -> bool {
    doc.preset
        .as_deref()
        .map(|s| s.trim().eq_ignore_ascii_case(UNDERGRADUATE_THESIS))
        .unwrap_or(false)
}

/// Mutates the plan to fill in undergraduate-thesis defaults. Existing
/// caller-supplied values win — defaults are only injected into None /
/// missing fields. Safe to call multiple times (idempotent).
pub fn apply_undergraduate_thesis(plan: &mut FormatPlan) {
    // Document-level defaults: A4 portrait, 中文宋体 / 英文 Times New Roman,
    // 12pt base, 20pt fixed line spacing as block-level (we encode the
    // exact line value as 400 + lineRule=exact in each block's format).
    let d = &mut plan.document;
    d.page_size.get_or_insert_with(|| "a4".into());
    d.orientation.get_or_insert_with(|| "portrait".into());
    d.cn_font.get_or_insert_with(|| "宋体".into());
    d.en_font.get_or_insert_with(|| "Times New Roman".into());
    d.base_font_pt.get_or_insert(12.0);

    let m = d.margins.get_or_insert_with(PlanMargins::default);
    m.top.get_or_insert_with(|| "1440".into());
    m.bottom.get_or_insert_with(|| "1440".into());
    m.left.get_or_insert_with(|| "1800".into());
    m.right.get_or_insert_with(|| "1800".into());

    // Switch on oddEven headers + center page number footer. The actual
    // header/footer parts get generated in docx.rs; setting these flags
    // makes the section sectPr emit footerReference and the settings.xml
    // emit evenAndOddHeaders.
    let hf = d.header_footer.get_or_insert_with(PlanHeaderFooter::default);
    hf.odd_even = true;
    hf.page_number
        .get_or_insert_with(|| "center-page-number".into());

    // Per-block formatting defaults driven by role.
    for block in &mut plan.blocks {
        apply_block_defaults(block);
    }
}

fn apply_block_defaults(block: &mut PlanBlock) {
    let role = block.role.trim().to_ascii_lowercase();
    let role = role.as_str();
    // role-specific format profile
    let preset = match role {
        "heading1" => Some(thesis_format(
            18.0, true, "center", true,  /* pageBreakBefore */
            /* firstLineChars */ None, /* hangingChars */ None,
            "240", "120", "400", "exact",
        )),
        "heading2" => Some(thesis_format(
            16.0, true, "left", false,
            None, None,
            "200", "120", "400", "exact",
        )),
        "heading3" => Some(thesis_format(
            14.0, true, "left", false,
            None, None,
            "160", "80", "400", "exact",
        )),
        "title" => Some(thesis_format(
            22.0, true, "center", false,
            None, None,
            "240", "240", "480", "exact",
        )),
        "body" | "paragraph" | "" => Some(thesis_format(
            12.0, false, "both", false,
            Some("200"), None,
            "0", "0", "400", "exact",
        )),
        "reference" => Some(thesis_format(
            12.0, false, "both", false,
            None, Some("200"),
            "0", "0", "400", "exact",
        )),
        "caption" => Some(thesis_format(
            12.0, false, "center", false,
            None, None,
            "80", "80", "360", "exact",
        )),
        "equation" => Some(thesis_format(
            12.0, false, "center", false,
            None, None,
            "160", "160", "0", "auto",
        )),
        _ => None,
    };

    let Some(defaults) = preset else { return };

    let f = block.format.get_or_insert_with(PlanFormat::default);
    // Only fill what the caller hasn't explicitly set.
    merge_into(f, &defaults);
}

#[allow(clippy::too_many_arguments)]
fn thesis_format(
    pt: f64,
    bold: bool,
    align: &str,
    page_break_before: bool,
    first_line_chars: Option<&str>,
    hanging_chars: Option<&str>,
    before: &str,
    after: &str,
    line: &str,
    line_rule: &str,
) -> PlanFormat {
    // NOTE: FormatPlan exposes firstLineIndent/hangingIndent in twips;
    // the thesis spec is in 1/100 character units (firstLineChars=200 =
    // 2 chars). We approximate by converting to twips at 12pt base:
    // 2 chars × ~12pt × 20 (twips/pt) / 2 (12pt half-pt) ≈ 480 twips.
    // .NET uses dedicated Chars fields; Rust FormatPlan currently only
    // has twips. 480 twips = ~24pt indent which matches 2 Chinese chars
    // at 12pt visually.
    let first_line_indent = first_line_chars.map(|c| match c {
        "200" => "480".to_string(),
        other => other.to_string(),
    });
    let hanging_indent = hanging_chars.map(|c| match c {
        "200" => "480".to_string(),
        other => other.to_string(),
    });

    PlanFormat {
        bold: if bold { Some(true) } else { None },
        align: Some(align.to_string()),
        page_break_before: if page_break_before { Some(true) } else { None },
        first_line_indent,
        hanging_indent,
        before_spacing: Some(before.to_string()),
        after_spacing: Some(after.to_string()),
        line_spacing: Some(line.to_string()),
        line_spacing_rule: Some(line_rule.to_string()),
        font_pt: Some(pt),
        ..Default::default()
    }
}

/// Fill in any None / unset field on `dst` from `src`. Caller-set values
/// (Some) are never overwritten.
fn merge_into(dst: &mut PlanFormat, src: &PlanFormat) {
    macro_rules! fill {
        ($field:ident) => {
            if dst.$field.is_none() {
                dst.$field = src.$field.clone();
            }
        };
    }
    fill!(bold);
    fill!(italic);
    fill!(underline);
    fill!(font_color);
    fill!(vertical_align);
    fill!(cn_font);
    fill!(en_font);
    fill!(font_pt);
    fill!(align);
    fill!(first_line_indent);
    fill!(hanging_indent);
    fill!(line_spacing);
    fill!(line_spacing_rule);
    fill!(before_spacing);
    fill!(after_spacing);
    fill!(page_break_before);
    // line_spacing_rule is encoded inside the existing PlanFormat fields;
    // FormatPlan has no explicit lineSpacingRule. The renderer treats
    // line_spacing as "auto" by default. For the thesis exact line rule
    // we would need a schema extension — see RS12 known gap.
}
