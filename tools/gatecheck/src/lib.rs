//! `gatecheck`: validates governance payloads against the gate schemas.
//!
//! What it checks, in order:
//!
//! 1. Every file in `docs/governance/schemas/*.schema.json` is itself a valid
//!    JSON Schema 2020-12 document (meta-validation).
//! 2. Every payload under `docs/governance/examples/valid/` and
//!    `docs/design/packets/` validates against the schema named by its `gate`
//!    field.
//! 3. Every payload under `docs/governance/examples/invalid/` is rejected by
//!    the schema named by its `gate` field. This keeps the schemas honest: a
//!    schema that accepts everything would pass check 2 trivially.
//! 4. Cross-field rules that JSON Schema cannot express:
//!    - a packet file `PKT-NNNN.json` carries `packet_id` `PKT-NNNN`;
//!    - `depends_on_packets` only names packets that exist;
//!    - two packets may not claim overlapping `allowed_files` unless one
//!      declares a dependency on the other (locked subsystem boundaries);
//!    - in a Gate 3 payload, `ci_evidence.head_sha` equals `github.head_sha`
//!      (CI evidence is only evidence for the exact commit).
//!
//! The library returns a [`Report`]; the binary prints it and sets the exit
//! code; the integration test asserts it is clean.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use jsonschema::{Registry, Validator};
use serde_json::Value;

/// Relative locations inside the repository. One place to change them.
pub const SCHEMAS_DIR: &str = "docs/governance/schemas";
pub const VALID_EXAMPLES_DIR: &str = "docs/governance/examples/valid";
pub const INVALID_EXAMPLES_DIR: &str = "docs/governance/examples/invalid";
pub const PACKETS_DIR: &str = "docs/design/packets";

/// Maps the `gate` discriminator in a payload to its schema file name.
const GATE_SCHEMAS: &[(&str, &str)] = &[
    ("G1_PLANNING", "gate1-planning-handoff.schema.json"),
    ("G2_CODING", "gate2-coding-handoff.schema.json"),
    ("G3_QA_MERGE", "gate3-qa-merge-handoff.schema.json"),
];

/// Outcome of a full run.
#[derive(Debug, Default)]
pub struct Report {
    /// Human-readable lines for checks that passed.
    pub passed: Vec<String>,
    /// Human-readable lines for checks that failed. Empty means success.
    pub failures: Vec<String>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }

    fn pass(&mut self, msg: impl Into<String>) {
        self.passed.push(msg.into());
    }

    fn fail(&mut self, msg: impl Into<String>) {
        self.failures.push(msg.into());
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in &self.passed {
            writeln!(f, "ok    {line}")?;
        }
        for line in &self.failures {
            writeln!(f, "FAIL  {line}")?;
        }
        writeln!(
            f,
            "{} passed, {} failed",
            self.passed.len(),
            self.failures.len()
        )
    }
}

