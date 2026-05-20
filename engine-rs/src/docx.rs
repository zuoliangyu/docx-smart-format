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
use crate::presets;
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
    template_path: Option<&str>,
) -> std::io::Result<()> {
    // Clone so a preset can fill in defaults without mutating the caller's plan.
    let mut plan = plan.clone();

    // Preset application happens before anything else — it must finish
    // before HF parts / RefNormalize / rendering, since it mutates blocks
    // and document.headerFooter.
    let preset_enabled = presets::is_undergraduate_thesis(&plan.document);
    if preset_enabled {
        presets::apply_undergraduate_thesis(&mut plan);
    }
    // The thesis preset auto-enables reference normalization (mirrors .NET
    // template-preset=builtin-undergraduate-thesis behavior). Callers can
    // still pass --normalize-references false to override.
    let normalize_refs = normalize_refs || preset_enabled;

    let rn_owned: Option<RefNormalize> = if normalize_refs {
        let r = RefNormalize::collect(&plan);
        if r.is_active() { Some(r) } else { None }
    } else {
        None
    };
    let rn: Option<&RefNormalize> = rn_owned.as_ref();

    let mut pkg = Package::new();

    // Collect every header/footer part required by the plan. Drives:
    //   - per-part OPC entry (word/header*.xml or word/footer*.xml)
    //   - per-part document relationship (so sectPr can reference it)
    //   - settings.xml emission when any part is type != "default"
    let hf_parts = collect_hf_parts(&plan.document);
    let mut hf_refs: HfRefs = HfRefs::default();
    let mut header_seq = 0u32;
    let mut footer_seq = 0u32;
    let mut needs_even_odd = false;
    for hf in &hf_parts {
        let (filename, content_type) = match hf.kind {
            "header" => {
                header_seq += 1;
                (
                    format!("header{}.xml", header_seq),
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
                )
            }
            "footer" => {
                footer_seq += 1;
                (
                    format!("footer{}.xml", footer_seq),
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
                )
            }
            _ => unreachable!(),
        };
        let rel_type = format!("{R_NS}/{}", hf.kind);
        let rid = pkg.add_rel(&rel_type, &filename);
        pkg.overrides
            .push((format!("/word/{filename}"), content_type.into()));
        pkg.parts
            .push((format!("word/{filename}"), hf.xml.clone().into_bytes(), false));
        hf_refs.insert(hf.kind, hf.type_, rid);
        if hf.type_ == "even" || hf.type_ == "first" {
            needs_even_odd = true;
        }
    }

    // Word needs an explicit settings.xml part with <w:evenAndOddHeaders/>
    // to actually render odd vs even differently — without this the docx
    // package still parses, but Word ignores the even-typed parts.
    if needs_even_odd {
        let _ = pkg.add_rel(&format!("{R_NS}/settings"), "settings.xml");
        pkg.overrides.push((
            "/word/settings.xml".into(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml".into(),
        ));
        pkg.parts.push((
            "word/settings.xml".into(),
            build_settings_xml(needs_even_odd).into_bytes(),
            false,
        ));
    }

    // Copy styles.xml from the template docx if provided. The template's
    // <w:style w:styleId="..."/> definitions become resolvable by every
    // paragraph that sets format.style_id, giving 'apply --template' parity
    // with the .NET TemplateApplyEngine for the structural-reformat use case.
    if let Some(tpl) = template_path {
        if let Some(styles_xml) = read_template_styles(tpl) {
            let _ = pkg.add_rel(&format!("{R_NS}/styles"), "styles.xml");
            pkg.overrides.push((
                "/word/styles.xml".into(),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml".into(),
            ));
            pkg.parts
                .push(("word/styles.xml".into(), styles_xml.into_bytes(), false));
        }
    }

    let document_xml = render_document(&plan, &mut pkg, &hf_refs, source, rn);

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

/// One header or footer OPC part required by the plan.
struct HfPart {
    kind: &'static str,   // "header" | "footer"
    type_: &'static str,  // "default" | "even" | "first"
    xml: String,
}

/// Maps (kind, type_) -> assigned rId. Lookup from sectPr emission.
#[derive(Default)]
struct HfRefs {
    inner: std::collections::HashMap<(&'static str, &'static str), String>,
}

impl HfRefs {
    fn insert(&mut self, kind: &'static str, type_: &'static str, rid: String) {
        self.inner.insert((kind, type_), rid);
    }
    fn headers(&self) -> Vec<(&'static str, &str)> {
        let mut v: Vec<_> = self.inner.iter()
            .filter(|((k, _), _)| *k == "header")
            .map(|((_, t), r)| (*t, r.as_str()))
            .collect();
        v.sort_by_key(|(t, _)| order_for(t));
        v
    }
    fn footers(&self) -> Vec<(&'static str, &str)> {
        let mut v: Vec<_> = self.inner.iter()
            .filter(|((k, _), _)| *k == "footer")
            .map(|((_, t), r)| (*t, r.as_str()))
            .collect();
        v.sort_by_key(|(t, _)| order_for(t));
        v
    }
}

fn order_for(t: &str) -> u8 {
    // sectPr child-order is fixed; among the headerReference/footerReference
    // siblings the OOXML schema doesn't care about ordering between types,
    // but a stable order is nicer for diffing.
    match t { "default" => 0, "even" => 1, "first" => 2, _ => 99 }
}

fn collect_hf_parts(doc: &PlanDocument) -> Vec<HfPart> {
    if presets::is_undergraduate_thesis(doc) {
        return thesis_hf_parts(doc);
    }
    // Pre-RS12 behavior: a single default footer when page_number is set.
    if let Some(xml) = footer_part(doc) {
        return vec![HfPart { kind: "footer", type_: "default", xml }];
    }
    Vec::new()
}

fn thesis_hf_parts(doc: &PlanDocument) -> Vec<HfPart> {
    let uni = doc
        .thesis_university
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("XXX大学");
    let title = doc
        .thesis_title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("论文标题");
    vec![
        HfPart {
            kind: "header",
            type_: "default",
            xml: build_header_xml(doc, &format!("{uni}毕业论文")),
        },
        HfPart {
            kind: "header",
            type_: "even",
            xml: build_header_xml(doc, title),
        },
        HfPart {
            kind: "footer",
            type_: "default",
            xml: build_page_number_footer_xml(doc),
        },
        HfPart {
            kind: "footer",
            type_: "even",
            xml: build_page_number_footer_xml(doc),
        },
    ]
}

fn build_header_xml(doc: &PlanDocument, text: &str) -> String {
    let rpr = run_properties(None, doc, None, false);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="{W_NS}"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r></w:p></w:hdr>"#,
        xml_escape(text)
    )
}

fn build_page_number_footer_xml(doc: &PlanDocument) -> String {
    let rpr = run_properties(None, doc, None, false);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="{W_NS}"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r>{rpr}<w:t xml:space="preserve">第 </w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r>{rpr}<w:t>1</w:t></w:r></w:fldSimple><w:r>{rpr}<w:t xml:space="preserve"> 页</w:t></w:r></w:p></w:ftr>"#
    )
}

