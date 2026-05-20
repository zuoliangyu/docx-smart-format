//! Port of .NET DocxRenderer.AutoFormat. Splits a plain text run into
//! semantic segments (chemical-subscript, ion-charge superscript, unit
//! exponent, or `^X` / `_{XX}` script markers). Returns a list of
//! `(text, vertical_align)` pairs the caller wraps into `<w:r>` runs.

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    /// None | Some("superscript") | Some("subscript")
    pub vertical_align: Option<String>,
}

impl Segment {
    fn plain(s: &str) -> Self {
        Segment {
            text: s.to_string(),
            vertical_align: None,
        }
    }
    fn with(va: &str, s: &str) -> Self {
        Segment {
            text: s.to_string(),
            vertical_align: Some(va.to_string()),
        }
    }
}

/// .NET SplitAutoFormatSegments equivalent. Returns at least one segment.
pub fn split(text: &str) -> Vec<Segment> {
    if text.is_empty() {
        return vec![Segment::plain(text)];
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Segment> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    while i < chars.len() {
        // ^X / ^{XX} / _X / _{XX}
        if let Some((seg, next)) = try_read_script(&chars, i) {
            flush_buf(&mut out, &mut buf);
            out.push(seg);
            i = next;
            continue;
        }
        // Token (run of token chars)
        if let Some((token, next)) = try_read_token(&chars, i) {
            i = next;
            // Try chemistry split first, then unit exponent.
            if let Some(segs) = try_split_chemistry(&token) {
                flush_buf(&mut out, &mut buf);
                out.extend(segs);
                continue;
            }
            if let Some(segs) = try_split_unit_exponent(&token) {
                flush_buf(&mut out, &mut buf);
                out.extend(segs);
                continue;
            }
            buf.push_str(&token);
            continue;
        }
        buf.push(chars[i]);
        i += 1;
    }
    flush_buf(&mut out, &mut buf);
    if out.is_empty() {
        out.push(Segment::plain(text));
    }
    out
}

fn flush_buf(out: &mut Vec<Segment>, buf: &mut String) {
    if !buf.is_empty() {
        out.push(Segment::plain(buf));
        buf.clear();
    }
}

fn try_read_script(chars: &[char], i: usize) -> Option<(Segment, usize)> {
    let marker = *chars.get(i)?;
    if marker != '^' && marker != '_' {
        return None;
    }
    let start = i + 1;
    let c = *chars.get(start)?;
    let va = if marker == '^' { "superscript" } else { "subscript" };
    if c == '{' {
        // ^{XXX}
        let mut end = start + 1;
        while end < chars.len() && chars[end] != '}' {
            end += 1;
        }
        if end >= chars.len() || end == start + 1 {
            return None;
        }
        let s: String = chars[start + 1..end].iter().collect();
        if s.is_empty() {
            return None;
        }
        Some((Segment::with(va, &s), end + 1))
    } else {
        // ^X (run of script chars)
        let mut cursor = start;
        while cursor < chars.len() && is_script_char(chars[cursor]) {
            cursor += 1;
        }
        if cursor == start {
            return None;
        }
        let s: String = chars[start..cursor].iter().collect();
        Some((Segment::with(va, &s), cursor))
    }
}

fn try_read_token(chars: &[char], i: usize) -> Option<(String, usize)> {
    if !chars.get(i).map(|c| is_token_char(*c)).unwrap_or(false) {
        return None;
    }
    let mut end = i;
    while end < chars.len() && is_token_char(chars[end]) {
        end += 1;
    }
    Some((chars[i..end].iter().collect(), end))
}

fn try_split_chemistry(token: &str) -> Option<Vec<Segment>> {
    if token.is_empty() {
        return None;
    }
    let has_upper = token.chars().any(|c| c.is_uppercase());
    let has_digit = token.chars().any(|c| c.is_ascii_digit());
    let has_pm = token.contains('+') || token.contains('-');
    if !has_upper || (!has_digit && !has_pm) {
        return None;
    }
    // English-word guard: real chemical element symbols are at most two
    // characters (H, He, Na, Mg, ...). A token with 3+ consecutive
    // lowercase letters cannot be chemistry -- it's a word like
    // "Heading1" / "Section2" / "Chapter3" / "World123". Reject so that
    // the digit doesn't get auto-subscripted inside an English word.
    let mut lower_run = 0u32;
    for c in token.chars() {
        if c.is_lowercase() {
            lower_run += 1;
            if lower_run >= 3 {
                return None;
            }
        } else {
            lower_run = 0;
        }
    }

    // Split off trailing charge (\d*[+-]+).
    let bytes: Vec<char> = token.chars().collect();
    let mut charge_start = bytes.len();
    let mut k = bytes.len();
    while k > 0 && (bytes[k - 1] == '+' || bytes[k - 1] == '-') {
        k -= 1;
    }
    let pm_start = k;
    // optional preceding digits
    let mut d = pm_start;
    while d > 0 && bytes[d - 1].is_ascii_digit() {
        d -= 1;
    }
    if pm_start < bytes.len() {
        // there is a + or - suffix
        charge_start = d;
    }

    let core: String = bytes[..charge_start].iter().collect();
    let charge: String = bytes[charge_start..].iter().collect();

    let mut segs: Vec<Segment> = Vec::new();
    let mut plain = String::new();
    let core_chars: Vec<char> = core.chars().collect();
    let mut i = 0;
    while i < core_chars.len() {
        let ch = core_chars[i];
        if ch.is_ascii_digit() && i > 0 && is_chem_anchor(core_chars[i - 1]) {
            if !plain.is_empty() {
                segs.push(Segment::plain(&plain));
                plain.clear();
            }
            let s = i;
            while i < core_chars.len() && core_chars[i].is_ascii_digit() {
                i += 1;
            }
            let sub: String = core_chars[s..i].iter().collect();
            segs.push(Segment::with("subscript", &sub));
            continue;
        }
        plain.push(ch);
        i += 1;
    }
    if !plain.is_empty() {
        segs.push(Segment::plain(&plain));
    }
    if !charge.is_empty() {
        segs.push(Segment::with("superscript", &charge));
    }

    // Only return if we actually produced at least one sub/super segment.
    if segs.iter().any(|s| s.vertical_align.is_some()) {
        Some(segs)
    } else {
        None
    }
}

/// Recognized scientific units that can carry a ²/³ exponent suffix.
const UNITS: &[&str] = &[
    "mm", "cm", "dm", "km", "nm", "um", "\u{03bc}m", "\u{00b5}m", "m", "L", "mL", "\u{03bc}L",
    "\u{00b5}L", "g", "kg", "mg", "s", "min", "h", "Hz", "kHz", "MHz", "GHz", "Pa", "kPa", "MPa",
    "V", "mV", "A", "mA", "W", "kW", "J", "kJ", "N", "mol", "\u{2103}", "\u{00b0}C", "K",
];

fn try_split_unit_exponent(token: &str) -> Option<Vec<Segment>> {
    if token.len() < 2 {
        return None;
    }
    let last = token.chars().last()?;
    if last != '2' && last != '3' {
        return None;
    }
    let base_str = &token[..token.len() - last.len_utf8()];
    // Longest-match unit prefix; UNITS already lists e.g. mm/m so order
    // doesn't matter for correctness as long as it matches exactly.
    if UNITS.iter().any(|&u| u == base_str) {
        Some(vec![
            Segment::plain(base_str),
            Segment::with("superscript", &last.to_string()),
        ])
    } else {
        None
    }
}

fn is_chem_anchor(c: char) -> bool {
    c.is_alphabetic() || c == ')' || c == ']' || c == '}'
}

fn is_token_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '+'
                | '-'
                | '\u{00b7}' // ·
                | '\u{00b0}' // °
                | '\u{03bc}' // μ
                | '\u{00b5}' // µ
        )
}

fn is_script_char(c: char) -> bool {
    c.is_alphanumeric() || c == '+' || c == '-'
}
