//! Loading the vendored models, and validating documents against them.

use std::path::{Path, PathBuf};

use dexpi::model::{Models, Severity};

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec")
}

fn models() -> Models {
    Models::load_from_dir(&spec_dir().join("models")).expect("loading vendored models")
}

const MINIMAL: &str = include_str!("fixtures/minimal_process.xml");

fn diagnose(models: &Models, source: &str) -> Vec<String> {
    let document = dexpi::xml::parse(source).expect("parse");
    models
        .validate(&document)
        .expect("validate")
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message)
        .collect()
}

#[test]
fn loads_the_vendored_models() {
    let models = models();
    for name in ["Builtin", "Core", "Plant", "Process"] {
        assert!(models.get(name).is_some(), "{name} did not load");
    }
}

/// The compiled-in models are the vendored files, so they must behave identically:
/// the same models, and the same verdict on the reference P&ID.
#[cfg(feature = "bundled-models")]
#[test]
fn bundled_models_match_the_vendored_files() {
    let bundled = Models::dexpi_2_0();
    let from_dir = models();
    let names = |m: &Models| m.models().map(|m| m.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&bundled), names(&from_dir));

    let source = std::fs::read_to_string(spec_dir().join("examples/reference_pid.xml"))
        .expect("reading reference_pid.xml");
    assert_eq!(diagnose(&bundled, &source), diagnose(&from_dir, &source));
}

#[test]
fn resolves_references_across_models() {
    let models = models();
    let process = models.get("Process").expect("Process");
    let scope = dexpi::model::Scope::of_model(process);

    // An import prefix, a self-reference with an empty prefix, and the implicit
    // Builtin model that is never imported anywhere.
    for reference in [
        "Core/QualifiedValue",
        "Core/PhysicalQuantities.PhysicalQuantity",
        "/Process.GeneratingFlow",
        "Builtin/String",
    ] {
        assert!(
            models.resolve(&scope, reference).is_some(),
            "{reference} did not resolve"
        );
    }
}

#[test]
fn distinguishes_abstract_from_concrete_classes() {
    let models = models();
    let process = models.get("Process").expect("Process");
    let scope = dexpi::model::Scope::of_model(process);

    let step = models
        .resolve(&scope, "/Process.ProcessStep")
        .expect("ProcessStep");
    assert!(!step.kind.is_instantiable(), "ProcessStep is abstract");

    let pumping = models.resolve(&scope, "/Process.Pumping").expect("Pumping");
    assert!(pumping.kind.is_instantiable(), "Pumping is concrete");
}

/// `Pumping` reaches `Identifier` through `GeneratingFlow` and `ProcessStep`. A lookup
/// that ignored supertypes would reject nearly every valid document.
#[test]
fn finds_inherited_properties() {
    let models = models();
    let process = models.get("Process").expect("Process");
    let scope = dexpi::model::Scope::of_model(process);
    let pumping = models.resolve(&scope, "/Process.Pumping").expect("Pumping");

    let identifier = models
        .find_property(&scope, pumping, "Identifier")
        .expect("Identifier is inherited from ProcessStep");
    assert_eq!(identifier.model, "Process");
    assert_eq!(identifier.path, "Process.ProcessStep.Identifier");
}

#[test]
fn minimal_process_document_is_valid() {
    let errors = diagnose(&models(), MINIMAL);
    assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
}

#[test]
fn rejects_an_unknown_class() {
    let source = MINIMAL.replace("Process/Process.Pumping", "Process/Process.Pmuping");
    let errors = diagnose(&models(), &source);
    assert!(
        errors.iter().any(|e| e.contains("unknown type")),
        "got {errors:#?}"
    );
}

#[test]
fn rejects_instantiating_an_abstract_class() {
    let source = MINIMAL.replace("Process/Process.Pumping", "Process/Process.ProcessStep");
    let errors = diagnose(&models(), &source);
    assert!(
        errors.iter().any(|e| e.contains("cannot be instantiated")),
        "got {errors:#?}"
    );
}

#[test]
fn rejects_an_unknown_property() {
    let source = MINIMAL.replace(r#"property="Identifier""#, r#"property="Idenifier""#);
    let errors = diagnose(&models(), &source);
    assert!(
        errors.iter().any(|e| e.contains("has no property")),
        "got {errors:#?}"
    );
}

/// Composition and reference properties are written with different elements, and
/// swapping them is the kind of mistake a hand-editor makes.
#[test]
fn rejects_a_property_used_with_the_wrong_element() {
    // `Ports` is a composition property, so it must be written as <Components>.
    let source = r#"<Model name="m" uri="http://example.org/m">
  <Import prefix="Process" source="https://data.dexpi.org/models/2.0.0/Process.xml"/>
  <Object id="P1" type="Process/Process.Pumping">
    <Data property="Ports"/>
  </Object>
</Model>"#;
    let errors = diagnose(&models(), source);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("composition property") && e.contains("<Data>")),
        "got {errors:#?}"
    );
}