/// Read `word/styles.xml` out of a template docx (zip). Returns None on
/// any failure -- the engine then renders without external styles.
fn read_template_styles(path: &str) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut entry = zip.by_name("word/styles.xml").ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    Some(s)
}

fn build_settings_xml(even_odd: bool) -> String {
    let mut settings = String::new();
    if even_odd {
        settings.push_str("<w:evenAndOddHeaders/>");
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="{W_NS}">{settings}</w:settings>"#
    )
}

/// Centered PAGE-field footer. Mirrors .NET NeedsPageNumberFooter.
/// `left-vertical` switches to a VML/wps dual-track anchored text box at
/// the binding edge (for landscape pages where page number runs vertically).
fn footer_part(doc: &PlanDocument) -> Option<String> {
    let v = doc
        .header_footer
        .as_ref()
        .and_then(|hf| hf.page_number.as_deref())
        .map(|s| s.trim().to_ascii_lowercase());
    let rpr = run_properties(None, doc, None, false);
    match v.as_deref() {
        Some("continuous") | Some("center-page-number") => {
            Some(format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="{W_NS}"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:fldSimple w:instr=" PAGE "><w:r>{rpr}<w:t>1</w:t></w:r></w:fldSimple></w:p></w:ftr>"#
            ))
        }
        Some("left-vertical") => {
            // Page-number paragraph carried INSIDE the textbox.
            let inner_p = format!(
                r#"<w:p xmlns:w="{W_NS}"><w:pPr><w:jc w:val="center"/></w:pPr><w:fldSimple w:instr=" PAGE "><w:r>{rpr}<w:t>1</w:t></w:r></w:fldSimple></w:p>"#
            );
            let textbox = build_vertical_pagenum_textbox(&inner_p);
            // Host paragraph: hold the run that carries the anchored shape.
            // Spacing 0/0 / 240 auto matches .NET's default host paragraph.
            Some(format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="{W_NS}"><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r>{textbox}</w:r></w:p></w:ftr>"#
            ))
        }
        _ => None,
    }
}

