//! Resolving DEXPI name references.
//!
//! A reference is `prefix/Dotted.Path`: the prefix names a model through the importing
//! document's `<Import>` elements, and the path is walked through named declarations.
//! An empty prefix means the current model, which is how a model refers to itself
//! (`/Process.GeneratingFlow`).

use indexmap::IndexMap;

use super::item::{Item, ItemKind};
use super::{Model, Models};

/// The `<Import>` prefixes in scope for a document, plus the model it belongs to.
#[derive(Debug, Clone)]
pub struct Scope {
    /// Model whose name an empty prefix resolves to.
    pub current: String,
    /// Import prefix to model URI.
    pub prefixes: IndexMap<String, String>,
}

impl Scope {
    /// The scope a model file's own references are resolved in.
    pub fn of_model(model: &Model) -> Self {
        Self {
            current: model.name.clone(),
            prefixes: model.imports.clone(),
        }
    }
}

impl Models {
    pub fn get(&self, name: &str) -> Option<&Model> {
        self.by_name.get(name)
    }

    pub fn models(&self) -> impl Iterator<Item = &Model> {
        self.by_name.values()
    }

    fn by_uri(&self, uri: &str) -> Option<&Model> {
        self.by_name.values().find(|model| model.uri == uri)
    }

    /// Resolve `reference` as written inside a document with the given scope.
    ///
    /// Returns `None` when the model is not loaded or the path names nothing. Callers
    /// distinguish the two with [`Models::model_for_prefix`], because an unresolvable
    /// import is a different problem from a misspelled class.
    pub fn resolve(&self, scope: &Scope, reference: &str) -> Option<&Item> {
        let (prefix, path) = split_reference(reference)?;
        let model = self.model_for_prefix(scope, prefix)?;
        self.resolve_in(model, path)
    }

    /// Resolve a dotted path within a known model.
    pub fn resolve_in<'a>(&'a self, model: &'a Model, path: &str) -> Option<&'a Item> {
        let mut segments = path.split('.');
        let mut item = model.items.get(segments.next()?)?;
        for segment in segments {
            item = item.child(segment)?;
        }
        Some(item)
    }

    /// The model a reference prefix names, if it is loaded.
    ///
    /// `Builtin` is always in scope: DEXPI XML uses `Builtin/String` and friends with
    /// no corresponding `<Import>` element anywhere.
    pub fn model_for_prefix(&self, scope: &Scope, prefix: &str) -> Option<&Model> {
        if prefix.is_empty() || prefix == scope.current {
            return self.by_name.get(&scope.current);
        }
        if prefix == "Builtin" {
            return self.by_name.get("Builtin");
        }
        match scope.prefixes.get(prefix) {
            Some(uri) => self.by_uri(uri),
            // Fall back to the prefix as a model name: harmless, and it keeps
            // hand-written test fixtures from needing a full set of imports.
            None => self.by_name.get(prefix),
        }
    }

    /// Whether `class` is `ancestor`, or inherits from it transitively.
    ///
    /// Used to ask domain questions of a document — "is this object a process step?"
    /// — without hardcoding the ~100 concrete subtypes of `ProcessStep`, which the
    /// standard is free to extend.
    pub fn is_subtype_of(&self, scope: &Scope, class: &Item, ancestor: &Item) -> bool {
        if std::ptr::eq(class, ancestor)
            || (class.model == ancestor.model && class.path == ancestor.path)
        {
            return true;
        }
        class.super_types.iter().any(|super_type| {
            let scope = Scope {
                current: class.model.clone(),
                ..scope.clone()
            };
            self.resolve(&scope, super_type)
                .is_some_and(|parent| self.is_subtype_of(&scope, parent, ancestor))
        })
    }