#[test]
fn rejects_a_dangling_reference() {
    let source = MINIMAL.replace(r##"objects="#S1""##, r##"objects="#S9""##);
    let errors = diagnose(&models(), &source);
    assert!(
        errors.iter().any(|e| e.contains("points at no object")),
        "got {errors:#?}"
    );
}

#[test]
fn rejects_a_missing_required_property() {
    let source = MINIMAL.replace(
        "    <Data property=\"OriginatingSystemName\">\n      <String>bfd</String>\n    </Data>\n",
        "",
    );
    let errors = diagnose(&models(), &source);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("requires property") && e.contains("OriginatingSystemName")),
        "got {errors:#?}"
    );
}

/// The hand-authored BFD: five process steps, four streams, a component list and
/// material template, stream conditions as `QualifiedValue`-wrapped physical
/// quantities, and a diagram. It is the only DEXPI 2.0 process document available.
#[test]
fn flash_separation_bfd_is_valid() {
    let source = std::fs::read_to_string(spec_dir().join("examples/flash_separation_bfd.xml"))
        .expect("reading flash_separation_bfd.xml");
    let errors = diagnose(&models(), &source);
    assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
}

/// A misspelled enumeration literal is well-formed XML naming nothing, which is
/// exactly what hand-editing produces and what nothing else would catch.
#[test]
fn rejects_an_unknown_enumeration_literal() {
    let source = MINIMAL.replace("PortDirection.Outlet", "PortDirection.Out");
    let errors = diagnose(&models(), &source);
    assert!(
        errors.iter().any(|e| e.contains("unknown data reference")),
        "got {errors:#?}"
    );
}

/// The published reference P&ID does not fully conform to the 2.0 models: it omits
/// `EngineeringModel.ExportDateTime`, and eight of its twenty-four `Shape` objects
/// omit the required `SymbolRegistrationNumber`.
///
/// Pinned as a test because it is the only real instance document available, and
/// because the useful property is the *absence* of any other kind of error across its
/// 1,612 objects — that is what says the resolver handles the models correctly.
#[test]
fn reference_pid_has_only_the_known_conformance_gaps() {
    let source = std::fs::read_to_string(spec_dir().join("examples/reference_pid.xml"))
        .expect("reading reference_pid.xml");
    let errors = diagnose(&models(), &source);

    let unexpected: Vec<_> = errors
        .iter()
        .filter(|e| !e.contains("requires property"))
        .collect();
    assert!(
        unexpected.is_empty(),
        "unexpected diagnostics: {unexpected:#?}"
    );
    assert_eq!(errors.len(), 9, "known gaps changed: {errors:#?}");
}

/// `MetaData` is imported by the models but was never published, so an unresolved
/// import has to be tolerated rather than fatal.
#[test]
fn reports_an_unavailable_import_as_a_warning() {
    let source = MINIMAL.replace(
        r#"<Import prefix="Core" source="https://data.dexpi.org/models/2.0.0/Core.xml"/>"#,
        r#"<Import prefix="Core" source="https://data.dexpi.org/models/2.0.0/Core.xml"/>
  <Import prefix="MetaData" source="https://data.dexpi.org/models/2.0.0/MetaData.xml"/>"#,
    );
    let document = dexpi::xml::parse(&source).expect("parse");
    let diagnostics = models().validate(&document).expect("validate");

    assert!(
        diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("MetaData")),
        "got {diagnostics:#?}"
    );
}

/// The palette of step kinds comes from the model, not from a list in the code.
#[test]
fn enumerates_the_concrete_process_step_classes() {
    let models = models();
    let process = models.get("Process").expect("Process");
    let scope = dexpi::model::Scope::of_model(process);

    let classes = models.concrete_subtypes_of(&scope, "Process/Process.ProcessStep");
    assert!(classes.len() > 50, "only found {}", classes.len());
    for expected in [
        "Process/Process.Pumping",
        "Process/Process.SeparatingByFlash",
        "Process/Process.Source",
        "Process/Process.Sink",
    ] {
        assert!(classes.contains(&expected.to_owned()), "missing {expected}");
    }
    // Abstract classes are not offered.
    assert!(!classes.contains(&"Process/Process.ProcessStep".to_owned()));
    assert!(!classes.contains(&"Process/Process.Splitting".to_owned()));
}

/// A quantity's unit enumeration is the innermost layer of its type, and each unit
/// carries the symbol it is written with.
#[test]
fn finds_the_unit_type_and_symbol_of_a_quantity() {
    let models = models();
    let process = models.get("Process").expect("Process");
    let scope = dexpi::model::Scope::of_model(process);

    let stream = models.resolve(&scope, "/Process.Stream").expect("Stream");
    let temperature = stream.child("Temperature").expect("Temperature");
    assert_eq!(
        temperature.unit_type(),
        Some("Core/PhysicalQuantities.TemperatureUnit")
    );

    let units = models
        .resolve(&scope, temperature.unit_type().unwrap())
        .expect("the unit enumeration");
    let celsius = units.child("DegreeCelsius").expect("DegreeCelsius");
    assert_eq!(celsius.meta("un_symbol"), Some("°C"));
}

/// `id` is an xsd:ID, which the schema requires to be unique. Two objects sharing one
/// makes every reference to it ambiguous.
#[test]
fn rejects_a_duplicate_id() {
    let source = MINIMAL.replace(r#"id="V201""#, r#"id="P101""#);
    let errors = diagnose(&models(), &source);
    assert!(
        errors.iter().any(|e| e.contains("duplicate id")),
        "got {errors:#?}"
    );
}
