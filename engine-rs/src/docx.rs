//! Raw-OOXML-first .docx writer.
//!
//! No docx crate: the hard features this engine exists for
//! (mc:AlternateContent + VML dual track, OMML, template style copy,
//! field codes) have no mature Rust library, so we emit OOXML directly
//! and keep full control.
//!
//! Coverage:
//!   RS0  FormatPlan generate path at paragraph fidelity.
//!   RS1  multi-section: sections[] + block sectionKey -> section breaks.
//!   RS2  OPC relationship/parts machinery + a centered PAGE-field footer
//!        when document.headerFooter.pageNumber is set. The package
//!        builder here is what RS4 (images) will reuse.

use crate::plan::{FormatPlan, PlanBlock, PlanDocument, PlanFormat, PlanSection};
use std::io::Write;
use zip::write::SimpleFileOptions;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const FOOTER_RID: &str = "rId1";

pub fn build(plan: &FormatPlan, output: &str) -> std::io::Result<()> {
    let footer_xml = footer_part(&plan.document);
    let has_footer = footer_xml.is_some();

    let document_xml = render_document(plan, has_footer);

    let file = std::fs::File::create(output)?;
    let mut zip = zip::ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default();

    zip.start_file("[Content_Types].xml", stored)?;
    zip.write_all(content_types(has_footer).as_bytes())?;

    zip.start_file("_rels/.rels", stored)?;
    zip.write_all(ROOT_RELS.as_bytes())?;

    zip.start_file("word/document.xml", deflated)?;
    zip.write_all(document_xml.as_bytes())?;

    if let Some(footer) = footer_xml {
        zip.start_file("word/footer1.xml", deflated)?;
        zip.write_all(footer.as_bytes())?;

        zip.start_file("word/_rels/document.xml.rels", stored)?;
        zip.write_all(document_rels().as_bytes())?;
    }

    zip.finish()?;
    Ok(())
}

fn content_types(has_footer: bool) -> String {
    let footer_override = if has_footer {
        r#"<Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{footer_override}</Types>"#
    )
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn document_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="{FOOTER_RID}" Type="{R_NS}/footer" Target="footer1.xml"/></Relationships>"#
    )
}

/// Centered PAGE-field footer, emitted when pageNumber qualifies.
/// Mirrors .NET FormatPlanCompiler.NeedsPageNumberFooter.
fn footer_part(doc: &PlanDocument) -> Option<String> {
    let v = doc
        .header_footer
        .as_ref()
        .and_then(|hf| hf.page_number.as_deref())
        .map(|s| s.trim().to_ascii_lowercase());
    match v.as_deref() {
        Some("continuous") | Some("center-page-number") => {
            let rpr = run_properties(None, doc, None, false);
            Some(format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="{W_NS}"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:fldSimple w:instr=" PAGE "><w:r>{rpr}<w:t>1</w:t></w:r></w:fldSimple></w:p></w:ftr>"#
            ))
        }
        _ => None,
    }
}

/// A built paragraph kept structured so a section break can be injected
/// into the last paragraph's pPr (mirrors .NET AttachSectionBreak).
struct Para {
    ppr_inner: String,
    run: String,
}

impl Para {
    fn serialize(&self, extra_ppr: &str) -> String {
        let ppr = format!("{}{}", self.ppr_inner, extra_ppr);
        let ppr = if ppr.is_empty() {
            String::new()
        } else {
            format!("<w:pPr>{ppr}</w:pPr>")
        };
        format!("<w:p>{ppr}{}</w:p>", self.run)
    }
}

/// Body-level element. A table (`<w:tbl>`) is a sibling of `<w:p>` and
/// cannot host a sectPr, so section-break injection must fall back to a
/// trailing empty paragraph (mirrors .NET AttachSectionBreak).
enum BodyElem {
    Para(Para),
    Raw(String),
}

