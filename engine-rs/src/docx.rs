//! Raw-OOXML-first .docx writer.
//!
//! No docx crate: the hard features this engine exists for
//! (mc:AlternateContent + VML dual track, OMML, template style copy,
//! field codes) have no mature Rust library, so we emit OOXML directly.
//!
//! Coverage:
//!   RS0  FormatPlan generate path at paragraph fidelity.
//!   RS1  multi-section: sections[] + block sectionKey -> section breaks.
//!   RS2  footer (centered PAGE field) when headerFooter.pageNumber set.
//!   RS3  tables (three-line, header row) + caption.
//!   RS4  images: a real OPC relationship/parts accumulator (Package)
//!        that footer + media share; inline DrawingML picture.

use crate::analyze::SourceBlock;
use crate::autoformat;
use crate::plan::{FormatPlan, PlanBlock, PlanDocument, PlanFormat, PlanSection};
use crate::refs::{self, RefNormalize, BOOKMARK_PREFIX, DEFAULT_HANGING_CHARS, DEFAULT_TAB_POSITION};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use zip::write::SimpleFileOptions;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WP_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PIC_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const M_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";

/// OPC relationship/parts accumulator. footer and media both flow
/// through here; rIds are allocated in registration order.
struct Package {
    rels: Vec<(String, String, String)>, // (id, type, target relative to word/)
    overrides: Vec<(String, String)>,    // (partName, contentType)
    defaults: BTreeMap<String, String>,  // ext -> contentType
    parts: Vec<(String, Vec<u8>, bool)>, // (zip path, bytes, stored?)
    next_rid: u32,
    img_seq: u32,
}

impl Package {
    fn new() -> Self {
        Package {
            rels: Vec::new(),
            overrides: Vec::new(),
            defaults: BTreeMap::new(),
            parts: Vec::new(),
            next_rid: 1,
            img_seq: 0,
        }
    }

    fn add_rel(&mut self, rel_type: &str, target: &str) -> String {
        let id = format!("rId{}", self.next_rid);
        self.next_rid += 1;
        self.rels
            .push((id.clone(), rel_type.to_string(), target.to_string()));
        id
    }
}

pub fn build(
    plan: &FormatPlan,
    output: &str,
    source: Option<&HashMap<String, SourceBlock>>,
    normalize_refs: bool,
) -> std::io::Result<()> {
    let rn_owned: Option<RefNormalize> = if normalize_refs {
        let r = RefNormalize::collect(plan);
        if r.is_active() { Some(r) } else { None }
    } else {
        None
    };
    let rn: Option<&RefNormalize> = rn_owned.as_ref();

    let mut pkg = Package::new();

    // Footer first so it keeps rId1 (parity with the RS2 layout).
    let footer_rid = if let Some(footer_xml) = footer_part(&plan.document) {
        let rid = pkg.add_rel(&format!("{R_NS}/footer"), "footer1.xml");
        pkg.overrides.push((
            "/word/footer1.xml".to_string(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml".to_string(),
        ));
        pkg.parts
            .push(("word/footer1.xml".to_string(), footer_xml.into_bytes(), false));
        Some(rid)
    } else {
        None
    };

    let document_xml = render_document(plan, &mut pkg, footer_rid.as_deref(), source, rn);

    let file = std::fs::File::create(output)?;
    let mut zip = zip::ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default();

    zip.start_file("[Content_Types].xml", stored)?;
    zip.write_all(content_types(&pkg).as_bytes())?;

    zip.start_file("_rels/.rels", stored)?;
    zip.write_all(ROOT_RELS.as_bytes())?;

    zip.start_file("word/document.xml", deflated)?;
    zip.write_all(document_xml.as_bytes())?;

    if !pkg.rels.is_empty() {
        zip.start_file("word/_rels/document.xml.rels", stored)?;
        zip.write_all(document_rels(&pkg).as_bytes())?;
    }

    for (path, bytes, is_stored) in &pkg.parts {
        zip.start_file(path, if *is_stored { stored } else { deflated })?;
        zip.write_all(bytes)?;
    }

    zip.finish()?;
    Ok(())
}

fn content_types(pkg: &Package) -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
    );
    for (ext, ct) in &pkg.defaults {
        s.push_str(&format!(
            r#"<Default Extension="{}" ContentType="{}"/>"#,
            xml_escape(ext),
            xml_escape(ct)
        ));
    }
    s.push_str(r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#);
    for (part, ct) in &pkg.overrides {
        s.push_str(&format!(
            r#"<Override PartName="{}" ContentType="{}"/>"#,
            xml_escape(part),
            xml_escape(ct)
        ));
    }
    s.push_str("</Types>");
    s
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn document_rels(pkg: &Package) -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for (id, rtype, target) in &pkg.rels {
        s.push_str(&format!(
            r#"<Relationship Id="{}" Type="{}" Target="{}"/>"#,
            xml_escape(id),
            xml_escape(rtype),
            xml_escape(target)
        ));
    }
    s.push_str("</Relationships>");
    s
}

