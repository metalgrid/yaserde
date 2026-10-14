//! Generic data structure serialization framework.
//!

use crate::xml::{XmlAttribute, XmlNamespace, XmlWriteEvent};
use crate::YaSerialize;
use ::xml::writer::XmlEvent;
#[cfg(not(feature = "quick-xml-backend"))]
use ::xml::EmitterConfig;
use ::xml::EventWriter;
use std::io::{Cursor, Write};
use std::str;

#[cfg(feature = "quick-xml-backend")]
mod quick_xml;

enum Backend<W: Write> {
  XmlRs(EventWriter<W>),
  #[cfg(feature = "quick-xml-backend")]
  Quick(quick_xml::QuickWriter<W>),
}

/// Serialize XML into a plain String with no formatting.
pub fn to_string<T: YaSerialize>(model: &T) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer(model, buf, &Config::default())?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

/// Serialize XML into a plain String with control on formatting.
pub fn to_string_with_config<T: YaSerialize>(model: &T, config: &Config) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer(model, buf, config)?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

pub fn serialize_with_writer<W: Write, T: YaSerialize>(
  model: &T,
  writer: W,
  config: &Config,
) -> Result<W, String> {
  let mut serializer = Serializer::new_from_writer(writer, config);
  match YaSerialize::serialize(model, &mut serializer) {
    Ok(()) => Ok(serializer.into_inner()),
    Err(msg) => Err(msg),
  }
}

pub fn to_string_content<T: YaSerialize>(model: &T) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer_content(model, buf)?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

pub fn serialize_with_writer_content<W: Write, T: YaSerialize>(
  model: &T,
  writer: W,
) -> Result<W, String> {
  let mut serializer = Serializer::new_for_inner(writer);
  serializer.set_skip_start_end(true);
  match YaSerialize::serialize(model, &mut serializer) {
    Ok(()) => Ok(serializer.into_inner()),
    Err(msg) => Err(msg),
  }
}

pub struct Serializer<W: Write> {
  writer: Backend<W>,
  skip_start_end: bool,
  start_event_name: Option<String>,
}

impl<W: Write> Serializer<W> {
  /// Retain an explicitly supplied xml-rs writer, including its configuration and state.
  pub fn new(writer: EventWriter<W>) -> Self {
    Serializer {
      writer: Backend::XmlRs(writer),
      skip_start_end: false,
      start_event_name: None,
    }
  }

  /// Use quick-xml when `quick-xml-backend` is enabled, otherwise xml-rs.
  pub fn new_from_writer(writer: W, config: &Config) -> Self {
    #[cfg(feature = "quick-xml-backend")]
    {
      Self {
        writer: Backend::Quick(quick_xml::QuickWriter::new(writer, config)),
        skip_start_end: false,
        start_event_name: None,
      }
    }
    #[cfg(not(feature = "quick-xml-backend"))]
    {
      let mut emitter_config = EmitterConfig::new()
        .cdata_to_characters(false)
        .perform_indent(config.perform_indent)
        .write_document_declaration(config.write_document_declaration);

      if let Some(indent_string_value) = &config.indent_string {
        emitter_config = emitter_config.indent_string(indent_string_value.clone());
      }

      Self::new(EventWriter::new_with_config(writer, emitter_config))
    }
  }

  pub fn new_for_inner(writer: W) -> Self {
    Self::new_from_writer(
      writer,
      &Config {
        write_document_declaration: false,
        ..Config::default()
      },
    )
  }

  pub fn into_inner(self) -> W {
    match self.writer {
      Backend::XmlRs(writer) => writer.into_inner(),
      #[cfg(feature = "quick-xml-backend")]
      Backend::Quick(writer) => writer.into_inner(),
    }
  }

  pub fn skip_start_end(&self) -> bool {
    self.skip_start_end
  }

  pub fn set_skip_start_end(&mut self, state: bool) {
    self.skip_start_end = state;
  }

  pub fn get_start_event_name(&self) -> Option<String> {
    self.start_event_name.clone()
  }

  pub fn set_start_event_name(&mut self, name: Option<String>) {
    self.start_event_name = name;
  }

