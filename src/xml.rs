//! A small XML reader, enough for Office Open XML's property parts:
//! elements with their attributes and text, the five predefined entities,
//! character references and CDATA. Comments, processing instructions and
//! document type declarations are skipped (no entity is ever defined or
//! expanded). Names keep their namespace prefix; [`Element::local_name`]
//! drops it. Depth and element count are capped.

/// The deepest nesting read.
const MAX_DEPTH: usize = 64;
/// The most elements read.
const MAX_ELEMENTS: usize = 65_536;

/// An element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// Its qualified name (`dc:creator`).
    pub name: String,
    /// Its attributes: qualified name and value.
    pub attributes: Vec<(String, String)>,
    /// Its text: the text directly inside it, entities decoded.
    pub text: String,
    /// Its child elements, in order.
    pub children: Vec<Element>,
}

impl Element {
    /// Its name without the namespace prefix (`creator`).
    pub fn local_name(&self) -> &str {
        local(&self.name)
    }

    /// The value of the attribute with local name `name`.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| local(n) == name)
            .map(|(_, value)| value.as_str())
    }

    /// The first child with local name `name`.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.local_name() == name)
    }
}

fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

/// A parsed document: its root element (what was read of it, when damaged)
/// and the damage met.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    /// The root element, when one was started.
    pub root: Option<Element>,
    /// Why reading stopped early.
    pub damage: Option<String>,
}

/// Read an XML document. Elements still open where it stops are closed.
pub fn parse(input: &str) -> Tree {
    let mut parser = Parser {
        input,
        at: 0,
        open: Vec::new(),
        root: None,
        elements: 0,
    };
    let damage = parser.run().err();
    while let Some(element) = parser.open.pop() {
        parser.close(element);
    }
    Tree {
        root: parser.root,
        damage,
    }
}

struct Parser<'a> {
    input: &'a str,
    /// Byte offset of the next markup or text.
    at: usize,
    /// Elements started and not yet ended, outermost first.
    open: Vec<Element>,
    root: Option<Element>,
    elements: usize,
}

impl<'a> Parser<'a> {
    fn run(&mut self) -> Result<(), String> {
        loop {
            let rest = self.rest();
            if rest.is_empty() || (self.root.is_some() && self.open.is_empty()) {
                break;
            }
            if let Some(after) = rest.strip_prefix("<!--") {
                self.skip_past(after, "-->")?;
            } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
                let (body, _) = after
                    .split_once("]]>")
                    .ok_or_else(|| self.damage("unterminated CDATA section"))?;
                self.append_text(body);
                self.advance(rest.len() - after.len() + body.len() + "]]>".len());
            } else if let Some(after) = rest.strip_prefix("<?") {
                self.skip_past(after, "?>")?;
            } else if rest.starts_with("<!") {
                self.skip_declaration()?;
            } else if let Some(after) = rest.strip_prefix("</") {
                self.end_tag(after)?;
            } else if let Some(after) = rest.strip_prefix('<') {
                self.start_tag(after)?;
            } else {
                let raw = rest.split('<').next().unwrap_or(rest);
                self.append_text(&unescape(raw));
                self.advance(raw.len());
            }
        }
        match self.open.last() {
            Some(element) => Err(format!("XML ends inside <{}>", element.name)),
            None => Ok(()),
        }
    }

    fn rest(&self) -> &'a str {
        self.input.get(self.at..).unwrap_or_default()
    }

    fn advance(&mut self, bytes: usize) {
        self.at = self.at.saturating_add(bytes).min(self.input.len());
    }

    fn damage(&self, what: &str) -> String {
        format!("XML: {what} at byte {}", self.at)
    }

    /// Skip to just past `end`, found in `after` (the rest following an
    /// opening delimiter).
    fn skip_past(&mut self, after: &str, end: &str) -> Result<(), String> {
        let at = after
            .find(end)
            .ok_or_else(|| self.damage(&format!("no closing {end}")))?;
        let consumed = self.rest().len() - after.len() + at + end.len();
        self.advance(consumed);
        Ok(())
    }

    /// `<!DOCTYPE …>`, its internal subset in brackets included.
    fn skip_declaration(&mut self) -> Result<(), String> {
        let mut brackets = 0_usize;
        for (i, c) in self.rest().char_indices() {
            match c {
                '[' => brackets += 1,
                ']' => brackets = brackets.saturating_sub(1),
                '>' if brackets == 0 => {
                    self.advance(i + 1);
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(self.damage("unterminated declaration"))
    }

    fn start_tag(&mut self, after: &str) -> Result<(), String> {
        if self.open.len() >= MAX_DEPTH {
            return Err(self.damage(&format!("nesting deeper than {MAX_DEPTH}")));
        }
        if self.elements >= MAX_ELEMENTS {
            return Err(self.damage(&format!("more than {MAX_ELEMENTS} elements")));
        }
        let mut tag = Tag { rest: after };
        let name = tag.name();
        if name.is_empty() {
            return Err(self.damage("element without a name"));
        }
        let mut element = Element {
            name: name.to_owned(),
            ..Element::default()
        };
        let empty = loop {
            tag.skip_space();
            if let Some(rest) = tag.rest.strip_prefix("/>") {
                tag.rest = rest;
                break true;
            }
            if let Some(rest) = tag.rest.strip_prefix('>') {
                tag.rest = rest;
                break false;
            }
            let attribute = tag
                .attribute()
                .ok_or_else(|| self.damage(&format!("malformed attribute in <{name}>")))?;
            element.attributes.push(attribute);
        };
        let consumed = self.rest().len() - tag.rest.len();
        self.advance(consumed);
        self.elements += 1;
        if empty {
            self.close(element);
        } else {
            self.open.push(element);
        }
        Ok(())
    }

    fn end_tag(&mut self, after: &str) -> Result<(), String> {
        let (inside, _) = after
            .split_once('>')
            .ok_or_else(|| self.damage("unterminated end tag"))?;
        let name = inside.trim();
        let element = self
            .open
            .pop()
            .ok_or_else(|| self.damage(&format!("</{name}> without a start")))?;
        if element.name != name {
            let damage = self.damage(&format!("</{name}> ends <{}>", element.name));
            self.open.push(element);
            return Err(damage);
        }
        let consumed = self.rest().len() - after.len() + inside.len() + ">".len();
        self.advance(consumed);
        self.close(element);
        Ok(())
    }

    /// An element ended: into its parent, or the root.
    fn close(&mut self, element: Element) {
        match self.open.last_mut() {
            Some(parent) => parent.children.push(element),
            None => {
                if self.root.is_none() {
                    self.root = Some(element);
                }
            }
        }
    }

    fn append_text(&mut self, text: &str) {
        if let Some(element) = self.open.last_mut() {
            element.text.push_str(text);
        }
    }
}

/// The inside of a start tag, read from its name on.
struct Tag<'a> {
    rest: &'a str,
}

impl<'a> Tag<'a> {
    fn name(&mut self) -> &'a str {
        let end = self
            .rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>' || c == '=')
            .unwrap_or(self.rest.len());
        let (name, rest) = self.rest.split_at(end);
        self.rest = rest;
        name
    }

    fn skip_space(&mut self) {
        self.rest = self.rest.trim_start();
    }

    /// `name="value"` or `name='value'`.
    fn attribute(&mut self) -> Option<(String, String)> {
        let name = self.name();
        if name.is_empty() {
            return None;
        }
        self.skip_space();
        self.rest = self.rest.strip_prefix('=')?;
        self.skip_space();
        let quote = self
            .rest
            .chars()
            .next()
            .filter(|&c| c == '"' || c == '\'')?;
        let (value, rest) = self.rest.get(1..)?.split_once(quote)?;
        self.rest = rest;
        Some((name.to_owned(), unescape(value)))
    }
}

