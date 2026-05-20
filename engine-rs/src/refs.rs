//! Port of .NET ReferenceNormalizer.
//!
//! Two passes:
//!   1. Reference blocks (role=reference, or text starting with `[n]`)
//!      are rewritten as: bookmarkStart + "[n]" + bookmarkEnd + tab +
//!      content. Bookmark name = _Ref_ref_<n>. Each ref number gets a
//!      stable bookmark id (counter starts at 1000, mirrors .NET).
//!   2. Superscript segments in non-reference paragraphs are scanned for
//!      `[n]` / `[n,m]` / `[n-m]`; numbers matching a known ref become a
//!      <w:fldSimple w:instr=" REF _Ref_ref_<n> \h "/> field. Brackets and
//!      separators remain plain super-text runs.
//!
//! Citation parsing is hand-rolled to keep the binary small (no regex dep).

use crate::plan::{FormatPlan, PlanBlock};
use std::collections::HashMap;

pub const BOOKMARK_PREFIX: &str = "_Ref_ref_";
pub const DEFAULT_HANGING_CHARS: &str = "200";
pub const DEFAULT_TAB_POSITION: &str = "480";

pub struct RefNormalize {
    /// ref number -> stable bookmark id
    pub refs: HashMap<u32, u32>,
}

impl RefNormalize {
    pub fn collect(plan: &FormatPlan) -> Self {
        let mut refs = HashMap::new();
        let mut next_id: u32 = 1000;
        for block in &plan.blocks {
            if let Some(n) = block_ref_number(block) {
                refs.entry(n).or_insert_with(|| {
                    let id = next_id;
                    next_id += 1;
                    id
                });
            }
        }
        RefNormalize { refs }
    }

    pub fn is_active(&self) -> bool {
        !self.refs.is_empty()
    }
}

fn block_ref_number(block: &PlanBlock) -> Option<u32> {
    let is_ref_role = block.role.eq_ignore_ascii_case("reference");
    let text = block.text.as_deref().unwrap_or("");
    // Treat as reference if explicit role OR text starts with "[\d+]".
    let leading_n = parse_leading_ref(text);
    if !is_ref_role && leading_n.is_none() {
        return None;
    }
    leading_n
}

/// Parse text of the form `^\s*\[(\d+)\](\s*)(.*)$`. Returns the number.
fn parse_leading_ref(text: &str) -> Option<u32> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'[' {
        return None;
    }
    i += 1;
    let s = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == s || i >= bytes.len() || bytes[i] != b']' {
        return None;
    }
    text[s..i].parse().ok()
}

/// Parses a leading reference and returns (ref_num, content_after_tab).
pub fn split_leading_ref(text: &str) -> Option<(u32, String)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'[' {
        return None;
    }
    i += 1;
    let s = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == s || i >= bytes.len() || bytes[i] != b']' {
        return None;
    }
    let n: u32 = text[s..i].parse().ok()?;
    i += 1; // past ']'
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    Some((n, text[i..].to_string()))
}

/// One piece of a citation-bearing superscript segment.
#[derive(Debug)]
pub enum CitPiece {
    /// Plain super text (the brackets, the separators, or unknown numbers).
    Text(String),
    /// A resolved citation -> REF field referencing _Ref_ref_<n>.
    Ref(u32),
}

/// Scan a superscript segment text for citations and split it.
/// Returns Some(pieces) iff at least one number maps to a known ref.
pub fn split_super_for_citations(text: &str, refs: &HashMap<u32, u32>) -> Option<Vec<CitPiece>> {
    let bytes = text.as_bytes();
    let mut out: Vec<CitPiece> = Vec::new();
    let mut last = 0;
    let mut i = 0;
    let mut any_ref = false;

    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        // Try to parse a citation starting at i.
        if let Some((end, inner)) = parse_citation(text, i) {
            if i > last {
                out.push(CitPiece::Text(text[last..i].to_string()));
            }
            out.push(CitPiece::Text("[".to_string()));
            for tok in inner {
                match tok {
                    CitTok::Num(s) => {
                        let n: u32 = s.trim().parse().unwrap_or(0);
                        if refs.contains_key(&n) {
                            out.push(CitPiece::Ref(n));
                            any_ref = true;
                        } else {
                            out.push(CitPiece::Text(s));
                        }
                    }
                    CitTok::Sep(s) => out.push(CitPiece::Text(s)),
                }
            }
            out.push(CitPiece::Text("]".to_string()));
            last = end;
            i = end;
            continue;
        }
        i += 1;
    }

    if last < bytes.len() {
        out.push(CitPiece::Text(text[last..].to_string()));
    }
    if any_ref {
        Some(out)
    } else {
        None
    }
}

enum CitTok {
    Num(String),
    Sep(String),
}

/// At s[start]=='[', try to parse `\[(\d+(?:\s*[,\-]\s*\d+)*)\]`. Returns
/// the byte index AFTER `]` plus the inner token list.
fn parse_citation(s: &str, start: usize) -> Option<(usize, Vec<CitTok>)> {
    let bytes = s.as_bytes();
    if bytes.get(start)? != &b'[' {
        return None;
    }
    let mut i = start + 1;
    // first number
    let n0 = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == n0 {
        return None;
    }
    let mut toks = Vec::new();
    toks.push(CitTok::Num(s[n0..i].to_string()));

    loop {
        // optional whitespace
        let ws_start = i;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b']' {
            // success
            return Some((i + 1, toks));
        }
        if bytes[i] != b',' && bytes[i] != b'-' {
            return None;
        }
        // separator with surrounding whitespace
        let sep_char_end = i + 1;
        let mut j = sep_char_end;
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        let nn = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == nn {
            return None;
        }
        toks.push(CitTok::Sep(s[ws_start..nn].to_string()));
        toks.push(CitTok::Num(s[nn..j].to_string()));
        i = j;
    }
}
