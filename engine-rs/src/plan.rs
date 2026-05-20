//! FormatPlan contract (Rust mirror of engine/src/FormatPlan.cs).
//! Only the fields the current vertical slice consumes are wired; the rest
//! parse-and-ignore so the JSON contract stays shared with the .NET engine.

use serde::Deserialize;

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct FormatPlan {
    pub doc_type: Option<String>,
    pub document: PlanDocument,
    pub sections: Vec<PlanSection>,
    pub blocks: Vec<PlanBlock>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanDocument {
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_size: Option<String>,
    pub orientation: Option<String>,
    pub margins: Option<PlanMargins>,
    pub cn_font: Option<String>,
    pub en_font: Option<String>,
    pub base_font_pt: Option<f64>,
    pub line_spacing: Option<f64>,
    pub header_footer: Option<PlanHeaderFooter>,

    /// Optional preset that fills the plan with thesis/report defaults
    /// (.NET BuiltinTemplateFactory port). Supported: "undergraduate-thesis".
    pub preset: Option<String>,
    /// Used by undergraduate-thesis preset for the odd-page header text.
    pub thesis_university: Option<String>,
    /// Used by undergraduate-thesis preset for the even-page header text.
    pub thesis_title: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanMargins {
    pub top: Option<String>,
    pub bottom: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanHeaderFooter {
    pub odd_even: bool,
    pub first_different: bool,
    pub page_number: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanSection {
    pub key: String,
    pub r#type: Option<String>,
    pub orientation: Option<String>,
    pub margins: Option<PlanMargins>,
    pub page_start: Option<i32>,
    pub page_num_fmt: Option<String>,
    pub title_page: bool,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanBlock {
    pub r#ref: Option<String>,
    pub role: String,
    pub section_key: Option<String>,
    pub text: Option<String>,
    pub caption: Option<String>,
    pub format: Option<PlanFormat>,
    pub table: Option<PlanTable>,
    pub image: Option<PlanImage>,
    pub equation: Option<PlanEquation>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanEquation {
    pub text: Option<String>,
    /// Raw OMML (`<m:oMath>...</m:oMath>` or a full `<m:oMathPara>`).
    /// Inserted as-is; the m: namespace prefix is declared on the document
    /// root by the engine.
    pub xml: Option<String>,
    /// "inline" (default) or "display".
    pub display_mode: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanTable {
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanImage {
    pub path: Option<String>,
    pub content_type: Option<String>,
    pub width_emu: Option<i64>,
    pub height_emu: Option<i64>,
    pub alt_text: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanFormat {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<String>,
    pub font_color: Option<String>,
    pub vertical_align: Option<String>,
    pub cn_font: Option<String>,
    pub en_font: Option<String>,
    pub font_pt: Option<f64>,
    pub align: Option<String>,
    pub first_line_indent: Option<String>,
    /// 首行缩进 (单位:1/100 字符宽度)。"200" = 真 2 字符,随字号伸缩。
    /// 中文规范的"首行缩进 2 字符"权威表达。优先级高于 firstLineIndent。
    pub first_line_chars: Option<String>,
    pub hanging_indent: Option<String>,
    /// 悬挂缩进 (单位:1/100 字符宽度)。参考文献条目用 "200" = 2 字符。
    pub hanging_chars: Option<String>,
    pub line_spacing: Option<String>,
    /// auto | exact | atLeast. Default auto when unset.
    pub line_spacing_rule: Option<String>,
    pub before_spacing: Option<String>,
    pub after_spacing: Option<String>,
    pub page_break_before: Option<bool>,

    /// Paragraph style id (e.g. "Heading1", "Reference"). Resolved against
    /// the styles.xml copied from `build --template`. Emitted as
    /// `<w:pStyle w:val="..."/>`.
    pub style_id: Option<String>,
}
