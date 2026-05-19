//! Raw-OOXML-first .docx writer.
//!
//! No docx crate: the hard features this engine exists for
//! (mc:AlternateContent + VML dual track, OMML, template style copy,
//! field codes) have no mature Rust library, so we emit OOXML directly
//! and keep full control. This slice covers the FormatPlan generate path
//! at paragraph fidelity; sections/objects/headers come next.

use crate::plan::{FormatPlan, PlanBlock, PlanDocument, PlanFormat};
use std::io::Write;
use zip::write::SimpleFileOptions;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

pub fn build(plan: &FormatPlan, output: &str) -> std::io::Result<()> {
    let document_xml = render_document(plan);

    let file = std::fs::File::create(output)?;
    let mut zip = zip::ZipWriter::new(file);
    // Stored for [Content_Types]/.rels is conventional; deflate the body.
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default();

    zip.start_file("[Content_Types].xml", stored)?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;

    zip.start_file("_rels/.rels", stored)?;
    zip.write_all(ROOT_RELS.as_bytes())?;

    zip.start_file("word/document.xml", deflated)?;
    zip.write_all(document_xml.as_bytes())?;

    zip.finish()?;
    Ok(())
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn render_document(plan: &FormatPlan) -> String {
    let mut body = String::new();
    for block in &plan.blocks {
        body.push_str(&render_block(&plan.document, block));
    }
    body.push_str(&section_properties(&plan.document));

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="{W_NS}"><w:body>{body}</w:body></w:document>"#
    )
}

fn render_block(doc: &PlanDocument, block: &PlanBlock) -> String {
    let role = block.role.trim().to_ascii_lowercase();
    let heading_level = match role.as_str() {
        "heading1" => Some(1),
        "heading2" => Some(2),
        "heading3" => Some(3),
        _ => None,
    };
    let text = block.text.clone().unwrap_or_else(|| {
        if role == "caption" {
            block.caption.clone().unwrap_or_default()
        } else {
            String::new()
        }
    });

    let ppr = paragraph_properties(block.format.as_ref(), doc);
    let rpr = run_properties(block.format.as_ref(), doc, heading_level, role == "title");

    let run = if role == "pagenumber" {
        format!(
            r#"<w:fldSimple w:instr=" PAGE "><w:r>{rpr}<w:t>1</w:t></w:r></w:fldSimple>"#
        )
    } else {
        format!(
            r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
            xml_escape(&text)
        )
    };

    format!("<w:p>{ppr}{run}</w:p>")
}

