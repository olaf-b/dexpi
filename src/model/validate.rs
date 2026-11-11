//! Checking an instance document against the models it imports.

use std::collections::{HashMap, HashSet};

use crate::xml::{Document, Element};

use super::item::{Item, ItemKind};
use super::resolve::Scope;
use super::{Error, Models};

/// Something wrong with a document, located in the file.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The document is not valid against the models.
    Error,
    /// Worth reporting, but the document may still be usable — most often an import
    /// that names a model which was never published (`MetaData`, see spec/VERSIONS.md).
    Warning,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(
            f,
            "{}:{}: {severity}: {}",
            self.line, self.column, self.message
        )
    }
}

impl Models {
    /// Validate an instance document.
    ///
    /// Checks that every object names a concrete class, that every property exists on
    /// that class or one it inherits from and is used with the right element, that
    /// multiplicities hold, and that references point somewhere. It does not yet check
    /// values against their declared data types — union and bound types make that a
    /// larger job, and it is the natural next increment.
    pub fn validate(&self, document: &Document) -> Result<Vec<Diagnostic>, Error> {
        let root = document.root().ok_or_else(|| Error::NotAModel {
            label: "document".to_owned(),
        })?;

        let mut diagnostics = Vec::new();
        let mut report = |severity, message: String, offset: usize| {
            let (line, column) = document.line_column(offset);
            diagnostics.push(Diagnostic {
                severity,
                message,
                line,
                column,
            });
        };

        // An instance document declares no model of its own, so an empty prefix has
        // nothing to resolve against; every reference in it must name an import.
        let mut prefixes = indexmap::IndexMap::new();
        for import in root.children_named("Import") {
            let (Some(prefix), Some(source)) =
                (import.attribute("prefix"), import.attribute("source"))
            else {
                continue;
            };
            if !self.models().any(|model| model.uri == source) {
                report(
                    Severity::Warning,
                    format!("imported model {prefix:?} is not available ({source})"),
                    import.offset,
                );
            }
            prefixes.insert(prefix, source);
        }
        let scope = Scope {
            current: String::new(),
            prefixes,
        };

        // `id` is an xsd:ID, which the schema requires to be unique in the file. Two
        // objects sharing one makes every reference to it ambiguous.
        let mut seen: Vec<String> = Vec::new();
        for object in objects(root) {
            let Some(id) = object.attribute("id") else {
                continue;
            };
            if seen.contains(&id) {
                report(
                    Severity::Error,
                    format!("duplicate id {id:?}; ids must be unique in a document"),
                    object.offset,
                );
            }
            seen.push(id);
        }

        let ids = collect_ids(root);
        for object in objects(root) {
            self.check_object(object, &scope, &ids, &mut report);
        }
        Ok(diagnostics)
    }

    fn check_object(
        &self,
        object: &Element,
        scope: &Scope,
        ids: &HashSet<String>,
        report: &mut impl FnMut(Severity, String, usize),
    ) {
        let Some(type_reference) = object.attribute("type") else {
            report(
                Severity::Error,
                "<Object> has no type attribute".to_owned(),
                object.offset,
            );
            return;
        };

        let Some(class) = self.resolve(scope, &type_reference) else {
            report(
                Severity::Error,
                format!("unknown type {type_reference:?}"),
                object.offset,
            );
            return;
        };
        if !class.kind.is_instantiable() {
            report(
                Severity::Error,
                format!(
                    "{type_reference:?} is a {} and cannot be instantiated",
                    class.kind.describe()
                ),
                object.offset,
            );
            return;
        }

        // Count uses per property so multiplicity can be checked once at the end,
        // rather than per element.
        let mut counts: HashMap<String, usize> = HashMap::new();

        for child in object.children() {
            let expected = match child.name.as_str() {
                "Data" => ItemKind::DataProperty,
                "Components" => ItemKind::CompositionProperty,
                "References" => ItemKind::ReferenceProperty,
                _ => continue,
            };
            let Some(name) = child.attribute("property") else {
                report(
                    Severity::Error,
                    format!("<{}> has no property attribute", child.name),
                    child.offset,
                );
                continue;
            };

            let Some(property) = self.find_property(scope, class, &name) else {
                report(
                    Severity::Error,
                    format!("{} has no property {name:?}", class.qualified_name()),
                    child.offset,
                );
                continue;
            };
            if property.kind != expected {
                report(
                    Severity::Error,
                    format!(
                        "{name:?} is a {} and cannot be written as <{}>",
                        property.kind.describe(),
                        child.name
                    ),
                    child.offset,
                );
                continue;
            }

            let used = match expected {
                // One `<Components>` may hold several objects, and one `<Data>` several
                // values; multiplicity counts those, not the wrapper elements.
                ItemKind::ReferenceProperty => child
                    .attribute("objects")
                    .map(|list| list.split_whitespace().count())
                    .unwrap_or(0),
                _ => child.children().count(),
            };
            *counts.entry(name.clone()).or_default() += used;

            if expected == ItemKind::ReferenceProperty {
                self.check_references(child, ids, report);
            } else {
                self.check_data_references(child, scope, report);
            }
        }

        for (name, count) in &counts {
            let Some(property) = self.find_property(scope, class, name) else {
                continue;
            };
            if let Some(upper) = property.upper
                && *count > upper as usize
            {
                report(
                    Severity::Error,
                    format!("{name:?} allows at most {upper} value(s) but has {count}"),
                    object.offset,
                );
            }
        }

        // Required properties that never appeared at all.
        for property in required_properties(class, self, scope) {
            if !counts.contains_key(&property.name) {
                report(
                    Severity::Error,
                    format!(
                        "{} requires property {:?}",
                        class.qualified_name(),
                        property.name
                    ),
                    object.offset,
                );
            }
        }
    }