/// Text with the predefined entities and character references decoded;
/// anything else that starts with `&` is kept as written.
pub fn unescape(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        let (before, from_ampersand) = rest.split_at(at);
        text.push_str(before);
        let decoded = from_ampersand
            .split_once(';')
            .and_then(|(entity, after)| Some((entity_char(entity.get(1..)?)?, after)));
        if let Some((c, after)) = decoded {
            text.push(c);
            rest = after;
        } else {
            text.push('&');
            rest = from_ampersand.get(1..).unwrap_or_default();
        }
    }
    text.push_str(rest);
    text
}

/// The character an entity (between `&` and `;`) stands for.
fn entity_char(entity: &str) -> Option<char> {
    const DECIMAL: u32 = 10;
    const HEXADECIMAL: u32 = 16;
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let reference = entity.strip_prefix('#')?;
            let code = match reference.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, HEXADECIMAL),
                None => u32::from_str_radix(reference, DECIMAL),
            };
            char::from_u32(code.ok()?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_attributes_text() {
        let tree = parse(
            "<?xml version=\"1.0\"?>\n<!-- c --><cp:core xmlns:cp=\"urn:x\">\
             <dc:title a='1' b = \"x&amp;y\">R&amp;D &#233;&#x2014;<![CDATA[<raw>]]></dc:title>\
             <empty/><dc:creator >Me</dc:creator ></cp:core>",
        );
        assert_eq!(tree.damage, None);
        let root = tree.root.unwrap();
        assert_eq!(root.local_name(), "core");
        assert_eq!(root.attribute("cp"), Some("urn:x"));
        let title = root.child("title").unwrap();
        assert_eq!(title.name, "dc:title");
        assert_eq!(title.text, "R&D é\u{2014}<raw>");
        assert_eq!(title.attribute("b"), Some("x&y"));
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.child("creator").unwrap().text, "Me");
    }

    #[test]
    fn unknown_entities_are_kept() {
        assert_eq!(unescape("a &b; &#xzz; & c"), "a &b; &#xzz; & c");
        assert_eq!(unescape("&#1114112;"), "&#1114112;");
    }

    #[test]
    fn doctype_is_skipped_not_expanded() {
        let tree = parse("<!DOCTYPE r [<!ENTITY e \"boom\">]><r>&e;</r>");
        assert_eq!(tree.root.unwrap().text, "&e;");
    }

    #[test]
    fn damage_keeps_what_was_read() {
        let tree = parse("<r><a>1</a><b>2</c></r>");
        let root = tree.root.unwrap();
        assert!(tree.damage.unwrap().contains("</c> ends <b>"));
        assert_eq!(root.child("a").unwrap().text, "1");
        assert_eq!(root.child("b").unwrap().text, "2");
        let tree = parse("<r><a>1");
        assert!(tree.damage.is_some());
        assert_eq!(tree.root.unwrap().child("a").unwrap().text, "1");
    }

    #[test]
    fn depth_is_capped() {
        let deep = "<a>".repeat(MAX_DEPTH + 1);
        assert!(parse(&deep).damage.unwrap().contains("nesting"));
    }
}
