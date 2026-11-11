//! Reading, writing and validating DEXPI 2.0 documents.
//!
//! The crate has two layers, and the dependency runs one way:
//!
//! * [`xml`] is the syntax layer: a byte-faithful parser and writer for DEXPI XML.
//!   It knows nothing about what the elements mean.
//! * [`model`] loads the DEXPI information models (themselves DEXPI XML documents)
//!   and validates instance documents against them. It builds on [`xml`].

pub mod model;
pub mod xml;