fn render_document(plan: &FormatPlan, has_footer: bool) -> String {
    let footer_rid = if has_footer { Some(FOOTER_RID) } else { None };

    let mut groups: Vec<(Option<String>, Vec<&PlanBlock>)> = Vec::new();
    for block in &plan.blocks {
        let key = block.section_key.clone();
        match groups.last_mut() {
            Some((k, v)) if *k == key => v.push(block),
            _ => groups.push((key, vec![block])),
        }
    }
    if groups.is_empty() {
        groups.push((None, Vec::new()));
    }

    let find_section = |key: &Option<String>| -> Option<&PlanSection> {
        key.as_ref()
            .and_then(|k| plan.sections.iter().find(|s| &s.key == k))
    };

    let mut body = String::new();
    let last_idx = groups.len() - 1;
    for (gi, (key, blocks)) in groups.iter().enumerate() {
        let section = find_section(key);
        let mut elems: Vec<BodyElem> = blocks
            .iter()
            .flat_map(|b| render_block(&plan.document, b))
            .collect();

        if gi == last_idx {
            for e in &elems {
                match e {
                    BodyElem::Para(p) => body.push_str(&p.serialize("")),
                    BodyElem::Raw(s) => body.push_str(s),
                }
            }
            body.push_str(&format!(
                "<w:sectPr>{}</w:sectPr>",
                section_inner(section, &plan.document, false, footer_rid)
            ));
        } else {
            let sect = format!(
                "<w:sectPr>{}</w:sectPr>",
                section_inner(section, &plan.document, true, footer_rid)
            );
            // sectPr must live in a paragraph. If the section ends in a
            // table (or is empty), append a trailing empty paragraph.
            let host_in_last_para = matches!(elems.last(), Some(BodyElem::Para(_)));
            if !host_in_last_para {
                elems.push(BodyElem::Para(Para {
                    ppr_inner: String::new(),
                    run: "<w:r/>".to_string(),
                }));
            }
            let n = elems.len();
            for (ei, e) in elems.iter().enumerate() {
                match e {
                    BodyElem::Para(p) if ei == n - 1 => body.push_str(&p.serialize(&sect)),
                    BodyElem::Para(p) => body.push_str(&p.serialize("")),
                    BodyElem::Raw(s) => body.push_str(s),
                }
            }
        }
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}"><w:body>{body}</w:body></w:document>"#
    )
}

