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
                        });
                    }
                }
                b"p" if in_body && in_table == 0 => {
                    para_idx += 1;
                    let (text, style, direct) = parse_paragraph(&mut reader);
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

/// Returns (text, pStyle, direct first-run+pPr props).
fn parse_paragraph(reader: &mut Reader<&[u8]>) -> (String, Option<String>, Props) {
    let mut text = String::new();
    let mut style = None;
    let mut direct = Props::default();
    let mut rpr_seen = false;
    let mut buf = Vec::new();
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match local(e.name().as_ref()) {
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
                b"rPr" if !rpr_seen => {
                    rpr_seen = true;
                    parse_props(reader, b"rPr", &mut direct);
                }
                b"t" => in_text = true,
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => {
                text.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(e)) => match local(e.name().as_ref()) {
                b"t" => in_text = false,
                b"p" => break,
                _ => {}
            },
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    (text, style, direct)
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
