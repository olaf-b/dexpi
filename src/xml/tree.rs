//! A byte-faithful syntax tree for DEXPI XML.
//!
//! This layer models the *file*, not the DEXPI object graph. It deliberately keeps
//! every lexical detail the parser saw — attribute spelling and order, whitespace
//! between elements, entity references, and whether a tag was written `<X/>` or
//! `<X></X>` — so that reading a document and writing it back reproduces the input
//! byte for byte. Semantics (classes, properties, references) belong one layer up.

/// A whole DEXPI XML file.
///
/// A DEXPI XML file has no XML declaration and a single `Model` root element, but
/// [`Document`] stores a node list so that leading and trailing whitespace survives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub nodes: Vec<Node>,
}

impl Document {
    /// Translate a byte offset into 1-based line and column, for diagnostics.
    ///
    /// Counts from the reserialized document, which is byte-identical to the source.
    pub fn line_column(&self, offset: usize) -> (usize, usize) {
        let text = super::write(self);
        let head = &text[..offset.min(text.len())];
        let line = head.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = head.rfind('\n').map_or(head.len(), |i| head.len() - i - 1) + 1;
        (line, column)
    }

    /// The root `Model` element, if the document has one.
    pub fn root(&self) -> Option<&Element> {
        self.nodes.iter().find_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    /// The root element, for editing.
    pub fn root_mut(&mut self) -> Option<&mut Element> {
        self.nodes.iter_mut().find_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    /// Character data, stored exactly as it appeared (still escaped).
    Text(String),
    /// An entity or character reference such as `&amp;`, stored as its name (`amp`).
    ///
    /// The parser reports these separately from text, so they get their own node
    /// rather than being silently decoded and re-encoded on the way out.
    EntityRef(String),
    /// A comment, stored without its `<!--` and `-->`.
    ///
    /// DEXPI's own files carry none, but a document that has been worked on by hand
    /// may, and dropping someone's annotations on the way through would be worse than
    /// refusing the file.
    Comment(String),
    /// An XML declaration or processing instruction, stored without its `<?` and `?>`.
    ///
    /// DEXPI writes neither — its files begin directly with `<Model>`, which is
    /// well-formed for UTF-8 — but nothing forbids them, and another tool's output must
    /// still be readable.
    Instruction(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    /// Everything between the element name and the closing `>` or `/>`, verbatim.
    ///
    /// Kept raw rather than parsed so that spacing, quoting and attribute order are
    /// reproduced exactly. Use [`Element::attributes`] for structured access.
    pub(super) attrs_raw: String,
    pub children: Vec<Node>,
    /// True when the source wrote this as `<X/>` rather than `<X></X>`.
    ///
    /// The distinction carries no meaning in XML, but preserving it keeps diffs
    /// clean when a tool rewrites a file a human has been editing.
    pub self_closing: bool,
    /// Byte offset of this element's `<` in the source document.
    ///
    /// Carried so that layers above can report problems against a place in the file
    /// rather than against a path in an abstract graph. See [`Document::line_column`].
    pub offset: usize,
}

impl Element {
    /// The element's attributes, in source order, with values still escaped.
    ///
    /// Parsed on demand from the preserved raw text rather than stored, so there is
    /// one representation of an attribute list and no way for the two to disagree.
    pub fn attributes(&self) -> Result<Vec<(String, String)>, super::Error> {
        // `BytesStart` wants "<name><attrs>" with the length of the name part.
        let content = format!("x{}", self.attrs_raw);
        let start = quick_xml::events::BytesStart::from_content(content, 1);
        start
            .attributes()
            .map(|attribute| {
                let attribute = attribute.map_err(quick_xml::Error::from)?;
                Ok((
                    attribute.key.as_ref().to_owned(),
                    attribute.value.into_owned(),
                ))
            })
            .collect()
    }

    /// The value of a named attribute, if present.
    pub fn attribute(&self, key: &str) -> Option<String> {
        self.attributes()
            .ok()?
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value)
    }

    /// The element's text content, with entity references decoded.
    ///
    /// Intended for leaf values — `<String>P-101</String>`, `<Double>2.2</Double>` —
    /// where the content is a value rather than mixed markup.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for node in &self.children {
            match node {
                Node::Text(text) => out.push_str(text),
                Node::EntityRef(name) => out.push_str(&decode_entity(name)),
                Node::Element(_) | Node::Comment(_) | Node::Instruction(_) => {}
            }
        }
        out
    }

    /// Set an attribute, replacing it if it is already present.
    ///
    /// Rewrites the whole attribute text, so an element edited this way loses any
    /// unusual spacing it had. That is acceptable for a value being changed on purpose;
    /// untouched elements keep their original text exactly.
    pub fn set_attribute(&mut self, key: &str, value: &str) {
        let escaped = value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;");
        let mut attributes: Vec<(String, String)> = self.attributes().unwrap_or_default();
        match attributes.iter_mut().find(|(name, _)| name == key) {
            Some((_, existing)) => *existing = escaped,
            None => attributes.push((key.to_owned(), escaped)),
        }
        self.attrs_raw = attributes
            .into_iter()
            .map(|(name, value)| format!(r#" {name}="{value}""#))
            .collect();
    }

    /// Replace the element's content with a single text node.
    ///
    /// Escapes the five predefined entities so the result stays well-formed, and
    /// writes them literally rather than as [`Node::EntityRef`] because a value that
    /// has just been edited has no original spelling worth preserving.
    pub fn set_text(&mut self, text: &str) {
        let escaped = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        self.children = vec![Node::Text(escaped)];
        self.self_closing = false;
    }

    /// Child elements, skipping text and entity nodes.
    pub fn children(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    /// Child elements with the given tag name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children().filter(move |e| e.name == name)
    }

    /// Mutable child elements, skipping text and entity nodes.
    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.children.iter_mut().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }
}

/// Decode an entity reference name into the text it stands for.
///
/// Unknown names are returned in their original form rather than dropped: this is a
/// convenience for reading values, not a second parser.
fn decode_entity(name: &str) -> String {
    match name {
        "amp" => "&".to_owned(),
        "lt" => "<".to_owned(),
        "gt" => ">".to_owned(),
        "quot" => "\"".to_owned(),
        "apos" => "'".to_owned(),
        _ => {
            if let Some(digits) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
                    .unwrap_or_else(|| format!("&{name};"))
            } else if let Some(digits) = name.strip_prefix('#') {
                digits
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
                    .unwrap_or_else(|| format!("&{name};"))
            } else {
                format!("&{name};")
            }
        }
    }
}