fn paragraph_properties(fmt: Option<&PlanFormat>, doc: &PlanDocument) -> String {
    let mut inner = String::new();

    if let Some(a) = fmt.and_then(|f| f.align.as_deref()).and_then(map_align) {
        inner.push_str(&format!(r#"<w:jc w:val="{a}"/>"#));
    }

    let first_line = fmt.and_then(|f| f.first_line_indent.clone());
    let hanging = fmt.and_then(|f| f.hanging_indent.clone());
    if first_line.is_some() || hanging.is_some() {
        let mut ind = String::from("<w:ind");
        if let Some(v) = first_line {
            ind.push_str(&format!(r#" w:firstLine="{}""#, xml_escape(&v)));
        }
        if let Some(v) = hanging {
            ind.push_str(&format!(r#" w:hanging="{}""#, xml_escape(&v)));
        }
        ind.push_str("/>");
        inner.push_str(&ind);
    }

    let line = fmt
        .and_then(|f| f.line_spacing.clone())
        .or_else(|| doc.line_spacing.map(|ls| ((240.0 * ls).round() as i64).to_string()));
    let before = fmt.and_then(|f| f.before_spacing.clone());
    let after = fmt.and_then(|f| f.after_spacing.clone());
    if line.is_some() || before.is_some() || after.is_some() {
        let mut sp = String::from("<w:spacing");
        if let Some(v) = before {
            sp.push_str(&format!(r#" w:before="{}""#, xml_escape(&v)));
        }
        if let Some(v) = after {
            sp.push_str(&format!(r#" w:after="{}""#, xml_escape(&v)));
        }
        if let Some(v) = line {
            sp.push_str(&format!(r#" w:line="{}" w:lineRule="auto""#, xml_escape(&v)));
        }
        sp.push_str("/>");
        inner.push_str(&sp);
    }

    if fmt.and_then(|f| f.page_break_before).unwrap_or(false) {
        inner.push_str("<w:pageBreakBefore/>");
    }

    if inner.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{inner}</w:pPr>")
    }
}

fn run_properties(
    fmt: Option<&PlanFormat>,
    doc: &PlanDocument,
    heading_level: Option<u8>,
    is_title: bool,
) -> String {
    let ascii = fmt
        .and_then(|f| f.en_font.clone())
        .or_else(|| doc.en_font.clone())
        .unwrap_or_else(|| "Times New Roman".to_string());
    let east = fmt
        .and_then(|f| f.cn_font.clone())
        .or_else(|| doc.cn_font.clone())
        .unwrap_or_else(|| "宋体".to_string());

    let mut inner = format!(
        r#"<w:rFonts w:ascii="{a}" w:hAnsi="{a}" w:eastAsia="{e}" w:cs="{e}"/>"#,
        a = xml_escape(&ascii),
        e = xml_escape(&east)
    );

    let bold = fmt.and_then(|f| f.bold).unwrap_or(false) || heading_level.is_some() || is_title;
    if bold {
        inner.push_str("<w:b/>");
    }
    if fmt.and_then(|f| f.italic).unwrap_or(false) {
        inner.push_str("<w:i/>");
    }
    if let Some(u) = fmt.and_then(|f| f.underline.as_deref()).and_then(map_underline) {
        inner.push_str(&format!(r#"<w:u w:val="{u}"/>"#));
    }
    if let Some(c) = fmt.and_then(|f| f.font_color.as_deref()) {
        inner.push_str(&format!(r#"<w:color w:val="{}"/>"#, xml_escape(c)));
    }
    if let Some(va) = fmt
        .and_then(|f| f.vertical_align.as_deref())
        .and_then(map_vertical_align)
    {
        inner.push_str(&format!(r#"<w:vertAlign w:val="{va}"/>"#));
    }

    let sz = fmt
        .and_then(|f| f.font_pt)
        .map(half_point)
        .or_else(|| doc.base_font_pt.map(half_point))
        .unwrap_or_else(|| fallback_heading_size(heading_level));
    inner.push_str(&format!(
        r#"<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/>"#
    ));

    format!("<w:rPr>{inner}</w:rPr>")
}

fn section_properties(doc: &PlanDocument) -> String {
    let (w, h) = page_size_twips(doc.page_size.as_deref());
    let orient = match doc.orientation.as_deref() {
        Some("landscape") => r#" w:orient="landscape""#,
        _ => "",
    };
    let m = doc.margins.as_ref();
    let top = m.and_then(|x| x.top.clone()).unwrap_or_else(|| "1440".into());
    let bottom = m.and_then(|x| x.bottom.clone()).unwrap_or_else(|| "1440".into());
    let left = m.and_then(|x| x.left.clone()).unwrap_or_else(|| "1800".into());
    let right = m.and_then(|x| x.right.clone()).unwrap_or_else(|| "1800".into());
    format!(
        r#"<w:sectPr><w:pgSz w:w="{w}" w:h="{h}"{orient}/><w:pgMar w:top="{}" w:bottom="{}" w:left="{}" w:right="{}" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#,
        xml_escape(&top),
        xml_escape(&bottom),
        xml_escape(&left),
        xml_escape(&right)
    )
}

fn page_size_twips(size: Option<&str>) -> (&'static str, &'static str) {
    match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("letter") => ("12240", "15840"),
        Some("legal") => ("12240", "20160"),
        Some("a5") => ("8391", "11906"),
        _ => ("11906", "16838"),
    }
}

fn half_point(pt: f64) -> String {
    ((pt * 2.0).round() as i64).to_string()
}

fn fallback_heading_size(level: Option<u8>) -> String {
    match level {
        Some(1) => "36",
        Some(2) => "32",
        Some(3) => "28",
        _ => "24",
    }
    .to_string()
}

fn map_align(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "center" => Some("center"),
        "right" => Some("right"),
        "both" | "justify" => Some("both"),
        "distribute" => Some("distribute"),
        "left" => Some("left"),
        _ => None,
    }
}

fn map_underline(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "single" => Some("single"),
        "double" => Some("double"),
        "none" => Some("none"),
        _ => None,
    }
}

fn map_vertical_align(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "superscript" | "super" | "上标" => Some("superscript"),
        "subscript" | "sub" | "下标" => Some("subscript"),
        "baseline" | "normal" | "基线" => Some("baseline"),
        _ => None,
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}
