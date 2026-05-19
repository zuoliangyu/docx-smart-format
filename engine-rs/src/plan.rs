//! FormatPlan contract (Rust mirror of engine/src/FormatPlan.cs).
//! Only the fields the current vertical slice consumes are wired; the rest
//! parse-and-ignore so the JSON contract stays shared with the .NET engine.

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FormatPlan {
    pub doc_type: Option<String>,
    pub document: PlanDocument,
    pub sections: Vec<PlanSection>,
    pub blocks: Vec<PlanBlock>,
}

#[derive(Debug, Deserialize, Default)]
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
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanMargins {
    pub top: Option<String>,
    pub bottom: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanHeaderFooter {
    pub odd_even: bool,
    pub first_different: bool,
    pub page_number: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
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

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanBlock {
    pub r#ref: Option<String>,
    pub role: String,
    pub section_key: Option<String>,
    pub text: Option<String>,
    pub caption: Option<String>,
    pub format: Option<PlanFormat>,
    pub table: Option<PlanTable>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanTable {
    pub rows: Vec<Vec<String>>,
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
    pub hanging_indent: Option<String>,
    pub line_spacing: Option<String>,
    pub before_spacing: Option<String>,
    pub after_spacing: Option<String>,
    pub page_break_before: Option<bool>,
}
