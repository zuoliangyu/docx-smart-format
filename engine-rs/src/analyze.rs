//! RS5 spike: read an arbitrary .docx and resolve *effective* paragraph
//! formatting (docDefaults -> paragraph-style basedOn chain -> direct
//! pPr/rPr). This is the make-or-break capability: the .NET engine leans
//! on WordEffectiveStyleResolver here. Goal is a feasibility verdict on
//! .NET-generated input, not full parity.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::Read;

#[derive(Clone, Default, Debug)]
struct Props {
    ascii: Option<String>,
    east: Option<String>,
    sz: Option<String>,
    bold: Option<bool>,
    italic: Option<bool>,
    vertical_align: Option<String>,
    jc: Option<String>,
    outline: Option<i32>,
}

impl Props {
    fn overlay(&mut self, o: &Props) {
        if o.ascii.is_some() {
            self.ascii = o.ascii.clone();
        }
        if o.east.is_some() {
            self.east = o.east.clone();
        }
        if o.sz.is_some() {
            self.sz = o.sz.clone();
        }
        if o.bold.is_some() {
            self.bold = o.bold;
        }
        if o.italic.is_some() {
            self.italic = o.italic;
        }
        if o.vertical_align.is_some() {
            self.vertical_align = o.vertical_align.clone();
        }
        if o.jc.is_some() {
            self.jc = o.jc.clone();
        }
        if o.outline.is_some() {
            self.outline = o.outline;
        }
    }
}

#[derive(Default)]
struct StyleDef {
    name: Option<String>,
    based_on: Option<String>,
    props: Props,
}

fn local(name: &[u8]) -> &[u8] {
    match name.iter().position(|&b| b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

fn attr<'a>(e: &'a quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if local(a.key.as_ref()) == key.as_bytes() {
            Some(String::from_utf8_lossy(&a.value).into_owned())
        } else {
            None
        }
    })
}

fn read_zip_part(path: &str, part: &str) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut f = zip.by_name(part).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

