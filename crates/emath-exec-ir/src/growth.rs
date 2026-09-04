//! Core-growth gate: CDLOC/SCBD/KGS measured; operation-name
//! branches blocked.
//!
//! The law: a rising handwritten-core-per-capability slope is a
//! regression. A stable pure cell enters the nucleus as DATA (cell
//! schema + registry entry); any parser/sema/backend/kernel-dispatch
//! branch that NAMES a cohort operation fails the gate typed
//! (`E-GROWTH-001`). The gate scans SOURCE TEXT (so it runs as a unit
//! test over `include_str!` snapshots of the real nucleus — a live
//! tripwire), classifies each file, strips comments, and reports:
//!
//! - **CDLOC** (core-domain LOC per capability): lines naming a cohort
//!   cell outside its data zone. 0 on a healthy nucleus.
//! - **SCBD** (shared-core branch deltas): match arms / equality tests
//!   dispatching on a cohort identity. 0 on a healthy nucleus.
//! - **KGS** (kernel generic surface): the `EmirOp` variant count — the
//!   bounded generic vocabulary. Grows only with NEW GENERIC ops, never
//!   per cell.
//!
//! Numbers are hypotheses until calibrated (a stated caveat); the
//! gate's BINDING rule is the zero-operation-name-branches invariant —
//! the seeded PR fixture and the live nucleus prove both directions.

/// Stable gate refusal code (operation-name branch in a gated file).
pub const E_GROWTH_NAME_BRANCH: &str = "E-GROWTH-001";

/// How a scanned nucleus file treats cohort names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NucleusClass {
    /// Cohort names are DATA here (the registry); counted, never
    /// violations.
    DataZone,
    /// Cohort names are operation-name branches: violations.
    Gated,
}

/// One gate violation: an operation-name branch in a gated file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateViolation {
    /// The scanned file's tag (`backend:codegen.rs`).
    pub file: String,
    /// 1-based source line of the branch.
    pub line: u32,
    /// The offending cohort token.
    pub token: String,
}

/// The measured report. Numbers are hypotheses until calibrated; the
/// violations list is the BINDING verdict.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GrowthReport {
    /// Core lines naming a capability outside its data zone.
    pub cdloc: u32,
    /// Shared-core branch deltas on capability identity.
    pub scbd: u32,
    /// Cohort-name mentions per file (all classes, post comment-strip).
    pub mentions_per_file: std::collections::BTreeMap<String, u32>,
    /// Mentions inside data zones (the registry — the admitted path).
    pub data_zone_mentions: u32,
    /// Typed violations (`E-GROWTH-001`); empty = gate green.
    pub violations: Vec<GateViolation>,
}

