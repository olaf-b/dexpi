//! Building the metamodel from vendored DEXPI XML model files.

use indexmap::IndexMap;

use crate::xml::Element;

use super::item::{Item, ItemKind};
use super::{Error, Model, Models};

impl Models {
    /// Load a set of model documents.
    ///
    /// `Builtin` must be among them: DEXPI XML references `Builtin/String` and friends
    /// without ever declaring an `<Import>` for them, so it cannot be discovered from
    /// the files themselves.
    pub fn load(sources: &[(&str, &str)]) -> Result<Self, Error> {
        let mut by_name = IndexMap::new();
        for (label, text) in sources {
            let document = crate::xml::parse(text).map_err(|source| Error::Parse {
                label: (*label).to_owned(),
                source,
            })?;
            let root = document.root().ok_or_else(|| Error::NotAModel {
                label: (*label).to_owned(),
            })?;
            let model = Model::from_root(root)?;
            by_name.insert(model.name.clone(), model);
        }

        if !by_name.contains_key("Builtin") {
            return Err(Error::MissingBuiltin);
        }
        Ok(Self { by_name })
    }

    /// The DEXPI 2.0.0 models, compiled into the library.
    ///
    /// These are the `Builtin`, `Core`, `Plant` and `Process` files vendored in this
    /// crate's `spec/models` (published by DEXPI e.V. under CC BY 4.0, see
    /// `spec/LICENSE`). Each call parses them afresh, so load them once and keep the
    /// result.
    ///
    /// ```
    /// let models = dexpi::model::Models::dexpi_2_0();
    /// assert!(models.get("Plant").is_some());
    /// ```
    #[cfg(feature = "bundled-models")]
    pub fn dexpi_2_0() -> Self {
        const SOURCES: &[(&str, &str)] = &[
            ("Builtin", include_str!("../../spec/models/Builtin.xml")),
            ("Core", include_str!("../../spec/models/Core.xml")),
            ("Plant", include_str!("../../spec/models/Plant.xml")),
            ("Process", include_str!("../../spec/models/Process.xml")),
        ];
        Self::load(SOURCES).expect("the bundled DEXPI 2.0 models load; the test suite checks this")
    }

    /// Load the four model files (`Builtin.xml`, `Core.xml`, `Plant.xml`,
    /// `Process.xml`) from a directory, such as this repository's `spec/models`.
    pub fn load_from_dir(dir: &std::path::Path) -> Result<Self, Error> {
        let mut texts = Vec::new();
        for name in ["Builtin", "Core", "Plant", "Process"] {
            let path = dir.join(format!("{name}.xml"));
            let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            texts.push((name.to_owned(), text));
        }
        let borrowed: Vec<(&str, &str)> = texts
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect();
        Self::load(&borrowed)
    }
}

impl Model {
    fn from_root(root: &Element) -> Result<Self, Error> {
        if root.name != "Model" {
            return Err(Error::NotAModel {
                label: root.name.clone(),
            });
        }
        let name = root
            .attribute("name")
            .ok_or_else(|| Error::MissingAttribute {
                element: "Model".to_owned(),
                attribute: "name".to_owned(),
            })?;
        let uri = root.attribute("uri").unwrap_or_default();

        // Prefixes an instance or model file uses to refer to other models. The
        // published URIs do not resolve (see spec/VERSIONS.md), so they serve purely
        // as identifiers matched against the `uri` of a loaded model.
        let mut imports = IndexMap::new();
        for import in root.children_named("Import") {
            if let (Some(prefix), Some(source)) =
                (import.attribute("prefix"), import.attribute("source"))
            {
                imports.insert(prefix, source);
            }
        }

        let mut items = IndexMap::new();
        for child in root.children() {
            if let Some(item) = build_item(child, &name, "") {
                items.insert(item.name.clone(), item);
            }
        }

        Ok(Self {
            name,
            uri,
            imports,
            items,
        })
    }
}

/// Convert one declaration element, and everything declared inside it, into an [`Item`].
///
/// Elements that are not declarations — the type expressions `BoundClass`,
/// `UnionDataType`, `DataTypeReference` and so on — are skipped. They describe what a
/// property's values look like, which this layer does not yet check.
fn build_item(element: &Element, model: &str, parent_path: &str) -> Option<Item> {
    let kind = ItemKind::from_tag(&element.name)?;
    let name = element.attribute("name")?;
    let path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}.{name}")
    };

    let super_types = element
        .attribute("superTypes")
        .map(|value| {
            value
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut children = IndexMap::new();
    for child in element.children() {
        if let Some(item) = build_item(child, model, &path) {
            children.insert(item.name.clone(), item);
        }
    }

    let mut types = Vec::new();
    collect_type_references(element, &mut types);
    let metadata = collect_metadata(element);

    Some(Item {
        kind,
        name,
        model: model.to_owned(),
        path,
        super_types,
        admits_undefined: types.iter().any(|t| t == "Builtin/Undefined"),
        data_type: types.iter().find(|t| *t != "Builtin/Undefined").cloned(),
        types,
        metadata,
        lower: element.attribute("lower").and_then(|v| v.parse().ok()),
        upper: element.attribute("upper").and_then(|v| v.parse().ok()),
        children,
    })
}

/// Gather the type references inside a declaration's type expression, in order.
fn collect_type_references(element: &Element, out: &mut Vec<String>) {
    for child in element.children() {
        if matches!(child.name.as_str(), "DataTypeReference" | "ClassReference")
            && let Some(reference) = child.attribute("type")
        {
            out.push(reference);
        }
        // Do not descend into nested declarations; their types are their own.
        if ItemKind::from_tag(&child.name).is_none() {
            collect_type_references(child, out);
        }
    }
}

/// Gather `MetaData` strings attached directly to a declaration.
fn collect_metadata(element: &Element) -> IndexMap<String, String> {
    element
        .children_named("Data")
        .filter_map(|data| {
            let property = data.attribute("property")?;
            let key = property.strip_prefix("MetaData/")?;
            let value = data.children().next()?.text();
            (!value.is_empty()).then(|| (key.to_owned(), value))
        })
        .collect()
}