    /// Every concrete class that descends from an ancestor, as qualified names.
    ///
    /// This is how a palette is built: the standard defines around a hundred concrete
    /// kinds of process step and is free to add more, so the list belongs to the model
    /// rather than to any code that offers it.
    pub fn concrete_subtypes_of(&self, scope: &Scope, ancestor: &str) -> Vec<String> {
        let Some(ancestor) = self.resolve(scope, ancestor) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for model in self.models() {
            let model_scope = Scope::of_model(model);
            for item in model.items.values() {
                collect_concrete(self, &model_scope, item, ancestor, &mut found);
            }
        }
        found.sort();
        found
    }

    /// The scope a model's own references resolve in.
    ///
    /// A type written inside a model — `/Enumerations.CompressionMethod` — is relative
    /// to that model, not to whatever document is being validated. Resolving it in the
    /// instance's scope silently finds nothing.
    pub fn scope_of(&self, model: &str) -> Option<Scope> {
        self.by_name.get(model).map(Scope::of_model)
    }

    /// Every property a class requires, including those it inherits.
    pub fn required_properties<'a>(&'a self, scope: &Scope, class: &'a Item) -> Vec<&'a Item> {
        let mut required: Vec<&Item> = class
            .children
            .values()
            .filter(|item| item.kind.is_property() && item.is_required())
            .collect();

        for super_type in &class.super_types {
            let scope = Scope {
                current: class.model.clone(),
                ..scope.clone()
            };
            if let Some(parent) = self.resolve(&scope, super_type) {
                for inherited in self.required_properties(&scope, parent) {
                    if !required.iter().any(|p| p.name == inherited.name) {
                        required.push(inherited);
                    }
                }
            }
        }
        required
    }

    /// Look up a property on a class, searching its supertypes.
    ///
    /// DEXPI classes inherit deeply — `Pumping` reaches `Identifier` through
    /// `GeneratingFlow`, `ProcessStep` and `Core.ConceptualObject` — so a property
    /// lookup that ignores supertypes rejects almost every valid document.
    pub fn find_property<'a>(
        &'a self,
        scope: &Scope,
        class: &'a Item,
        property: &str,
    ) -> Option<&'a Item> {
        if let Some(found) = class.child(property)
            && found.kind.is_property()
        {
            return Some(found);
        }
        for super_type in &class.super_types {
            let scope = Scope {
                current: class.model.clone(),
                ..scope.clone()
            };
            if let Some(parent) = self.resolve(&scope, super_type)
                && let Some(found) = self.find_property(&scope, parent, property)
            {
                return Some(found);
            }
        }
        None
    }
}

impl ItemKind {
    pub(super) fn is_property(self) -> bool {
        matches!(
            self,
            Self::DataProperty | Self::CompositionProperty | Self::ReferenceProperty
        )
    }
}

/// Walk a declaration tree, collecting concrete classes that descend from `ancestor`.
fn collect_concrete(
    models: &Models,
    scope: &Scope,
    item: &Item,
    ancestor: &Item,
    found: &mut Vec<String>,
) {
    if item.kind == ItemKind::ConcreteClass && models.is_subtype_of(scope, item, ancestor) {
        found.push(item.qualified_name());
    }
    for child in item.children.values() {
        collect_concrete(models, scope, child, ancestor, found);
    }
}

/// Split `prefix/Dotted.Path` into its two halves.
fn split_reference(reference: &str) -> Option<(&str, &str)> {
    let (prefix, path) = reference.split_once('/')?;
    (!path.is_empty()).then_some((prefix, path))
}

#[cfg(test)]
mod tests {
    use super::split_reference;

    #[test]
    fn splits_references() {
        assert_eq!(
            split_reference("Core/PhysicalQuantities.PhysicalQuantity"),
            Some(("Core", "PhysicalQuantities.PhysicalQuantity"))
        );
        // An empty prefix means "this model".
        assert_eq!(
            split_reference("/Process.GeneratingFlow"),
            Some(("", "Process.GeneratingFlow"))
        );
        assert_eq!(
            split_reference("Builtin/String"),
            Some(("Builtin", "String"))
        );
        assert_eq!(split_reference("NotAReference"), None);
    }
}