impl GrowthReport {
    /// The binding verdict: a clean report is a green gate.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Classify a scanned file by its tag prefix (`zone:path`). Unknown
/// prefixes classify GATED (fail closed — a new directory does not
/// silently escape the gate).
#[must_use]
pub fn nucleus_class(tag: &str) -> NucleusClass {
    match tag.split(':').next() {
        // The registry file is the admitted data zone.
        Some("kernel") if tag.starts_with("kernel:term_compile") => NucleusClass::DataZone,
        Some("data") => NucleusClass::DataZone,
        Some("kernel") | Some("parser") | Some("sema") | Some("backend") => NucleusClass::Gated,
        _ => NucleusClass::Gated,
    }
}

/// Strip `//` and `///` comment text (design notes never trip the gate)
/// while KEEPING string-literal contents — cohort names live inside
/// strings both in the registry (data) and in seeded branches (the
/// violation). A token in a comment is noise; a token in a string is
/// evidence.
fn strip_noise(line: &str) -> String {
    let no_comment = match line.find("//") {
        Some(index) => &line[..index],
        None => line,
    };
    let mut out = String::with_capacity(no_comment.len());
    let mut chars = no_comment.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                while let Some(inner) = chars.next() {
                    out.push(inner);
                    if inner == '\\' {
                        chars.next();
                    } else if inner == '"' {
                        break;
                    }
                }
            }
            '\'' => {
                out.push('\'');
                while let Some(inner) = chars.next() {
                    out.push(inner);
                    if inner == '\\' {
                        chars.next();
                    } else if inner == '\'' {
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Whether the stripped line is a BRANCH on capability identity (match
/// arm, equality test, or `starts_with`-style dispatch) rather than a
/// bare mention.
fn is_branch_line(stripped: &str) -> bool {
    stripped.contains("=>")
        || stripped.contains("==")
        || stripped.contains("!=")
        || (stripped.contains("match") && stripped.contains("op"))
}

/// Whole-token match: the cell path bounded by non-ident characters, so
/// `acme.exp` never trips a gate over `std.math.exp` (the character
/// before a match must not continue a different path, and the character
/// after must not extend the token).
fn line_mentions(stripped: &str, token: &str) -> bool {
    let bytes = stripped.as_bytes();
    let mut start = 0;
    while let Some(offset) = stripped[start..].find(token) {
        let absolute = start + offset;
        let end = absolute + token.len();
        let before_continues_identity = absolute > 0
            && (bytes[absolute - 1].is_ascii_alphanumeric() || bytes[absolute - 1] == b'.');
        let after_extends_token = end < bytes.len() && bytes[end].is_ascii_alphanumeric();
        if !before_continues_identity && !after_extends_token {
            return true;
        }
        start = end;
    }
    false
}

/// Run the gate over tagged nucleus sources.
///
/// `sources` are `(tag, text)` pairs (tags classify the file —
/// [`nucleus_class`]); `cohort` is the registered-cell identity list.
/// Comments and string literals are stripped first; each remaining
/// cohort-token mention is counted per file; mentions in a data zone
/// count toward `data_zone_mentions`, mentions in a gated file count as
/// [`GateViolation`]s (with a branch line also raising CDLOC/SCBD).
#[must_use]
pub fn growth_gate(sources: &[(&str, &str)], cohort: &[&str]) -> GrowthReport {
    let mut report = GrowthReport::default();
    for (tag, text) in sources {
        let class = nucleus_class(tag);
        let mut file_mentions = 0_u32;
        for (index, raw_line) in text.lines().enumerate() {
            let stripped = strip_noise(raw_line);
            let mut line_has_token = false;
            for token in cohort {
                if line_mentions(&stripped, token) {
                    line_has_token = true;
                    match class {
                        NucleusClass::DataZone => {
                            report.data_zone_mentions += 1;
                        }
                        NucleusClass::Gated => {
                            report.violations.push(GateViolation {
                                file: (*tag).to_string(),
                                line: (index + 1) as u32,
                                token: (*token).to_string(),
                            });
                            if is_branch_line(&stripped) {
                                report.scbd += 1;
                            }
                            report.cdloc += 1;
                        }
                    }
                }
            }
            if line_has_token {
                file_mentions += 1;
            }
        }
        report
            .mentions_per_file
            .insert((*tag).to_string(), file_mentions);
    }
    report
}

/// Count the kernel's generic op surface: `EmirOp` variants in the
/// exec-ir lib source. Bounded vocabulary — grows only with NEW GENERIC
/// ops, never per cell.
#[must_use]
pub fn kernel_generic_surface(lib_source: &str) -> usize {
    let mut in_enum = false;
    let mut count = 0;
    for raw_line in lib_source.lines() {
        let line = raw_line.trim();
        if line.starts_with("pub enum EmirOp") {
            in_enum = true;
            continue;
        }
        if in_enum {
            if line == "}" {
                break;
            }
            let is_variant = line.starts_with(|c: char| c.is_ascii_uppercase())
                && (line.contains('(')
                    || line.contains('{')
                    || line.ends_with(",")
                    || line.ends_with("}"));
            if is_variant && !line.starts_with("///") {
                count += 1;
            }
        }
    }
    count
}