/// Errors that stop the run before any check can be reported.
#[derive(Debug)]
pub enum Error {
    Io(PathBuf, std::io::Error),
    Json(PathBuf, serde_json::Error),
    SchemaBuild(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(p, e) => write!(f, "{}: {e}", p.display()),
            Error::Json(p, e) => write!(f, "{}: invalid JSON: {e}", p.display()),
            Error::SchemaBuild(m) => write!(f, "schema build failed: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Run every check against the repository rooted at `repo_root`.
pub fn run(repo_root: &Path) -> Result<Report, Error> {
    let mut report = Report::default();

    let schemas = load_json_dir(&repo_root.join(SCHEMAS_DIR))?;
    if schemas.is_empty() {
        report.fail(format!("no schemas found in {SCHEMAS_DIR}"));
        return Ok(report);
    }

    // 1. Meta-validation and registry construction.
    let mut registry = Registry::new();
    for (path, schema) in &schemas {
        match jsonschema::meta::validate(schema) {
            Ok(()) => report.pass(format!(
                "{} is valid JSON Schema 2020-12",
                rel(repo_root, path)
            )),
            Err(e) => report.fail(format!(
                "{} is not valid JSON Schema: {e}",
                rel(repo_root, path)
            )),
        }
        let Some(id) = schema.get("$id").and_then(Value::as_str) else {
            report.fail(format!("{} has no $id", rel(repo_root, path)));
            continue;
        };
        registry = registry
            .add(id, schema.clone())
            .map_err(|e| Error::SchemaBuild(format!("{}: {e}", rel(repo_root, path))))?;
    }
    let registry = registry
        .prepare()
        .map_err(|e| Error::SchemaBuild(e.to_string()))?;

    let mut validators: BTreeMap<&str, Validator> = BTreeMap::new();
    for (gate, file) in GATE_SCHEMAS {
        let Some((_, schema)) = schemas
            .iter()
            .find(|(p, _)| p.file_name().is_some_and(|n| n == *file))
        else {
            report.fail(format!(
                "missing schema {SCHEMAS_DIR}/{file} for gate {gate}"
            ));
            continue;
        };
        let validator = jsonschema::draft202012::options()
            .with_registry(&registry)
            .offline()
            .build(schema)
            .map_err(|e| Error::SchemaBuild(format!("{file}: {e}")))?;
        validators.insert(gate, validator);
    }

    // 2. Payloads that must validate.
    let mut packets: Vec<(PathBuf, Value)> = Vec::new();
    for dir in [VALID_EXAMPLES_DIR, PACKETS_DIR] {
        let files = load_json_dir(&repo_root.join(dir))?;
        if files.is_empty() {
            report.fail(format!("no payloads found in {dir}"));
        }
        for (path, payload) in files {
            check_valid(&validators, repo_root, &path, &payload, &mut report);
            if dir == PACKETS_DIR {
                packets.push((path, payload));
            }
        }
    }

    // 3. Payloads that must be rejected.
    let invalid = load_json_dir(&repo_root.join(INVALID_EXAMPLES_DIR))?;
    if invalid.is_empty() {
        report.fail(format!("no payloads found in {INVALID_EXAMPLES_DIR}"));
    }
    for (path, payload) in invalid {
        check_invalid(&validators, repo_root, &path, &payload, &mut report);
    }

    // 4. Cross-field rules.
    check_packet_ids(repo_root, &packets, &mut report);
    check_packet_dependencies(repo_root, &packets, &mut report);
    check_scope_overlap(repo_root, &packets, &mut report);

    Ok(report)
}

fn check_valid(
    validators: &BTreeMap<&str, Validator>,
    root: &Path,
    path: &Path,
    payload: &Value,
    report: &mut Report,
) {
    let name = rel(root, path);
    let Some(validator) = gate_validator(validators, payload) else {
        report.fail(format!("{name}: missing or unknown `gate` field"));
        return;
    };
    let errors: Vec<String> = validator
        .iter_errors(payload)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    if errors.is_empty() {
        report.pass(format!("{name} validates"));
    } else {
        for e in errors {
            report.fail(format!("{name}: {e}"));
        }
        return;
    }
    if let Some(msg) = check_ci_sha_matches(payload) {
        report.fail(format!("{name}: {msg}"));
    }
}

fn check_invalid(
    validators: &BTreeMap<&str, Validator>,
    root: &Path,
    path: &Path,
    payload: &Value,
    report: &mut Report,
) {
    let name = rel(root, path);
    let Some(validator) = gate_validator(validators, payload) else {
        report.fail(format!(
            "{name}: invalid examples must still name a known `gate` so they fail for the intended reason"
        ));
        return;
    };
    let mut errors: Vec<String> = validator
        .iter_errors(payload)
        .map(|e| e.to_string())
        .collect();
    // The cross-field rules count as part of the contract too.
    errors.extend(check_ci_sha_matches(payload));
    if errors.is_empty() {
        report.fail(format!("{name} was accepted but must be rejected"));
    } else {
        report.pass(format!(
            "{name} is rejected ({} error(s), first: {})",
            errors.len(),
            errors[0]
        ));
    }
}

fn gate_validator<'a>(
    validators: &'a BTreeMap<&str, Validator>,
    payload: &Value,
) -> Option<&'a Validator> {
    let gate = payload.get("gate")?.as_str()?;
    validators.get(gate)
}

/// Gate 3: the CI run must be for the exact commit under review.
fn check_ci_sha_matches(payload: &Value) -> Option<String> {
    if payload.get("gate")?.as_str()? != "G3_QA_MERGE" {
        return None;
    }
    let head = payload.pointer("/github/head_sha")?.as_str()?;
    let ci = payload.pointer("/ci_evidence/head_sha")?.as_str()?;
    (head != ci).then(|| format!("ci_evidence.head_sha {ci} does not match github.head_sha {head}"))
}

fn check_packet_ids(root: &Path, packets: &[(PathBuf, Value)], report: &mut Report) {
    for (path, payload) in packets {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let id = payload
            .get("packet_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if stem == id {
            report.pass(format!("{} packet_id matches file name", rel(root, path)));
        } else {
            report.fail(format!(
                "{}: packet_id {id:?} does not match file name {stem:?}",
                rel(root, path)
            ));
        }
    }
}

fn check_packet_dependencies(root: &Path, packets: &[(PathBuf, Value)], report: &mut Report) {
    let known: Vec<&str> = packets
        .iter()
        .filter_map(|(_, p)| p.get("packet_id")?.as_str())
        .collect();
    for (path, payload) in packets {
        for dep in depends_on(payload) {
            if !known.contains(&dep) {
                report.fail(format!(
                    "{}: depends_on_packets names unknown packet {dep}",
                    rel(root, path)
                ));
            }
        }
    }
}

fn check_scope_overlap(root: &Path, packets: &[(PathBuf, Value)], report: &mut Report) {
    let mut conflicts = 0usize;
    for (i, (path_a, a)) in packets.iter().enumerate() {
        for (path_b, b) in &packets[i + 1..] {
            let (Some(id_a), Some(id_b)) = (packet_id(a), packet_id(b)) else {
                continue;
            };
            let related = depends_on(a).contains(&id_b) || depends_on(b).contains(&id_a);
            if related {
                continue;
            }
            for fa in allowed_files(a) {
                for fb in allowed_files(b) {
                    if paths_overlap(fa, fb) {
                        conflicts += 1;
                        report.fail(format!(
                            "{} and {} both claim {fa:?} / {fb:?} without a declared dependency",
                            rel(root, path_a),
                            rel(root, path_b)
                        ));
                    }
                }
            }
        }
    }
    if conflicts == 0 && packets.len() > 1 {
        report.pass(format!(
            "{} packets have disjoint or dependency-ordered scopes",
            packets.len()
        ));
    }
}

fn packet_id(payload: &Value) -> Option<&str> {
    payload.get("packet_id")?.as_str()
}

fn depends_on(payload: &Value) -> Vec<&str> {
    payload
        .pointer("/subsystem_scope/depends_on_packets")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn allowed_files(payload: &Value) -> Vec<&str> {
    payload
        .pointer("/subsystem_scope/allowed_files")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Two scope entries overlap when they are equal, or when one is a directory
/// prefix (entry ending in `/`) of the other. No globs: a scope is a file or a
/// directory, nothing cleverer.
pub fn paths_overlap(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let is_dir_prefix = |dir: &str, other: &str| dir.ends_with('/') && other.starts_with(dir);
    is_dir_prefix(a, b) || is_dir_prefix(b, a)
}

/// Load every `*.json` file in `dir`, sorted by path. A missing directory is
/// an empty list, so that the caller can report it as a failure rather than
/// an I/O error.
fn load_json_dir(dir: &Path) -> Result<Vec<(PathBuf, Value)>, Error> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| Error::Io(dir.to_path_buf(), e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|e| Error::Io(path.clone(), e))?;
        let value: Value = serde_json::from_str(&text).map_err(|e| Error::Json(path.clone(), e))?;
        out.push((path, value));
    }
    Ok(out)
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::paths_overlap;

    #[test]
    fn identical_paths_overlap() {
        assert!(paths_overlap(
            "crates/kate-text/src/lib.rs",
            "crates/kate-text/src/lib.rs"
        ));
    }

    #[test]
    fn directory_prefix_overlaps_file_inside_it() {
        assert!(paths_overlap(
            "crates/kate-text/",
            "crates/kate-text/src/lib.rs"
        ));
        assert!(paths_overlap(
            "crates/kate-text/src/lib.rs",
            "crates/kate-text/"
        ));
    }

    #[test]
    fn sibling_crates_do_not_overlap() {
        assert!(!paths_overlap("crates/kate-text/", "crates/kate-textual/"));
        assert!(!paths_overlap(
            "crates/kate-text/src/lib.rs",
            "crates/kate-core/src/lib.rs"
        ));
    }

    #[test]
    fn a_file_is_not_a_prefix() {
        // Only entries ending in '/' are directories; a file never contains another path.
        assert!(!paths_overlap(
            "crates/kate-text",
            "crates/kate-text/src/lib.rs"
        ));
    }
}
