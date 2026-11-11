//! The DEXPI 2.0 information models, and validation of documents against them.
//!
//! DEXPI XML carries no domain vocabulary of its own: an instance document is a tree
//! of `<Object type="…">` elements whose meaning comes entirely from the models it
//! imports. This module loads those models — the vendored `Core`, `Plant`, `Process`
//! and `Builtin` files under `spec/models` — and uses them to answer the two questions
//! the syntax layer cannot: does this name refer to something, and is this document
//! consistent with what that something declares?
//!
//! ```no_run
//! use std::path::Path;
//!
//! let models = dexpi::model::Models::load_from_dir(Path::new("spec/models"))?;
//! let text = std::fs::read_to_string("spec/examples/reference_pid.xml")?;
//! let document = dexpi::xml::parse(&text)?;
//!
//! for diagnostic in models.validate(&document)? {
//!     println!("{diagnostic}");
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod item;
mod load;
mod resolve;
mod validate;

use indexmap::IndexMap;

pub use item::{Item, ItemKind};
pub use resolve::Scope;
pub use validate::{Diagnostic, Severity};

/// A loaded set of DEXPI models, resolvable against each other.
#[derive(Debug, Clone)]
pub struct Models {
    by_name: IndexMap<String, Model>,
}

/// One model file: `Core`, `Plant`, `Process` or `Builtin`.
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    /// The model's published URI. It does not resolve over the network (see
    /// `spec/VERSIONS.md`) and is used only to match `<Import source="…">`.
    pub uri: String,
    /// Import prefix to model URI.
    pub imports: IndexMap<String, String>,
    /// Top-level declarations, by name.
    pub items: IndexMap<String, Item>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing {label}")]
    Parse {
        label: String,
        #[source]
        source: crate::xml::Error,
    },

    #[error("{label} is not a DEXPI model: expected a <Model> root element")]
    NotAModel { label: String },

    #[error("<{element}> has no {attribute} attribute")]
    MissingAttribute { element: String, attribute: String },

    #[error(
        "the Builtin model was not loaded; DEXPI XML references Builtin/String and \
         similar without importing them, so it is always required"
    )]
    MissingBuiltin,
}