/// Parse a `<w:rPr>`/`<w:pPr>`-bearing scope until `end_local`, folding
/// recognized children into `p`. Generic over both styles and direct fmt.
fn parse_props(reader: &mut Reader<&[u8]>, end_local: &[u8], p: &mut Props) {
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match local(e.name().as_ref()) {
                b"rFonts" => {
                    if let Some(v) = attr(&e, "ascii") {
                        p.ascii = Some(v);
                    }
                    if let Some(v) = attr(&e, "eastAsia") {
                        p.east = Some(v);
                    }
                }
                b"sz" => {
                    if let Some(v) = attr(&e, "val") {
                        p.sz = Some(v);
                    }
                }
                b"b" => {
                    let v = attr(&e, "val");
                    p.bold = Some(!matches!(v.as_deref(), Some("0") | Some("false")));
                }
                b"i" => {
                    let v = attr(&e, "val");
                    p.italic = Some(!matches!(v.as_deref(), Some("0") | Some("false")));
                }
                b"vertAlign" => {
                    if let Some(v) = attr(&e, "val") {
                        p.vertical_align = Some(v);
                    }
                }
                b"jc" => {
                    if let Some(v) = attr(&e, "val") {
                        p.jc = Some(v);
                    }
                }
                b"outlineLvl" => {
                    if let Some(v) = attr(&e, "val") {
                        p.outline = v.parse::<i32>().ok().map(|x| x + 1);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if local(e.name().as_ref()) == end_local => break,
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
}

fn parse_styles(xml: &str) -> (Props, HashMap<String, StyleDef>) {
    let mut defaults = Props::default();
    let mut styles: HashMap<String, StyleDef> = HashMap::new();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local(e.name().as_ref()) {
                b"rPrDefault" => {
                    // docDefaults run props
                    let mut inner = Vec::new();
                    loop {
                        match reader.read_event_into(&mut inner) {
                            Ok(Event::Start(s)) if local(s.name().as_ref()) == b"rPr" => {
                                parse_props(&mut reader, b"rPr", &mut defaults);
                            }
                            Ok(Event::End(s)) if local(s.name().as_ref()) == b"rPrDefault" => {
                                break
                            }
                            Ok(Event::Eof) => break,
                            _ => {}
                        }
                        inner.clear();
                    }
                }
                b"style" => {
                    let id = attr(&e, "styleId").unwrap_or_default();
                    let mut def = StyleDef::default();
                    let mut inner = Vec::new();
                    loop {
                        match reader.read_event_into(&mut inner) {
                            Ok(Event::Start(s)) | Ok(Event::Empty(s)) => {
                                match local(s.name().as_ref()) {
                                    b"name" => def.name = attr(&s, "val"),
                                    b"basedOn" => def.based_on = attr(&s, "val"),
                                    b"rPr" => parse_props(&mut reader, b"rPr", &mut def.props),
                                    b"pPr" => parse_props(&mut reader, b"pPr", &mut def.props),
                                    _ => {}
                                }
                            }
                            Ok(Event::End(s)) if local(s.name().as_ref()) == b"style" => break,
                            Ok(Event::Eof) => break,
                            _ => {}
                        }
                        inner.clear();
                    }
                    if !id.is_empty() {
                        styles.insert(id, def);
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    (defaults, styles)
}

fn resolve_style(
    id: &str,
    styles: &HashMap<String, StyleDef>,
    defaults: &Props,
    depth: u8,
) -> Props {
    if depth > 16 {
        return defaults.clone();
    }
    let Some(def) = styles.get(id) else {
        return defaults.clone();
    };
    let mut p = match &def.based_on {
        Some(b) => resolve_style(b, styles, defaults, depth + 1),
        None => defaults.clone(),
    };
    p.overlay(&def.props);
    p
}

#[derive(Debug)]
struct Block {
    path: String,
    text: String,
    style: Option<String>,
    heading_level: Option<i32>,
    eff: Props,
    runs: Vec<SourceRun>,
}

/// Public projection of a source-document block for build --source overlay.
/// RS7 adds per-run fidelity (`runs`) so reformat preserves source inline
/// formatting (bold/italic/sub-superscript/tab) — mirrors .NET source.Runs.
#[derive(Debug, Clone)]
pub struct SourceBlock {
    pub path: String,
    pub text: String,
    pub heading_level: Option<i32>,
    pub runs: Vec<SourceRun>,
}

#[derive(Debug, Clone, Default)]
pub struct SourceRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub vertical_align: Option<String>,
    /// Some("tab") for a <w:tab/> marker; None for a regular text run.
    pub kind: Option<String>,
}

pub fn source_blocks(input: &str) -> std::io::Result<Vec<SourceBlock>> {
    let doc = read_zip_part(input, "word/document.xml")
        .ok_or_else(|| std::io::Error::other("word/document.xml missing"))?;
    let styles_xml = read_zip_part(input, "word/styles.xml").unwrap_or_default();
    let (defaults, styles) = parse_styles(&styles_xml);
    Ok(parse_document(&doc, &defaults, &styles)
        .into_iter()
        .map(|b| SourceBlock {
            path: b.path,
            text: b.text,
            heading_level: b.heading_level,
            runs: b.runs,
        })
        .collect())
}

fn parse_document(
    xml: &str,
    defaults: &Props,
    styles: &HashMap<String, StyleDef>,
) -> Vec<Block> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut blocks = Vec::new();
    let mut para_idx = 0;
    let mut table_idx = 0;
    let mut in_body = false;
    let mut in_table: u32 = 0; // <w:tbl> nest depth -- paragraphs inside don't count

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local(e.name().as_ref()) {
                b"body" => in_body = true,
                b"tbl" if in_body => {
                    in_table += 1;
                    if in_table == 1 {
                        table_idx += 1;
                        let text = parse_table_text(&mut reader);
                        in_table -= 1; // parse_table_text consumed through </w:tbl>
                        blocks.push(Block {
                            path: format!("/body/table[{table_idx}]"),
                            text,
                            style: None,
                            heading_level: None,
                            eff: defaults.clone(),
                            runs: Vec::new(),
                        });
                    }
                }
                b"p" if in_body && in_table == 0 => {
                    para_idx += 1;
                    let (text, style, direct, runs) = parse_paragraph(&mut reader);
                    let mut eff = match &style {
                        Some(s) => resolve_style(s, styles, defaults, 0),
                        None => defaults.clone(),
                    };
                    eff.overlay(&direct);
                    let heading_level = style
                        .as_deref()
                        .and_then(heading_from_style)
                        .or(eff.outline);
                    blocks.push(Block {
                        path: format!("/body/paragraph[{para_idx}]"),
                        text,
                        style,
                        heading_level,
                        eff,
                        runs,
                    });
                }
                _ => {}
            },
            Ok(Event::End(e)) => match local(e.name().as_ref()) {
                b"body" => in_body = false,
                b"tbl" if in_table > 0 => in_table -= 1,
                _ => {}
            },
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    blocks
}

/// Consume through </w:tbl>, returning all cell text joined by " | ".
fn parse_table_text(reader: &mut Reader<&[u8]>) -> String {
    let mut buf = Vec::new();
    let mut cells: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_text = false;
    let mut depth: u32 = 1; // we're already inside <w:tbl>
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local(e.name().as_ref()) {
                b"tbl" => depth += 1,
                b"tc" => cur.clear(),
                b"t" => in_text = true,
                _ => {}
            },
            Ok(Event::Empty(e)) if local(e.name().as_ref()) == b"t" => {
                // self-closing <w:t/> -> empty cell text
            }
            Ok(Event::Text(t)) if in_text => {
                cur.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(e)) => match local(e.name().as_ref()) {
                b"t" => in_text = false,
                b"tc" => cells.push(std::mem::take(&mut cur)),
                b"tbl" => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    cells.join(" | ")
}

fn heading_from_style(s: &str) -> Option<i32> {
    let l = s.to_ascii_lowercase();
    let digits: String = l.chars().filter(|c| c.is_ascii_digit()).collect();
    if l.starts_with("heading") || l.starts_with("\u{6807}\u{9898}") {
        digits.parse().ok()
    } else {
        None
    }
}

/// Returns (text, pStyle, direct(paragraph-level seed), runs(per-run)).
/// Walks the <w:p> event stream tracking <w:r> depth so:
///   - paragraph-level pStyle / jc / outlineLvl go into `direct`,
///   - the first encountered rPr (mark or first-run) seeds `direct` (kept
///     for the analyze JSON's effective view, RS5-compatible),
///   - every <w:r>'s rPr toggles flow into one `SourceRun` per run,
///   - <w:tab/> is emitted as a separate run with kind="tab".
fn parse_paragraph(
    reader: &mut Reader<&[u8]>,
) -> (String, Option<String>, Props, Vec<SourceRun>) {
    let mut text = String::new();
    let mut style = None;
    let mut direct = Props::default();
    let mut runs: Vec<SourceRun> = Vec::new();
    let mut cur = SourceRun::default();
    let mut in_r: u32 = 0;
    let mut in_t = false;
    let mut first_rpr_used = false;
    let mut buf = Vec::new();

    let flush = |runs: &mut Vec<SourceRun>, cur: &mut SourceRun| {
        if !cur.text.is_empty()
            || cur.bold
            || cur.italic
            || cur.vertical_align.is_some()
            || cur.kind.is_some()
        {
            runs.push(std::mem::take(cur));
        } else {
            *cur = SourceRun::default();
        }
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local(e.name().as_ref()) {
                b"r" => {
                    in_r += 1;
                    cur = SourceRun::default();
                }
                b"rPr" => {
                    let mut tmp = Props::default();
                    parse_props(reader, b"rPr", &mut tmp);
                    if in_r > 0 {
                        if matches!(tmp.bold, Some(true)) {
                            cur.bold = true;
                        }
                        if matches!(tmp.italic, Some(true)) {
                            cur.italic = true;
                        }
                        if tmp.vertical_align.is_some() {
                            cur.vertical_align = tmp.vertical_align.clone();
                        }
                    }
                    if !first_rpr_used {
                        direct.overlay(&tmp);
                        first_rpr_used = true;
                    }
                }
                b"t" => in_t = true,
                _ => {}
            },
            Ok(Event::Empty(e)) => match local(e.name().as_ref()) {
                b"pStyle" => style = attr(&e, "val"),
                b"jc" => {
                    if let Some(v) = attr(&e, "val") {
                        direct.jc = Some(v);
                    }
                }
                b"outlineLvl" => {
                    if let Some(v) = attr(&e, "val") {
                        direct.outline = v.parse::<i32>().ok().map(|x| x + 1);
                    }
                }
                b"tab" if in_r > 0 => {
                    let mut tab_cur = SourceRun::default();
                    tab_cur.kind = Some("tab".to_string());
                    // Close out current text run first (if any), then push the tab marker.
                    flush(&mut runs, &mut cur);
                    runs.push(tab_cur);
                }
                _ => {}
            },
            Ok(Event::Text(t)) if in_t && in_r > 0 => {
                let s = String::from_utf8_lossy(t.as_ref());
                cur.text.push_str(&s);
                text.push_str(&s);
            }
            Ok(Event::End(e)) => match local(e.name().as_ref()) {
                b"t" => in_t = false,
                b"r" => {
                    in_r -= 1;
                    flush(&mut runs, &mut cur);
                }
                b"p" => break,
                _ => {}
            },
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    (text, style, direct, runs)
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn opt(v: &Option<String>) -> String {
    v.as_ref().map(|s| json_str(s)).unwrap_or_else(|| "null".into())
}

pub fn analyze(input: &str, output: Option<&str>) -> std::io::Result<()> {
    let doc = read_zip_part(input, "word/document.xml")
        .ok_or_else(|| std::io::Error::other("word/document.xml missing"))?;
    let styles_xml = read_zip_part(input, "word/styles.xml").unwrap_or_default();
    let (defaults, styles) = parse_styles(&styles_xml);
    let blocks = parse_document(&doc, &defaults, &styles);

    let mut json = String::from("{\n  \"body\": [\n");
    for (i, b) in blocks.iter().enumerate() {
        // Mirror .NET's `bool Bold` default (false). null is for "unresolved",
        // which doesn't happen for paragraphs once defaults+style are applied.
        let bold = match b.eff.bold {
            Some(true) => "true",
            _ => "false",
        };
        let hl = b
            .heading_level
            .map(|x| x.to_string())
            .unwrap_or_else(|| "null".into());
        json.push_str(&format!(
            "    {{\"path\": {}, \"text\": {}, \"style\": {}, \"headingLevel\": {}, \"eff\": {{\"asciiFont\": {}, \"eastAsiaFont\": {}, \"sz\": {}, \"bold\": {}, \"align\": {}}}}}{}\n",
            json_str(&b.path),
            json_str(&b.text),
            opt(&b.style),
            hl,
            opt(&b.eff.ascii),
            opt(&b.eff.east),
            opt(&b.eff.sz),
            bold,
            opt(&b.eff.jc),
            if i + 1 < blocks.len() { "," } else { "" }
        ));
    }
    json.push_str("  ]\n}\n");

    match output {
        Some(path) => std::fs::write(path, json)?,
        None => println!("{json}"),
    }
    Ok(())
}
