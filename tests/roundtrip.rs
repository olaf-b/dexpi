//! Round-trip fidelity against the vendored DEXPI 2.0 artifacts.
//!
//! These files are the only real DEXPI 2.0 material that exists publicly: the four
//! generated domain models, and `reference_pid.xml`, the single published instance
//! document (a P&ID on the Plant model, ~1.5 MB). Between them they exercise the
//! metamodel side of the format and the instance side, which is the whole surface
//! the `xml` module is responsible for.

use std::fs;
use std::path::{Path, PathBuf};

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec")
}

/// Parse, serialize, and require the output to equal the input byte for byte.
fn assert_roundtrips(path: &Path) {
    let source =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

    let document =
        dexpi::xml::parse(&source).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()));
    let written = dexpi::xml::write(&document);

    if written != source {
        let at = source
            .char_indices()
            .zip(written.char_indices())
            .find(|((_, a), (_, b))| a != b)
            .map(|((i, _), _)| i)
            .unwrap_or(source.len().min(written.len()));
        let window = |s: &str| {
            let start = s[..at.min(s.len())]
                .char_indices()
                .rev()
                .nth(60)
                .map_or(0, |(i, _)| i);
            let end = s.len().min(at + 60);
            s[start..end].to_owned()
        };
        panic!(
            "{} did not round-trip; first difference at byte {at}\n  input:  {:?}\n  output: {:?}",
            path.display(),
            window(&source),
            window(&written),
        );
    }
}

fn assert_dir_roundtrips(dir: &str) {
    let dir = spec_dir().join(dir);
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e} — run tools/vendor-spec.sh", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "xml"))
        .collect();
    files.sort();

    assert!(!files.is_empty(), "no XML files in {}", dir.display());
    for file in files {
        assert_roundtrips(&file);
    }
}

/// The generated Core, Plant, Process and Builtin models.
#[test]
fn models_roundtrip() {
    assert_dir_roundtrips("models");
}

/// The DEXPI 2.0 reference P&ID — the only published instance document.
#[test]
fn reference_pid_roundtrips() {
    assert_dir_roundtrips("examples");
}

#[test]
fn preserves_empty_element_spelling() {
    for source in [r#"<Model name="a"/>"#, r#"<Model name="a"></Model>"#] {
        let document = dexpi::xml::parse(source).expect("parse");
        assert_eq!(dexpi::xml::write(&document), source);
    }
}

#[test]
fn preserves_entity_references() {
    let source = "<Model><Data>a &amp; b &lt; c</Data></Model>";
    let document = dexpi::xml::parse(source).expect("parse");
    assert_eq!(dexpi::xml::write(&document), source);
}

/// DEXPI writes no XML declaration — its files begin directly with `<Model>`, which is
/// well-formed for UTF-8 — but nothing forbids one, and another tool's output has to be
/// readable.
#[test]
fn preserves_an_xml_declaration() {
    let source = r#"<?xml version="1.0" encoding="UTF-8"?>
<Model name="x"/>"#;
    let document = dexpi::xml::parse(source).expect("parse");
    assert_eq!(dexpi::xml::write(&document), source);
}

/// Comments are how a file gets annotated by hand. Dropping them on the way through
/// would be worse than refusing the file.
#[test]
fn preserves_comments() {
    let source = r#"<!-- feed --><Model name="x"><!-- a note --></Model>"#;
    let document = dexpi::xml::parse(source).expect("parse");
    assert_eq!(dexpi::xml::write(&document), source);
}

#[test]
fn rejects_constructs_that_are_not_dexpi_xml() {
    let err = dexpi::xml::parse("<Model><![CDATA[raw]]></Model>").expect_err("should reject CDATA");
    assert!(
        matches!(err, dexpi::xml::Error::Unsupported { .. }),
        "got {err}"
    );
}