/// Builds the mc:AlternateContent wps/VML dual track for a vertical
/// (vert270) page-number text box at the binding edge. Mirrors
/// .NET DocxRenderer.BuildTextBoxAlternateContentXml with the engine's
/// default geometry — landscape A4 leftedge defaults.
fn build_vertical_pagenum_textbox(inner_paragraph_xml: &str) -> String {
    let pos_x: i64 = 457200;
    let pos_y: i64 = 1828800;
    let width: i64 = 457200;
    let height: i64 = 7772400;
    let text_direction = "vert270";
    let relative_from_h = "page";
    let relative_from_v = "page";
    let anchor = "ctr";
    let name = "VerticalTextBox";

    // VML coords are in points = EMU / 12700.
    let vml_left = pos_x as f64 / 12700.0;
    let vml_top = pos_y as f64 / 12700.0;
    let vml_width = width as f64 / 12700.0;
    let vml_height = height as f64 / 12700.0;
    let vml_layout_flow = "vertical;mso-layout-flow-alt:bottom-to-top";

    format!(
        concat!(
            r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#,
            r#" xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:wp="{WP_NS}" xmlns:a="{A_NS}""#,
            r#" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">"#,
            r#"<mc:Choice Requires="wps">"#,
            r#"<w:drawing>"#,
            r#"<wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0""#,
            r#" relativeHeight="251659264" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">"#,
            r#"<wp:simplePos x="0" y="0"/>"#,
            r#"<wp:positionH relativeFrom="{relative_from_h}"><wp:posOffset>{pos_x}</wp:posOffset></wp:positionH>"#,
            r#"<wp:positionV relativeFrom="{relative_from_v}"><wp:posOffset>{pos_y}</wp:posOffset></wp:positionV>"#,
            r#"<wp:extent cx="{width}" cy="{height}"/>"#,
            r#"<wp:effectExtent l="0" t="0" r="0" b="0"/>"#,
            r#"<wp:wrapNone/>"#,
            r#"<wp:docPr id="1" name="{name}"/>"#,
            r#"<wp:cNvGraphicFramePr/>"#,
            r#"<a:graphic>"#,
            r#"<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">"#,
            r#"<wps:wsp><wps:cNvSpPr txBox="1"/>"#,
            r#"<wps:spPr>"#,
            r#"<a:xfrm><a:off x="0" y="0"/><a:ext cx="{width}" cy="{height}"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#,
            r#"<a:noFill/><a:ln><a:noFill/></a:ln>"#,
            r#"</wps:spPr>"#,
            r#"<wps:txbx><w:txbxContent>{inner_paragraph_xml}</w:txbxContent></wps:txbx>"#,
            r#"<wps:bodyPr rot="0" spcFirstLastPara="0" vertOverflow="visible" horzOverflow="visible""#,
            r#" vert="{text_direction}" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720""#,
            r#" numCol="1" spcCol="0" rtlCol="0" fromWordArt="0" anchor="{anchor}" anchorCtr="0""#,
            r#" forceAA="0" compatLnSpc="1"/>"#,
            r#"</wps:wsp></a:graphicData></a:graphic>"#,
            r#"</wp:anchor></w:drawing></mc:Choice>"#,
            r#"<mc:Fallback>"#,
            r#"<w:pict xmlns:v="urn:schemas-microsoft-com:vml" xmlns:w10="urn:schemas-microsoft-com:office:word">"#,
            r#"<v:rect id="_x0000_s1026" style="position:absolute;margin-left:{vml_left:.2}pt;"#,
            r#"margin-top:{vml_top:.2}pt;width:{vml_width:.2}pt;height:{vml_height:.2}pt;"#,
            r#"z-index:251659264;mso-position-horizontal-relative:{relative_from_h};"#,
            r#"mso-position-vertical-relative:{relative_from_v}" stroked="f" filled="f">"#,
            r#"<v:textbox style="layout-flow:{vml_layout_flow}" inset="7.2pt,3.6pt,7.2pt,3.6pt">"#,
            r#"<w:txbxContent>{inner_paragraph_xml}</w:txbxContent>"#,
            r#"</v:textbox></v:rect></w:pict>"#,
            r#"</mc:Fallback></mc:AlternateContent>"#,
        ),
        W_NS = W_NS,
        R_NS = R_NS,
        WP_NS = WP_NS,
        A_NS = A_NS,
        pos_x = pos_x,
        pos_y = pos_y,
        width = width,
        height = height,
        name = name,
        relative_from_h = relative_from_h,
        relative_from_v = relative_from_v,
        text_direction = text_direction,
        anchor = anchor,
        vml_left = vml_left,
        vml_top = vml_top,
        vml_width = vml_width,
        vml_height = vml_height,
        vml_layout_flow = vml_layout_flow,
        inner_paragraph_xml = inner_paragraph_xml,
    )
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
    hf_refs: &HfRefs,
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
                section_inner(section, &plan.document, false, hf_refs)
            ));
        } else {
            let sect = format!(
                "<w:sectPr>{}</w:sectPr>",
                section_inner(section, &plan.document, true, hf_refs)
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
        // Table caption goes ABOVE the table per SKILL.md spec
        // ("表注默认放表上方"). Figure caption stays below.
        let mut out: Vec<BodyElem> = Vec::new();
        if let Some(cap) = block.caption.as_deref().filter(|c| !c.trim().is_empty()) {
            out.push(BodyElem::Para(caption_para(doc, cap)));
        }
        out.push(BodyElem::Raw(build_table(doc, &rows)));
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
                    // Bookmark wraps ONLY the digit, with the brackets as
                    // sibling runs. This way F9-updating a body REF field
                    // pulls back just "n", so the citation stays "[1]"
                    // instead of expanding to "[[1]]".
                    runs.push_str(&format!(
                        r#"<w:r>{rpr}<w:t>[</w:t></w:r>"#
                    ));
                    runs.push_str(&format!(
                        r#"<w:bookmarkStart w:id="{bid}" w:name="{BOOKMARK_PREFIX}{n}"/>"#
                    ));
                    runs.push_str(&format!(
                        r#"<w:r>{rpr}<w:t>{n}</w:t></w:r>"#
                    ));
                    runs.push_str(&format!(r#"<w:bookmarkEnd w:id="{bid}"/>"#));
                    runs.push_str(&format!(
                        r#"<w:r>{rpr}<w:t>]</w:t></w:r>"#
                    ));
                    runs.push_str(&format!(r#"<w:r>{rpr}<w:tab/></w:r>"#));
                    if !content.is_empty() {
                        runs.push_str(&format!(
                            r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
                            xml_escape(&content)
                        ));
                    }
                    // Reference paragraph isn't a heading.
                    let mut ppr_inner = paragraph_properties_inner(block.format.as_ref(), doc, None);
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

    let ppr_inner = paragraph_properties_inner(block.format.as_ref(), doc, heading_level);
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
    let line_rule = pf
        .and_then(|f| f.line_spacing_rule.as_deref())
        .and_then(map_line_rule)
        .unwrap_or("auto");
    if before.is_some() || after.is_some() || line.is_some() {
        let mut sp = String::from("<w:spacing");
        if let Some(v) = before {
            sp.push_str(&format!(r#" w:before="{}""#, xml_escape(&v)));
        }
        if let Some(v) = after {
            sp.push_str(&format!(r#" w:after="{}""#, xml_escape(&v)));
        }
        if let Some(v) = line {
            sp.push_str(&format!(
                r#" w:line="{}" w:lineRule="{}""#,
                xml_escape(&v),
                line_rule
            ));
        }
        sp.push_str("/>");
        ppr.push_str(&sp);
    }

    let run = if let Some(x) = xml {
        // Insert OMML XML as-is. The m: namespace prefix is declared on
        // the document root, so the caller-supplied element resolves.
        x.to_string()
    } else if !text.is_empty() {
        // Text fallback runs through autoformat so ^2 / x_1 / H2O become
        // proper sub/super runs instead of literal characters.
        render_text_segments(text, None, doc, None, false, None)
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
                            refs::CitPiece::Ref(n) => {
                                // Complex field (begin/separate/end). fldSimple's cached
                                // inner rPr is unreliable in some Word versions for
                                // superscript citations — the digit re-renders at baseline.
                                // Putting rPr on every run (including the begin/separate/
                                // end carriers and the display run) forces super to stick
                                // both before and after F9.
                                out.push_str(&format!(
                                    r#"<w:r>{rpr}<w:fldChar w:fldCharType="begin"/></w:r>"#
                                ));
                                out.push_str(&format!(
                                    r#"<w:r>{rpr}<w:instrText xml:space="preserve"> REF {BOOKMARK_PREFIX}{n} \h </w:instrText></w:r>"#
                                ));
                                out.push_str(&format!(
                                    r#"<w:r>{rpr}<w:fldChar w:fldCharType="separate"/></w:r>"#
                                ));
                                out.push_str(&format!(
                                    r#"<w:r>{rpr}<w:t>{n}</w:t></w:r>"#
                                ));
                                out.push_str(&format!(
                                    r#"<w:r>{rpr}<w:fldChar w:fldCharType="end"/></w:r>"#
                                ));
                            }
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

fn paragraph_properties_inner(
    fmt: Option<&PlanFormat>,
    doc: &PlanDocument,
    heading_level: Option<u8>,
) -> String {
    let mut inner = String::new();

    // pStyle must precede all other pPr children per OOXML schema.
    if let Some(s) = fmt.and_then(|f| f.style_id.as_deref()).filter(|s| !s.is_empty()) {
        inner.push_str(&format!(r#"<w:pStyle w:val="{}"/>"#, xml_escape(s)));
    }

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
    let line_rule = fmt
        .and_then(|f| f.line_spacing_rule.as_deref())
        .and_then(map_line_rule)
        .unwrap_or("auto");
    if line.is_some() || before.is_some() || after.is_some() {
        let mut sp = String::from("<w:spacing");
        if let Some(v) = before {
            sp.push_str(&format!(r#" w:before="{}""#, xml_escape(&v)));
        }
        if let Some(v) = after {
            sp.push_str(&format!(r#" w:after="{}""#, xml_escape(&v)));
        }
        if let Some(v) = line {
            sp.push_str(&format!(
                r#" w:line="{}" w:lineRule="{}""#,
                xml_escape(&v),
                line_rule
            ));
        }
        sp.push_str("/>");
        inner.push_str(&sp);
    }

    if fmt.and_then(|f| f.page_break_before).unwrap_or(false) {
        inner.push_str("<w:pageBreakBefore/>");
    }

    // <w:outlineLvl w:val="N"/> (0-indexed). Required for Word to treat
    // the paragraph as a heading for navigation pane / outline / TOC.
    // Emit independently of pStyle so headings work even without a
    // template's styles.xml defining Heading1/2/3.
    if let Some(n) = heading_level {
        let lvl = n.saturating_sub(1);
        inner.push_str(&format!(r#"<w:outlineLvl w:val="{lvl}"/>"#));
    }

    inner
}

fn run_properties(
    fmt: Option<&PlanFormat>,
    doc: &PlanDocument,
    heading_level: Option<u8>,
    is_title: bool,
) -> String {
    // When the block references a paragraph style (pStyle), inline run-level
    // rPr from doc defaults would *override* the style's run properties --
    // OOXML's precedence is: direct > paragraph style > docDefaults. We want
    // the template style to win, so when style_id is set we emit only what
    // the caller explicitly put on this block's format and skip auto-fill.
    let has_style = fmt
        .and_then(|f| f.style_id.as_deref())
        .filter(|s| !s.is_empty())
        .is_some();

    let explicit_ascii = fmt.and_then(|f| f.en_font.clone()).filter(|s| !s.is_empty());
    let explicit_east = fmt.and_then(|f| f.cn_font.clone()).filter(|s| !s.is_empty());
    let ascii = if has_style {
        explicit_ascii
    } else {
        explicit_ascii
            .or_else(|| doc.en_font.clone())
            .or_else(|| Some("Times New Roman".to_string()))
    };
    let east = if has_style {
        explicit_east
    } else {
        explicit_east
            .or_else(|| doc.cn_font.clone())
            .or_else(|| Some("宋体".to_string()))
    };

    let mut inner = String::new();
    if ascii.is_some() || east.is_some() {
        let a = ascii.unwrap_or_default();
        let e = east.unwrap_or_default();
        // Build rFonts with whichever sides are populated.
        let mut tag = String::from("<w:rFonts");
        if !a.is_empty() {
            tag.push_str(&format!(
                r#" w:ascii="{a}" w:hAnsi="{a}""#,
                a = xml_escape(&a)
            ));
        }
        if !e.is_empty() {
            tag.push_str(&format!(
                r#" w:eastAsia="{e}" w:cs="{e}""#,
                e = xml_escape(&e)
            ));
        }
        tag.push_str("/>");
        inner.push_str(&tag);
    }

    // Auto-bold for headings/titles ONLY when no template style is in play.
    // If a style is set, the style supplies its own bold (or absence thereof).
    let auto_bold = !has_style && (heading_level.is_some() || is_title);
    let bold = fmt.and_then(|f| f.bold).unwrap_or(false) || auto_bold;
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

    let sz = fmt.and_then(|f| f.font_pt).map(half_point).or_else(|| {
        if has_style {
            None
        } else {
            doc.base_font_pt
                .map(half_point)
                .or_else(|| Some(fallback_heading_size(heading_level)))
        }
    });
    if let Some(s) = sz {
        inner.push_str(&format!(r#"<w:sz w:val="{s}"/><w:szCs w:val="{s}"/>"#));
    }

    // OOXML precedence:  direct rPr > paragraph-style rPr > docDefaults.
    // An EMPTY <w:rPr></w:rPr> is treated by Word as a present-but-empty
    // direct-format layer that wins over the style's rPr -- which masks
    // the style's bold/size/etc and makes <w:pStyle w:val="Heading1"/>
    // appear to do nothing. When we have no direct overrides to emit,
    // return an empty string so the caller skips the rPr element entirely.
    if inner.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{inner}</w:rPr>")
    }
}

fn section_inner(
    section: Option<&PlanSection>,
    doc: &PlanDocument,
    is_break: bool,
    hf_refs: &HfRefs,
) -> String {
    let mut out = String::new();

    // headerReference / footerReference must come first in sectPr.
    for (type_, rid) in hf_refs.headers() {
        out.push_str(&format!(
            r#"<w:headerReference w:type="{type_}" r:id="{rid}"/>"#
        ));
    }
    for (type_, rid) in hf_refs.footers() {
        out.push_str(&format!(
            r#"<w:footerReference w:type="{type_}" r:id="{rid}"/>"#
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

fn map_line_rule(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "auto" => Some("auto"),
        "exact" => Some("exact"),
        "atleast" | "at-least" => Some("atLeast"),
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