  pub fn write<'a, E>(&mut self, event: E) -> ::xml::writer::Result<()>
  where
    E: Into<XmlEvent<'a>>,
  {
    match &mut self.writer {
      Backend::XmlRs(writer) => writer.write(event),
      #[cfg(feature = "quick-xml-backend")]
      Backend::Quick(writer) => writer.write(event.into()),
    }
  }

  pub fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String> {
    match event {
      XmlWriteEvent::StartElement {
        name,
        attributes,
        namespace,
      } => self.write_start_element(name.into_owned(), attributes, namespace),
      XmlWriteEvent::EndElement => self.write_end_element(),
      XmlWriteEvent::Characters(text) => self.write_characters(&text),
      XmlWriteEvent::CData(text) => self.write_cdata(&text),
    }
  }

  pub fn write_start_element<S: Into<String>>(
    &mut self,
    name: S,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  ) -> Result<(), String> {
    let name = name.into();
    #[cfg(feature = "quick-xml-backend")]
    if let Backend::Quick(writer) = &mut self.writer {
      return writer
        .start(
          ::xml::name::Name::local(&name),
          attributes.iter().map(|a| {
            (
              ::xml::name::Name {
                local_name: &a.name.local_name,
                namespace: a.name.namespace.as_deref(),
                prefix: a.name.prefix.as_deref(),
              },
              a.value.as_str(),
            )
          }),
          namespace
            .0
            .iter()
            .map(|(prefix, uri)| (prefix.as_str(), uri.as_str())),
        )
        .map_err(|e| e.to_string());
    }
    let name = ::xml::name::OwnedName::local(name);
    let attributes: Vec<_> = attributes
      .iter()
      .map(|attribute| attribute.to_xml_rs())
      .collect();
    let attributes = attributes
      .iter()
      .map(|attribute| attribute.borrow())
      .collect();
    self
      .write(::xml::writer::events::XmlEvent::StartElement {
        name: name.borrow(),
        attributes: ::std::borrow::Cow::Owned(attributes),
        namespace: ::std::borrow::Cow::Owned(namespace.to_xml_rs()),
      })
      .map_err(|e| e.to_string())
  }

  pub fn write_end_element(&mut self) -> Result<(), String> {
    self
      .write(::xml::writer::XmlEvent::end_element())
      .map_err(|e| e.to_string())
  }

  pub fn write_characters(&mut self, text: &str) -> Result<(), String> {
    self
      .write(::xml::writer::XmlEvent::characters(text))
      .map_err(|e| e.to_string())
  }

  pub fn write_cdata(&mut self, text: &str) -> Result<(), String> {
    self
      .write(::xml::writer::events::XmlEvent::cdata(text))
      .map_err(|e| e.to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn constructors_select_and_retain_the_backend() {
    let mut selected = Serializer::new_from_writer(Vec::new(), &Config::default());
    let inner = Serializer::new_for_inner(Vec::new());
    #[cfg(feature = "quick-xml-backend")]
    {
      assert!(matches!(selected.writer, Backend::Quick(_)));
      assert!(matches!(inner.writer, Backend::Quick(_)));
    }
    #[cfg(not(feature = "quick-xml-backend"))]
    {
      assert!(matches!(selected.writer, Backend::XmlRs(_)));
      assert!(matches!(inner.writer, Backend::XmlRs(_)));
    }
    selected.write(XmlEvent::start_element("legacy")).unwrap();
    selected.write_characters("neutral").unwrap();
    selected.write(XmlEvent::end_element()).unwrap();
    #[cfg(feature = "quick-xml-backend")]
    assert!(matches!(selected.writer, Backend::Quick(_)));
    let explicit = Serializer::new(EventWriter::new(Vec::new()));
    assert!(matches!(explicit.writer, Backend::XmlRs(_)));
  }
}

pub struct Config {
  pub perform_indent: bool,
  pub write_document_declaration: bool,
  pub indent_string: Option<String>,
}

impl Default for Config {
  fn default() -> Self {
    Config {
      perform_indent: false,
      write_document_declaration: true,
      indent_string: None,
    }
  }
}
