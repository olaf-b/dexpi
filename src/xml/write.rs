use std::fmt::Write as _;

use super::{Document, Element, Node};

/// Serialize a document back to DEXPI XML.
///
/// Output is produced directly from the preserved lexical detail rather than through
/// an XML writer, so nothing is re-escaped or re-formatted on the way out. Parsing a
/// document and writing it again reproduces the input exactly; see the round-trip
/// tests.
pub fn write(document: &Document) -> String {
    let mut out = String::new();
    for node in &document.nodes {
        write_node(&mut out, node);
    }
    out
}

fn write_node(out: &mut String, node: &Node) {
    match node {
        Node::Text(text) => out.push_str(text),
        Node::EntityRef(name) => {
            let _ = write!(out, "&{name};");
        }
        Node::Comment(text) => {
            let _ = write!(out, "<!--{text}-->");
        }
        Node::Instruction(text) => {
            let _ = write!(out, "<?{text}?>");
        }
        Node::Element(element) => write_element(out, element),
    }
}

fn write_element(out: &mut String, element: &Element) {
    let Element {
        name,
        attrs_raw,
        children,
        self_closing,
        offset: _,
    } = element;

    let _ = write!(out, "<{name}{attrs_raw}");
    if *self_closing {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for child in children {
        write_node(out, child);
    }
    let _ = write!(out, "</{name}>");
}
