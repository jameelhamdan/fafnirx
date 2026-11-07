//! Just enough XML for what sits beside or inside a picture or video: an `.nfo` and an XMP packet. Names are kept as written
//! (`dc:subject`), whatever namespace they stand for. Nothing fails: a document that stops making sense ends where it does, and
//! what was read before is kept.

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// An XMP list entry. Its text belongs to the property that holds the list, however many wrappers (`rdf:Bag`) are between.
const LIST_ENTRY: &str = "rdf:li";

#[derive(Default)]
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    /// The text directly inside it, trimmed (CDATA is literal, entities are decoded).
    text: String,
    /// The text of the list entries below it.
    items: Vec<String>,
}

/// An element that has been opened: where it is in the document, and the text read of it so far.
struct Open {
    /// `None` for a list entry.
    element: Option<usize>,
    text: String,
}

pub struct Doc(Vec<Element>);

impl Doc {
    pub fn parse(xml: &str) -> Doc {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().check_end_names = false;
        let (mut elements, mut open): (Vec<Element>, Vec<Open>) = (Vec::new(), Vec::new());
        let begin = |e: &BytesStart, elements: &mut Vec<Element>, open: &mut Vec<Open>| {
            let name = e.name().as_ref().to_string();
            let element = (name != LIST_ENTRY).then(|| {
                let attributes = e
                    .attributes()
                    .flatten()
                    .map(|a| {
                        (
                            a.key.as_ref().to_string(),
                            a.normalized_value(XmlVersion::Implicit1_0).map(|v| v.into_owned()).unwrap_or_default(),
                        )
                    })
                    .collect();
                elements.push(Element { name, attributes, ..Element::default() });
                elements.len() - 1
            });
            open.push(Open { element, text: String::new() });
        };
        let end = |elements: &mut Vec<Element>, open: &mut Vec<Open>| {
            let Some(done) = open.pop() else { return };
            let text = done.text.trim().to_string();
            match done.element {
                Some(i) => elements[i].text = text,
                None => open.iter().filter_map(|o| o.element).for_each(|i| elements[i].items.push(text.clone())),
            }
        };
        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => begin(&e, &mut elements, &mut open),
                Ok(Event::Empty(e)) => {
                    begin(&e, &mut elements, &mut open);
                    end(&mut elements, &mut open);
                }
                Ok(Event::End(_)) => end(&mut elements, &mut open),
                Ok(Event::Text(t)) => open.last_mut().into_iter().for_each(|o| o.text.push_str(&t.xml10_content())),
                Ok(Event::CData(c)) => open.last_mut().into_iter().for_each(|o| o.text.push_str(&c.xml10_content())),
                Ok(Event::GeneralRef(r)) => {
                    let decoded = match r.resolve_char_ref() {
                        Ok(Some(c)) => Some(c.to_string()),
                        _ => resolve_predefined_entity(&r).map(str::to_string),
                    };
                    if let (Some(o), Some(d)) = (open.last_mut(), decoded) {
                        o.text.push_str(&d);
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                Ok(_) => {}
            }
        }
        Doc(elements)
    }

    /// The text of the first element called `name`; none when it has none.
    pub fn text(&self, name: &str) -> Option<String> {
        self.0.iter().find(|e| e.name == name).map(|e| e.text.clone()).filter(|t| !t.is_empty())
    }

    /// The value of the first attribute called `name`, on any element (the short form of a simple property).
    pub fn attribute(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .flat_map(|e| &e.attributes)
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }

    /// A simple property, written as an attribute or as an element.
    pub fn property(&self, name: &str) -> Option<String> {
        self.attribute(name).or_else(|| self.text(name))
    }

    /// The entries of a list property, or its text itself when it is a plain value.
    pub fn items(&self, name: &str) -> Vec<String> {
        match self.0.iter().find(|e| e.name == name) {
            Some(e) if !e.items.is_empty() => e.items.clone(),
            Some(e) => Some(e.text.clone()).filter(|t| !t.is_empty()).into_iter().collect(),
            None => self.attribute(name).into_iter().collect(),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/library/xml.rs"]
mod tests;