    /// Check `<DataReference data="…">` targets, at any depth inside a value.
    ///
    /// These name enumeration literals — `Process/Enumerations.PortDirection.Outlet` —
    /// and a misspelled one is otherwise invisible: it is well-formed XML naming
    /// nothing, which is exactly the mistake hand-editing produces.
    fn check_data_references(
        &self,
        element: &Element,
        scope: &Scope,
        report: &mut impl FnMut(Severity, String, usize),
    ) {
        if element.name == "DataReference"
            && let Some(reference) = element.attribute("data")
        {
            match self.resolve(scope, &reference) {
                None => report(
                    Severity::Error,
                    format!("unknown data reference {reference:?}"),
                    element.offset,
                ),
                Some(item) if item.kind != ItemKind::EnumerationLiteral => report(
                    Severity::Error,
                    format!(
                        "{reference:?} is a {} where an enumeration literal was expected",
                        item.kind.describe()
                    ),
                    element.offset,
                ),
                Some(_) => {}
            }
        }
        for child in element.children() {
            self.check_data_references(child, scope, report);
        }
    }

    fn check_references(
        &self,
        element: &Element,
        ids: &HashSet<String>,
        report: &mut impl FnMut(Severity, String, usize),
    ) {
        let Some(list) = element.attribute("objects") else {
            return;
        };
        for reference in list.split_whitespace() {
            // An IDREF is "#" followed by the id; anything else is a name reference
            // into a model, which this check does not follow.
            let Some(id) = reference.strip_prefix('#') else {
                continue;
            };
            if !ids.contains(id) {
                report(
                    Severity::Error,
                    format!("reference {reference:?} points at no object in this document"),
                    element.offset,
                );
            }
        }
    }
}

/// Properties a class requires directly. Inherited requirements are reported against
/// the supertype that declares them, which is where a reader has to go to understand
/// them anyway.
fn required_properties<'a>(class: &'a Item, models: &'a Models, scope: &Scope) -> Vec<&'a Item> {
    let mut required: Vec<&Item> = class
        .children
        .values()
        .filter(|item| item.kind.is_property() && item.lower.unwrap_or(0) > 0)
        .collect();

    for super_type in &class.super_types {
        let scope = Scope {
            current: class.model.clone(),
            ..scope.clone()
        };
        if let Some(parent) = models.resolve(&scope, super_type) {
            required.extend(required_properties(parent, models, &scope));
        }
    }
    required
}

fn collect_ids(root: &Element) -> HashSet<String> {
    let mut ids = HashSet::new();
    fn walk(element: &Element, ids: &mut HashSet<String>) {
        if element.name == "Object"
            && let Some(id) = element.attribute("id")
        {
            ids.insert(id);
        }
        for child in element.children() {
            walk(child, ids);
        }
    }
    walk(root, &mut ids);
    ids
}

/// Every `Object` in the document, at any depth.
fn objects(root: &Element) -> Vec<&Element> {
    let mut found = Vec::new();
    fn walk<'a>(element: &'a Element, found: &mut Vec<&'a Element>) {
        if element.name == "Object" {
            found.push(element);
        }
        for child in element.children() {
            walk(child, found);
        }
    }
    walk(root, &mut found);
    found
}
