use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::{Document, Element, Error, Node};

/// Parse a DEXPI XML document.
///
/// DEXPI's own files use a narrow slice of XML: no declaration, no comments, no CDATA.
/// Comments, processing instructions and an XML declaration are nonetheless accepted and
/// preserved, because nothing forbids them and another tool's output has to be readable.
/// What remains unsupported — CDATA sections and DTDs — is rejected rather than dropped.
/// A format-fidelity layer that silently discards input is worse than one that refuses
/// it, because the loss only becomes visible after someone has saved over their file.
pub fn parse(input: &str) -> Result<Document, Error> {
    let mut reader = Reader::from_str(input);
    // Keep whitespace between elements: it is part of the file's formatting.
    let config = reader.config_mut();
    config.trim_text_start = false;
    config.trim_text_end = false;

    // A stack of elements under construction; the last entry is the current parent.
    let mut stack: Vec<Element> = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();

    loop {
        let position = reader.buffer_position() as usize;
        match reader.read_event()? {
            Event::Start(e) => stack.push(element(&e, false, position)),
            Event::Empty(e) => {
                let el = element(&e, true, position);
                push(&mut stack, &mut nodes, Node::Element(el));
            }
            Event::End(e) => {
                let name = e.name().as_ref().to_owned();
                let el = stack.pop().ok_or_else(|| Error::UnexpectedEnd {
                    name: name.clone(),
                    position,
                })?;
                if el.name != name {
                    return Err(Error::MismatchedEnd {
                        expected: el.name,
                        found: name,
                        position,
                    });
                }
                push(&mut stack, &mut nodes, Node::Element(el));
            }
            Event::Text(e) => {
                let text = e.into_inner().into_owned();
                if !text.is_empty() {
                    push(&mut stack, &mut nodes, Node::Text(text));
                }
            }
            Event::GeneralRef(e) => {
                let name = e.into_inner().into_owned();
                push(&mut stack, &mut nodes, Node::EntityRef(name));
            }
            Event::Comment(e) => {
                let text = e.into_inner().into_owned();
                push(&mut stack, &mut nodes, Node::Comment(text));
            }
            Event::Decl(e) => {
                let text = e.as_ref().to_owned();
                push(&mut stack, &mut nodes, Node::Instruction(text));
            }
            Event::PI(e) => {
                let text = e.into_inner().into_owned();
                push(&mut stack, &mut nodes, Node::Instruction(text));
            }
            Event::Eof => break,
            other => {
                return Err(Error::Unsupported {
                    construct: describe(&other),
                    position,
                });
            }
        }
    }

    if let Some(unclosed) = stack.pop() {
        return Err(Error::UnclosedElement {
            name: unclosed.name,
        });
    }
    Ok(Document { nodes })
}

fn element(start: &BytesStart<'_>, self_closing: bool, offset: usize) -> Element {
    Element {
        name: start.name().as_ref().to_owned(),
        attrs_raw: start.attributes_raw().to_owned(),
        children: Vec::new(),
        self_closing,
        offset,
    }
}

/// Append a node to the innermost open element, or to the document if there is none.
fn push(stack: &mut [Element], nodes: &mut Vec<Node>, node: Node) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => nodes.push(node),
    }
}

fn describe(event: &Event<'_>) -> &'static str {
    match event {
        Event::CData(_) => "CDATA section",
        Event::DocType(_) => "DTD",
        _ => "construct",
    }
}