/// Centered PAGE-field footer. Mirrors .NET NeedsPageNumberFooter.
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

/// <w:tbl> is a <w:p> sibling and cannot host a sectPr.
enum BodyElem {
    Para(Para),
    Raw(String),
}

fn render_document(
    plan: &FormatPlan,
    pkg: &mut Package,
    footer_rid: Option<&str>,
    source: Option<&HashMap<String, SourceBlock>>,
    rn: Option<&RefNormalize>,
) -> String {
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
        let section: Option<&PlanSection> = find_section(key);
        let mut elems: Vec<BodyElem> = Vec::new();
        for b in blocks {
            elems.extend(render_block(&plan.document, b, pkg, source, rn));
        }

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
<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:wp="{WP_NS}" xmlns:a="{A_NS}" xmlns:pic="{PIC_NS}" xmlns:m="{M_NS}"><w:body>{body}</w:body></w:document>"#
    )
}

fn render_block(
    doc: &PlanDocument,
    block: &PlanBlock,
    pkg: &mut Package,
    source: Option<&HashMap<String, SourceBlock>>,
    rn: Option<&RefNormalize>,
) -> Vec<BodyElem> {
    // Overlay: if ref + source available, reuse source paragraph (text +
    // heading level); plan.format is applied on top. Mirrors .NET
    // FormatPlanCompiler overlay at paragraph fidelity (per-run reuse TBD).
    let overlay = block
        .r#ref
        .as_deref()
        .zip(source)
        .and_then(|(r, m)| m.get(r));

    let mut role = block.role.trim().to_ascii_lowercase();
    if let Some(sb) = overlay {
        if let Some(lvl) = sb.heading_level {
            role = match lvl {
                1 => "heading1".into(),
                2 => "heading2".into(),
                3 => "heading3".into(),
                _ => role,
            };
        }
    }

    if role == "figure" || block.image.is_some() {
        let mut out = Vec::new();
        if let Some(p) = image_paragraph(block, pkg) {
            out.push(BodyElem::Para(p));
        }
        if let Some(cap) = block.caption.as_deref().filter(|c| !c.trim().is_empty()) {
            out.push(BodyElem::Para(caption_para(doc, cap)));
        }
        return out;
    }

    if role == "equation" || block.equation.is_some() {
        return vec![BodyElem::Para(equation_para(doc, block))];
    }

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
    let text = if let Some(sb) = overlay {
        sb.text.clone()
    } else {
        block.text.clone().unwrap_or_else(|| {
            if role == "caption" {
                block.caption.clone().unwrap_or_default()
            } else {
                String::new()
            }
        })
    };

    // Reference normalization (RS9): rewrite "[n] content" as
    // bookmarkStart + "[n]" + bookmarkEnd + tab + content, augmenting
    // the pPr with default hanging indent + a tab stop when absent.
    if let Some(rn) = rn {
        let is_ref = role == "reference" || refs::split_leading_ref(&text).is_some();
        if is_ref {
            if let Some((n, content)) = refs::split_leading_ref(&text) {
                if let Some(&bid) = rn.refs.get(&n) {
                    let rpr = run_properties(block.format.as_ref(), doc, None, false);
                    let mut runs = String::new();
                    runs.push_str(&format!(
                        r#"<w:bookmarkStart w:id="{bid}" w:name="{BOOKMARK_PREFIX}{n}"/>"#
                    ));
                    runs.push_str(&format!(
                        r#"<w:r>{rpr}<w:t xml:space="preserve">[{n}]</w:t></w:r>"#
                    ));
                    runs.push_str(&format!(r#"<w:bookmarkEnd w:id="{bid}"/>"#));
                    runs.push_str(&format!(r#"<w:r>{rpr}<w:tab/></w:r>"#));
                    if !content.is_empty() {
                        runs.push_str(&format!(
                            r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
                            xml_escape(&content)
                        ));
                    }
                    let mut ppr_inner = paragraph_properties_inner(block.format.as_ref(), doc);
                    if !ppr_inner.contains("<w:ind") {
                        ppr_inner.push_str(&format!(
                            r#"<w:ind w:hangingChars="{DEFAULT_HANGING_CHARS}"/>"#
                        ));
                    }
                    if !ppr_inner.contains("<w:tabs") {
                        ppr_inner.push_str(&format!(
                            r#"<w:tabs><w:tab w:val="left" w:pos="{DEFAULT_TAB_POSITION}"/></w:tabs>"#
                        ));
                    }
                    return vec![BodyElem::Para(Para { ppr_inner, run: runs })];
                }
            }
        }
    }

    let ppr_inner = paragraph_properties_inner(block.format.as_ref(), doc);
    let rpr = run_properties(block.format.as_ref(), doc, heading_level, role == "title");

    // Overlay with per-run fidelity: emit one <w:r> per source run so the
    // source's inline bold/italic/sub-superscript/tab survive reformat.
    // plan.format is intentionally NOT applied at run level on overlay
    // (matches .NET, which lets source.Runs pass through unmodified).
    let run = if let Some(sb) = overlay.filter(|s| !s.runs.is_empty()) {
        let mut s = String::new();
        for sr in &sb.runs {
            if sr.kind.as_deref() == Some("tab") {
                s.push_str(&format!("<w:r>{rpr}<w:tab/></w:r>"));
                continue;
            }
            if sr.text.is_empty() {
                continue;
            }
            let mut f = PlanFormat::default();
            if sr.bold {
                f.bold = Some(true);
            }
            if sr.italic {
                f.italic = Some(true);
            }
            f.vertical_align = sr.vertical_align.clone();
            let rp = run_properties(Some(&f), doc, heading_level, role == "title");
            s.push_str(&format!(
                r#"<w:r>{rp}<w:t xml:space="preserve">{}</w:t></w:r>"#,
                xml_escape(&sr.text)
            ));
        }
        s
    } else if role == "pagenumber" {
        format!(r#"<w:fldSimple w:instr=" PAGE "><w:r>{rpr}<w:t>1</w:t></w:r></w:fldSimple>"#)
    } else {
        render_text_segments(&text, block.format.as_ref(), doc, heading_level, role == "title", rn)
    };

    vec![BodyElem::Para(Para { ppr_inner, run })]
}

/// Inline DrawingML picture. Mirrors .NET BuildImageParagraph: a missing
/// or unreadable file yields no paragraph (caption still emitted).
fn image_paragraph(block: &PlanBlock, pkg: &mut Package) -> Option<Para> {
    let img = block.image.as_ref()?;
    let path = img.path.as_deref()?;
    let bytes = std::fs::read(path).ok()?;

    let ext = image_ext(path, img.content_type.as_deref());
    let content_type = image_content_type(ext);
    pkg.defaults
        .entry(ext.to_string())
        .or_insert_with(|| content_type.to_string());

    pkg.img_seq += 1;
    let media_name = format!("media/image{}.{}", pkg.img_seq, ext);
    pkg.parts
        .push((format!("word/{media_name}"), bytes, true));
    let rid = pkg.add_rel(&format!("{R_NS}/image"), &media_name);

    let w = img.width_emu.unwrap_or(4_000_000);
    let h = img.height_emu.unwrap_or(3_000_000);
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    let alt = img.alt_text.clone().unwrap_or_else(|| name.clone());
    let name = xml_escape(&name);
    let alt = xml_escape(&alt);

    let drawing = format!(
        concat!(
            r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0">"#,
            r#"<wp:extent cx="{w}" cy="{h}"/>"#,
            r#"<wp:effectExtent l="0" t="0" r="0" b="0"/>"#,
            r#"<wp:docPr id="1" name="{name}" descr="{alt}"/>"#,
            r#"<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr>"#,
            r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="{name}" descr="{alt}"/><pic:cNvPicPr/></pic:nvPicPr>"#,
            r#"<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
            r#"</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#,
        ),
        w = w,
        h = h,
        name = name,
        alt = alt,
        rid = rid
    );

    Some(Para {
        ppr_inner: String::new(),
        run: format!("<w:r>{drawing}</w:r>"),
    })
}

fn image_ext<'a>(path: &'a str, content_type: Option<&'a str>) -> &'a str {
    let by_ct = content_type.map(|c| c.to_ascii_lowercase());
    match by_ct.as_deref() {
        Some("image/png") => return "png",
        Some("image/jpeg") | Some("image/jpg") => return "jpeg",
        Some("image/gif") => return "gif",
        Some("image/bmp") => return "bmp",
        Some("image/tiff") => return "tiff",
        Some("image/x-emf") => return "emf",
        Some("image/x-wmf") => return "wmf",
        _ => {}
    }
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "png",
        Some("jpg") | Some("jpeg") => "jpeg",
        Some("gif") => "gif",
        Some("bmp") => "bmp",
        Some("tif") | Some("tiff") => "tiff",
        Some("emf") => "emf",
        Some("wmf") => "wmf",
        _ => "png",
    }
}

fn image_content_type(ext: &str) -> &'static str {
    match ext {
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tiff" => "image/tiff",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        _ => "image/png",
    }
}

/// Equation paragraph. Mirrors .NET BuildEquationParagraph: when xml is
/// supplied, embed it raw (caller-provided OMML); otherwise render the
/// text fallback. Display mode centers the paragraph. OMML height is
/// content-driven so line-spacing is intentionally skipped when xml is set
/// (matches the .NET comment about exact line height truncating tall math).
fn equation_para(doc: &PlanDocument, block: &PlanBlock) -> Para {
    let eq = block.equation.as_ref();
    let xml = eq
        .and_then(|e| e.xml.as_deref())
        .filter(|s| !s.trim().is_empty());
    let text = eq.and_then(|e| e.text.as_deref()).unwrap_or("");
    let display = eq
        .and_then(|e| e.display_mode.as_deref())
        .map(|s| s.eq_ignore_ascii_case("display"))
        .unwrap_or(false);

    let pf = block.format.as_ref();
    let mut ppr = String::new();

    let align = if display {
        Some("center")
    } else {
        pf.and_then(|f| f.align.as_deref())
    };
    if let Some(a) = align.and_then(map_align) {
        ppr.push_str(&format!(r#"<w:jc w:val="{a}"/>"#));
    }

    let first_line = pf.and_then(|f| f.first_line_indent.clone());
    let hanging = pf.and_then(|f| f.hanging_indent.clone());
    if first_line.is_some() || hanging.is_some() {
        let mut ind = String::from("<w:ind");
        if let Some(v) = first_line {
            ind.push_str(&format!(r#" w:firstLine="{}""#, xml_escape(&v)));
        }
        if let Some(v) = hanging {
            ind.push_str(&format!(r#" w:hanging="{}""#, xml_escape(&v)));
        }
        ind.push_str("/>");
        ppr.push_str(&ind);
    }

    let before = pf.and_then(|f| f.before_spacing.clone());
    let after = pf.and_then(|f| f.after_spacing.clone());
    let line = if xml.is_some() {
        None
    } else {
        pf.and_then(|f| f.line_spacing.clone())
            .or_else(|| doc.line_spacing.map(|ls| ((240.0 * ls).round() as i64).to_string()))
    };
    if before.is_some() || after.is_some() || line.is_some() {
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
        ppr.push_str(&sp);
    }

    let run = if let Some(x) = xml {
        // Insert OMML XML as-is. The m: namespace prefix is declared on
        // the document root, so the caller-supplied element resolves.
        x.to_string()
    } else if !text.is_empty() {
        let rpr = run_properties(None, doc, None, false);
        format!(
            r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
            xml_escape(text)
        )
    } else {
        // Empty equation: emit an empty run so the paragraph is well-formed.
        let rpr = run_properties(None, doc, None, false);
        format!("<w:r>{rpr}</w:r>")
    };

    Para {
        ppr_inner: ppr,
        run,
    }
}

fn caption_para(doc: &PlanDocument, text: &str) -> Para {
    Para {
        ppr_inner: r#"<w:jc w:val="center"/>"#.to_string(),
        run: render_text_segments(text, None, doc, None, false, None),
    }
}

/// Emit one or more <w:r> for a piece of generate-mode text, splitting it
/// with autoformat (H2O / m^2 / x^2 etc) so subscript/superscript flow
/// automatically. When `rn` is Some, superscript segments are scanned for
/// [n] / [n,m] / [n-m] citations and resolved into <w:fldSimple> REF
/// fields pointing at the corresponding _Ref_ref_<n> bookmark.
fn render_text_segments(
    text: &str,
    base: Option<&PlanFormat>,
    doc: &PlanDocument,
    heading: Option<u8>,
    is_title: bool,
    rn: Option<&RefNormalize>,
) -> String {
    let mut out = String::new();
    for seg in autoformat::split(text) {
        let mut overlay: PlanFormat = base.cloned().unwrap_or_default();
        if let Some(va) = &seg.vertical_align {
            overlay.vertical_align = Some(va.clone());
        }
        let rpr = run_properties(Some(&overlay), doc, heading, is_title);

        // Superscript + citation substitution (RS9 phase 2).
        if let Some(rn) = rn {
            if seg.vertical_align.as_deref() == Some("superscript") {
                if let Some(pieces) = refs::split_super_for_citations(&seg.text, &rn.refs) {
                    for piece in pieces {
                        match piece {
                            refs::CitPiece::Text(s) if s.is_empty() => {}
                            refs::CitPiece::Text(s) => out.push_str(&format!(
                                r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
                                xml_escape(&s)
                            )),
                            refs::CitPiece::Ref(n) => out.push_str(&format!(
                                r#"<w:fldSimple w:instr=" REF {BOOKMARK_PREFIX}{n} \h "><w:r>{rpr}<w:t>{n}</w:t></w:r></w:fldSimple>"#
                            )),
                        }
                    }
                    continue;
                }
            }
        }

        out.push_str(&format!(
            r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
            xml_escape(&seg.text)
        ));
    }
    if out.is_empty() {
        let rpr = run_properties(base, doc, heading, is_title);
        out.push_str(&format!("<w:r>{rpr}</w:r>"));
    }
    out
}

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
            let base_fmt = if is_header {
                let mut f = PlanFormat::default();
                f.bold = Some(true);
                Some(f)
            } else {
                None
            };
            let runs = render_text_segments(cell, base_fmt.as_ref(), doc, None, false, None);
            let tc_borders = if is_header {
                r#"<w:tcBorders><w:bottom w:val="single" w:sz="6"/></w:tcBorders>"#
            } else {
                ""
            };
            body.push_str(&format!(
                r#"<w:tc><w:tcPr><w:tcW w:type="auto"/>{tc_borders}</w:tcPr><w:p>{runs}</w:p></w:tc>"#
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
