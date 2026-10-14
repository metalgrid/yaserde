//! Native emission with YaSerDe's formatting and namespace conventions.
use super::Config;
use crate::xml::XmlName;
use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesPI, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Write;
use xml::common::XmlVersion;
use xml::name::Name;
use xml::writer::{Error, Result, XmlEvent};

#[derive(Clone, Copy, PartialEq)]
enum Content {
  Nothing,
  Markup,
  Text,
}

struct Element {
  name: XmlName,
  bindings: BTreeMap<String, String>,
  content: Content,
}

pub(super) struct QuickWriter<W: Write> {
  writer: Writer<W>,
  elements: Vec<Element>,
  outer_content: Content,
  pending_start: bool,
  declared: bool,
  auto_declaration: bool,
  indent: Option<String>,
}

impl<W: Write> QuickWriter<W> {
  pub(super) fn new(writer: W, config: &Config) -> Self {
    Self {
      writer: Writer::new(writer),
      elements: Vec::new(),
      outer_content: Content::Nothing,
      pending_start: false,
      declared: false,
      auto_declaration: config.write_document_declaration,
      indent: if config.perform_indent {
        Some(config.indent_string.clone().unwrap_or_else(|| "  ".into()))
      } else {
        None
      },
    }
  }

  pub(super) fn into_inner(self) -> W {
    self.writer.into_inner()
  }

  fn content(&mut self) -> &mut Content {
    match self.elements.last_mut() {
      Some(element) => &mut element.content,
      None => &mut self.outer_content,
    }
  }

  fn newline(&mut self, depth: usize) -> Result<()> {
    if let Some(indent) = &self.indent {
      self.writer.get_mut().write_all(b"\n")?;
      for _ in 0..depth {
        self.writer.get_mut().write_all(indent.as_bytes())?;
      }
    }
    Ok(())
  }

  fn before_markup(&mut self) -> Result<()> {
    let content = *self.content();
    if content != Content::Text && (!self.elements.is_empty() || content == Content::Markup) {
      self.newline(self.elements.len())?;
    }
    Ok(())
  }

  fn finish_start(&mut self) -> Result<()> {
    if self.pending_start {
      self.writer.get_mut().write_all(b">")?;
      self.pending_start = false;
    }
    Ok(())
  }

  fn ensure_declaration(&mut self) -> Result<()> {
    if self.auto_declaration && !self.declared {
      self.declaration(XmlVersion::Version10, None, None)?;
    }
    Ok(())
  }

  fn declaration(
    &mut self,
    version: XmlVersion,
    encoding: Option<&str>,
    standalone: Option<bool>,
  ) -> Result<()> {
    if self.declared {
      return Err(Error::DocumentStartAlreadyEmitted);
    }
    self.before_markup()?;
    self.writer.write_event(Event::Decl(BytesDecl::new(
      match version {
        XmlVersion::Version10 => "1.0",
        XmlVersion::Version11 => "1.1",
      },
      Some(encoding.unwrap_or("UTF-8")),
      standalone.map(|value| if value { "yes" } else { "no" }),
    )))?;
    self.declared = true;
    *self.content() = Content::Markup;
    Ok(())
  }