fn render_block(doc: &PlanDocument, block: &PlanBlock) -> Vec<BodyElem> {
    let role = block.role.trim().to_ascii_lowercase();

    // Table block: emit <w:tbl> + optional centered caption paragraph.
    if role == "table" || block.table.is_some() {
        let rows = block
            .table
            .as_ref()
            .map(|t| t.rows.clone())
            .unwrap_or_default();
        let mut out = vec![BodyElem::Raw(build_table(doc, &rows))];
        if let Some(cap) = block.caption.as_deref().filter(|c| !c.trim().is_empty()) {
            out.push(BodyElem::Para(caption_para(doc, cap)));
        }
        return out;
    }

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

    let ppr_inner = paragraph_properties_inner(block.format.as_ref(), doc);
    let rpr = run_properties(block.format.as_ref(), doc, heading_level, role == "title");

    let run = if role == "pagenumber" {
        format!(r#"<w:fldSimple w:instr=" PAGE "><w:r>{rpr}<w:t>1</w:t></w:r></w:fldSimple>"#)
    } else {
        format!(
            r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
            xml_escape(&text)
        )
    };

    vec![BodyElem::Para(Para { ppr_inner, run })]
}

fn caption_para(doc: &PlanDocument, text: &str) -> Para {
    let rpr = run_properties(None, doc, None, false);
    Para {
        ppr_inner: r#"<w:jc w:val="center"/>"#.to_string(),
        run: format!(
            r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
            xml_escape(text)
        ),
    }
}

/// Mirrors .NET DocxRenderer.BuildTable: full-width centered three-line
/// table (top + bottom rule, no side/inside borders); first row is a
/// header when there is more than one row (bold + bottom rule).
fn build_table(doc: &PlanDocument, rows: &[Vec<String>]) -> String {
    let tbl_pr = concat!(
        r#"<w:tblPr>"#,
        r#"<w:tblW w:w="5000" w:type="pct"/>"#,
        r#"<w:jc w:val="center"/>"#,
        r#"<w:tblBorders>"#,
        r#"<w:top w:val="single" w:sz="12"/>"#,
        r#"<w:bottom w:val="single" w:sz="12"/>"#,
        r#"<w:left w:val="none" w:sz="0"/>"#,
        r#"<w:right w:val="none" w:sz="0"/>"#,
        r#"<w:insideH w:val="none" w:sz="0"/>"#,
        r#"<w:insideV w:val="none" w:sz="0"/>"#,
        r#"</w:tblBorders>"#,
        r#"</w:tblPr>"#,
    );

    let mut body = String::new();
    let row_count = rows.len();
    for (ri, row) in rows.iter().enumerate() {
        let is_header = ri == 0 && row_count > 1;
        body.push_str("<w:tr>");
        for cell in row {
            let rpr = if is_header {
                let mut f = PlanFormat::default();
                f.bold = Some(true);
                run_properties(Some(&f), doc, None, false)
            } else {
                run_properties(None, doc, None, false)
            };
            let tc_borders = if is_header {
                r#"<w:tcBorders><w:bottom w:val="single" w:sz="6"/></w:tcBorders>"#
            } else {
                ""
            };
            body.push_str(&format!(
                r#"<w:tc><w:tcPr><w:tcW w:type="auto"/>{tc_borders}</w:tcPr><w:p><w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
                xml_escape(cell)
            ));
        }
        body.push_str("</w:tr>");
    }

    if row_count == 0 {
        body.push_str(
            r#"<w:tr><w:tc><w:tcPr><w:tcW w:type="auto"/></w:tcPr><w:p><w:r><w:t></w:t></w:r></w:p></w:tc></w:tr>"#,
        );
    }

    format!("<w:tbl>{tbl_pr}{body}</w:tbl>")
}

fn paragraph_properties_inner(fmt: Option<&PlanFormat>, doc: &PlanDocument) -> String {
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

    inner
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
    inner.push_str(&format!(r#"<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/>"#));

    format!("<w:rPr>{inner}</w:rPr>")
}

/// sectPr children, in OOXML-mandated order: footerReference must precede
/// w:type / w:pgSz. `is_break` => non-final section (honor section type).
fn section_inner(
    section: Option<&PlanSection>,
    doc: &PlanDocument,
    is_break: bool,
    footer_rid: Option<&str>,
) -> String {
    let mut out = String::new();

    if let Some(rid) = footer_rid {
        out.push_str(&format!(
            r#"<w:footerReference w:type="default" r:id="{rid}"/>"#
        ));
    }

    if is_break {
        if let Some(t) = section
            .and_then(|s| s.r#type.as_deref())
            .and_then(map_section_type)
        {
            out.push_str(&format!(r#"<w:type w:val="{t}"/>"#));
        }
    }

    let orientation = section
        .and_then(|s| s.orientation.clone())
        .or_else(|| doc.orientation.clone());
    let (w, h) = page_size_twips(doc.page_size.as_deref());
    let orient_attr = match orientation.as_deref() {
        Some("landscape") => r#" w:orient="landscape""#,
        _ => "",
    };

    let dm = doc.margins.as_ref();
    let sm = section.and_then(|s| s.margins.as_ref());
    let pick = |f: fn(&crate::plan::PlanMargins) -> Option<String>, def: &str| -> String {
        sm.and_then(f)
            .or_else(|| dm.and_then(f))
            .unwrap_or_else(|| def.to_string())
    };
    let top = pick(|m| m.top.clone(), "1440");
    let bottom = pick(|m| m.bottom.clone(), "1440");
    let left = pick(|m| m.left.clone(), "1800");
    let right = pick(|m| m.right.clone(), "1800");

    out.push_str(&format!(r#"<w:pgSz w:w="{w}" w:h="{h}"{orient_attr}/>"#));
    out.push_str(&format!(
        r#"<w:pgMar w:top="{}" w:bottom="{}" w:left="{}" w:right="{}" w:header="720" w:footer="720" w:gutter="0"/>"#,
        xml_escape(&top),
        xml_escape(&bottom),
        xml_escape(&left),
        xml_escape(&right)
    ));

    if let Some(s) = section {
        let fmt_attr = s
            .page_num_fmt
            .as_deref()
            .and_then(map_page_num_fmt)
            .map(|f| format!(r#" w:fmt="{f}""#))
            .unwrap_or_default();
        match s.page_start {
            Some(start) => {
                out.push_str(&format!(r#"<w:pgNumType{fmt_attr} w:start="{start}"/>"#))
            }
            None if !fmt_attr.is_empty() => {
                out.push_str(&format!(r#"<w:pgNumType{fmt_attr}/>"#))
            }
            None => {}
        }
        if s.title_page {
            out.push_str("<w:titlePg/>");
        }
    }

    out
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

fn map_section_type(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "continuous" => Some("continuous"),
        "evenpage" => Some("evenPage"),
        "oddpage" => Some("oddPage"),
        "nextcolumn" => Some("nextColumn"),
        "nextpage" => Some("nextPage"),
        _ => None,
    }
}

fn map_page_num_fmt(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "decimal" => Some("decimal"),
        "upperroman" => Some("upperRoman"),
        "lowerroman" => Some("lowerRoman"),
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
