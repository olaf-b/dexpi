//! Reading and writing the DEXPI XML exchange format (DEXPI 2.0).
//!
//! DEXPI 2.0 replaced Proteus XML with *DEXPI XML*, a generic serialization whose
//! schema describes a metamodel — `Model`, `Package`, `ConcreteClass`, `Object`,
//! `Data`, `References` — and no domain classes at all. The domain models (Core,
//! Plant, Process) are themselves DEXPI XML documents, which means one reader serves
//! both the models and the instance data described by them.
//!
//! This module is the syntax layer: a byte-faithful tree, with no interpretation of
//! what the elements mean. Two properties follow, and both matter downstream:
//!
//! * **Round-tripping is lossless by construction.** Nothing needs to be modelled in
//!   order to survive a read/write cycle, so a file from another DEXPI tool comes out
//!   the way it went in, including whatever parts of the standard this project does
//!   not yet understand.
//! * **Rewrites stay minimal.** Formatting, attribute order and entity spelling are
//!   preserved, so a tool writing a file that a person has been hand-editing changes
//!   only what it meant to change.
//!
//! ```
//! let source = r#"<Model name="example" uri="http://www.example.org"/>"#;
//! let document = dexpi::xml::parse(source)?;
//! assert_eq!(document.root().unwrap().name, "Model");
//! assert_eq!(dexpi::xml::write(&document), source);
//! # Ok::<(), dexpi::xml::Error>(())
//! ```

mod parse;
mod tree;
mod write;

pub use parse::parse;
pub use tree::{Document, Element, Node};
pub use write::write;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("malformed XML: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("unsupported XML {construct} at byte {position}: not valid in DEXPI XML")]
    Unsupported {
        construct: &'static str,
        position: usize,
    },

    #[error("closing tag </{name}> at byte {position} has no matching opening tag")]
    UnexpectedEnd { name: String, position: usize },

    #[error("expected </{expected}> but found </{found}> at byte {position}")]
    MismatchedEnd {
        expected: String,
        found: String,
        position: usize,
    },

    #[error("element <{name}> is never closed")]
    UnclosedElement { name: String },
}