  pub(super) fn start<'a>(
    &mut self,
    name: Name<'_>,
    attributes: impl IntoIterator<Item = (Name<'a>, &'a str)>,
    namespace: impl IntoIterator<Item = (&'a str, &'a str)>,
  ) -> Result<()> {
    self.ensure_declaration()?;
    self.finish_start()?;
    self.before_markup()?;
    let mut start = BytesStart::new(qname(name));
    let mut bindings = BTreeMap::new();
    for (prefix, uri) in namespace {
      if prefix == "xml" || prefix == "xmlns" {
        continue;
      }
      let previous = self
        .elements
        .iter()
        .rev()
        .find_map(|e| e.bindings.get(prefix));
      if previous.map(String::as_str).unwrap_or("") == uri {
        continue;
      }
      let key = if prefix.is_empty() {
        "xmlns".to_owned()
      } else {
        format!("xmlns:{}", prefix)
      };
      push_attribute(&mut start, &key, uri);
      bindings.insert(prefix.to_owned(), uri.to_owned());
    }
    for (name, value) in attributes {
      push_attribute(&mut start, &qname(name), value);
    }
    // Only the terminator is pending: start-event I/O errors must be returned now,
    // and into_inner must never need to flush a buffered event.
    self.writer.get_mut().write_all(b"<")?;
    self.writer.get_mut().write_all(&start)?;
    self.elements.push(Element {
      name: XmlName {
        local_name: name.local_name.into(),
        prefix: name.prefix.map(Into::into),
        namespace: name.namespace.map(Into::into),
      },
      bindings,
      content: Content::Markup,
    });
    self.pending_start = true;
    Ok(())
  }

  pub(super) fn end(&mut self, name: Option<Name<'_>>) -> Result<()> {
    let element = self
      .elements
      .last()
      .ok_or(Error::LastElementNameNotAvailable)?;
    if let Some(name) = name {
      if name.local_name != element.name.local_name
        || name.prefix != element.name.prefix.as_deref()
        || name.namespace != element.name.namespace.as_deref()
      {
        return Err(Error::EndElementNameIsNotEqualToLastStartElementName);
      }
    }
    if self.pending_start {
      self.writer.get_mut().write_all(b" />")?;
      self.pending_start = false;
      self.elements.pop();
    } else {
      let element = self.elements.pop().unwrap();
      if element.content == Content::Markup {
        self.newline(self.elements.len())?;
      }
      self
        .writer
        .write_event(Event::End(BytesEnd::new(element.name.to_string())))?;
    }
    *self.content() = Content::Markup;
    Ok(())
  }

  pub(super) fn characters(&mut self, text: &str) -> Result<()> {
    self.ensure_declaration()?;
    self.finish_start()?;
    self
      .writer
      .write_event(Event::Text(BytesText::from_escaped(
        quick_xml::escape::partial_escape(text),
      )))?;
    *self.content() = Content::Text;
    Ok(())
  }

  pub(super) fn cdata(&mut self, text: &str) -> Result<()> {
    self.finish_start()?;
    for chunk in BytesCData::escaped(text) {
      self.writer.write_event(Event::CData(chunk))?;
    }
    *self.content() = Content::Text;
    Ok(())
  }

  pub(super) fn write(&mut self, event: XmlEvent<'_>) -> Result<()> {
    match event {
      XmlEvent::StartDocument {
        version,
        encoding,
        standalone,
      } => self.declaration(version, encoding, standalone),
      XmlEvent::StartElement {
        name,
        attributes,
        namespace,
      } => self.start(
        name,
        attributes.iter().map(|a| (a.name, a.value)),
        namespace.iter(),
      ),
      XmlEvent::EndElement { name } => self.end(name),
      XmlEvent::Characters(text) => self.characters(text),
      XmlEvent::CData(text) => self.cdata(text),
      XmlEvent::Comment(text) => {
        self.finish_start()?;
        self.before_markup()?;
        let mut padded = String::new();
        if !text.starts_with(char::is_whitespace) {
          padded.push(' ');
        }
        padded.push_str(&text.replace("--", "- "));
        if !text.ends_with(char::is_whitespace) {
          padded.push(' ');
        }
        self
          .writer
          .write_event(Event::Comment(BytesText::from_escaped(padded)))?;
        *self.content() = Content::Markup;
        Ok(())
      }
      XmlEvent::ProcessingInstruction { name, data } => {
        self.ensure_declaration()?;
        self.finish_start()?;
        self.before_markup()?;
        let content = match data {
          Some(data) => Cow::Owned(format!("{} {}", name, data)),
          None => Cow::Borrowed(name),
        };
        self.writer.write_event(Event::PI(BytesPI::new(content)))?;
        *self.content() = Content::Markup;
        Ok(())
      }
    }
  }
}

fn qname(name: Name<'_>) -> Cow<'_, str> {
  match name.prefix {
    Some(prefix) => Cow::Owned(format!("{}:{}", prefix, name.local_name)),
    None => Cow::Borrowed(name.local_name),
  }
}

fn push_attribute(start: &mut BytesStart<'_>, name: &str, value: &str) {
  let escaped = quick_xml::escape::escape(value);
  let escaped = if escaped.contains(['\t', '\n', '\r']) {
    Cow::Owned(
      escaped
        .replace('\t', "&#x9;")
        .replace('\n', "&#xA;")
        .replace('\r', "&#xD;"),
    )
  } else {
    escaped
  };
  // The byte-slice attribute constructor accepts already escaped content.
  start.push_attribute((name.as_bytes(), escaped.as_bytes()));
}
