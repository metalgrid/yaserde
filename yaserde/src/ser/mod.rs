//! Generic data structure serialization framework.
//!

use crate::YaSerialize;
use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::collections::HashMap;
use std::io::{Cursor, Error as IoError, Write};
use std::str;
use xml::name::{Name, OwnedName};
use xml::writer::{EventWriter, XmlEvent};

/// Serialize XML into a plain String with no formatting (EmitterConfig).
pub fn to_string<T: YaSerialize>(model: &T) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer(model, buf, &Config::default())?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

/// Serialize XML into a plain String with control on formatting (via EmitterConfig parameters)
pub fn to_string_with_config<T: YaSerialize>(model: &T, config: &Config) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer(model, buf, config)?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

pub fn serialize_with_writer<W: Write, T: YaSerialize>(
  model: &T,
  mut writer: W,
  config: &Config,
) -> Result<W, String> {
  if config.perform_indent {
    let buffer = Cursor::new(Vec::new());
    let unformatted_config = Config {
      perform_indent: false,
      write_document_declaration: config.write_document_declaration,
      indent_string: None,
    };
    let cursor = serialize_with_writer(model, buffer, &unformatted_config)?;
    let xml = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
    let formatted = format_xml(xml, config.indent_string.as_deref().unwrap_or("  "))?;
    writer
      .write_all(formatted.as_bytes())
      .map_err(|e| e.to_string())?;
    return Ok(writer);
  }

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
  writer: Writer<W>,
  element_stack: Vec<String>,
  write_document_declaration: bool,
  pending_start: Option<(String, BytesStart<'static>)>,
  skip_start_end: bool,
  start_event_name: Option<String>,
  namespace_stack: Vec<HashMap<String, String>>,
  use_xml_rs_public_defaults: bool,
}

impl<W: Write> Serializer<W> {
  pub fn new(writer: EventWriter<W>) -> Self {
    let mut writer = Writer::new(writer.into_inner());
    writer.config_mut().add_space_before_slash_in_empty_elements = true;
    let mut serializer = Self::new_quick(writer);
    serializer.use_xml_rs_public_defaults = true;
    serializer
  }

  fn new_quick(writer: Writer<W>) -> Self {
    Serializer {
      writer,
      element_stack: Vec::new(),
      write_document_declaration: false,
      pending_start: None,
      skip_start_end: false,
      start_event_name: None,
      namespace_stack: Vec::new(),
      use_xml_rs_public_defaults: false,
    }
  }

  pub fn new_from_writer(writer: W, config: &Config) -> Self {
    let writer = build_writer(writer);
    let mut serializer = Self::new_quick(writer);

    if config.write_document_declaration {
      serializer.write_document_declaration = true;
      serializer
        .writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))
        .expect("failed to write XML declaration");
    }

    serializer
  }

  pub fn new_for_inner(writer: W) -> Self {
    let mut writer = Writer::new(writer);
    writer.config_mut().add_space_before_slash_in_empty_elements = true;
    Self::new_quick(writer)
  }

  pub fn into_inner(mut self) -> W {
    self
      .flush_pending_start()
      .expect("failed to finalize XML output");
    self.writer.into_inner()
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

  pub fn write<'a, E>(&mut self, event: E) -> xml::writer::Result<()>
  where
    E: Into<XmlEvent<'a>>,
  {
    let event = event.into();

    if self.use_xml_rs_public_defaults
      && !self.write_document_declaration
      && !matches!(event, XmlEvent::StartDocument { .. })
    {
      self
        .writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(map_io_error)?;
      self.write_document_declaration = true;
    }

    match event {
      XmlEvent::StartElement {
        name,
        attributes,
        namespace,
      } => {
        self.flush_pending_start()?;

        let tag = format_name(&name);
        let mut bs = BytesStart::new(tag.clone());
        let mut namespace_scope = self.namespace_stack.last().cloned().unwrap_or_default();

        for (prefix, uri) in namespace.as_ref().iter() {
          if prefix == "xml" || prefix == "xmlns" {
            continue;
          }

          if namespace_scope
            .get(prefix)
            .map(|existing| existing == uri)
            .unwrap_or(false)
          {
            continue;
          }

          if prefix.is_empty() {
            bs.push_attribute(("xmlns", uri));
          } else {
            let attr_name = format!("xmlns:{}", prefix);
            bs.push_attribute((attr_name.as_str(), uri));
          }

          namespace_scope.insert(prefix.to_string(), uri.to_string());
        }

        for attr in attributes.iter() {
          let attr_name = format_name(&attr.name);
          bs.push_attribute((attr_name.as_str(), attr.value));
        }

        self.element_stack.push(tag.clone());
        self.namespace_stack.push(namespace_scope);
        self.pending_start = Some((tag, bs.into_owned()));
      }
      XmlEvent::EndElement { name } => {
        let end_tag = name
          .map(|n| format_owned_name(&n.to_owned()))
          .or_else(|| self.element_stack.last().cloned())
          .unwrap_or_default();

        if let Some((pending_tag, pending_start)) = self.pending_start.take() {
          if pending_tag == end_tag {
            self.element_stack.pop();
            self.namespace_stack.pop();
            self
              .writer
              .write_event(Event::Empty(pending_start))
              .map_err(map_io_error)?;
            return Ok(());
          }

          self
            .writer
            .write_event(Event::Start(pending_start))
            .map_err(map_io_error)?;
        }

        if self
          .element_stack
          .last()
          .map(|tag| tag == &end_tag)
          .unwrap_or(false)
        {
          self.element_stack.pop();
          self.namespace_stack.pop();
        }

        self
          .writer
          .write_event(Event::End(BytesEnd::new(end_tag)))
          .map_err(map_io_error)?;
      }
      XmlEvent::Characters(data) => {
        self.flush_pending_start()?;
        let escaped = quick_xml::escape::partial_escape(&*data);
        self
          .writer
          .write_event(Event::Text(BytesText::from_escaped(escaped)))
          .map_err(map_io_error)?;
      }
      XmlEvent::CData(data) => {
        self.flush_pending_start()?;
        self
          .writer
          .write_event(Event::CData(BytesCData::new(data)))
          .map_err(map_io_error)?;
      }
      XmlEvent::StartDocument {
        version: _,
        encoding,
        standalone,
      } => {
        self.flush_pending_start()?;

        if !self.write_document_declaration {
          let enc = encoding.unwrap_or(if self.use_xml_rs_public_defaults {
            "UTF-8"
          } else {
            "utf-8"
          });
          let standalone_attr = standalone.map(|s| if s { "yes" } else { "no" });
          self
            .writer
            .write_event(Event::Decl(BytesDecl::new(
              "1.0",
              Some(enc),
              standalone_attr,
            )))
            .map_err(map_io_error)?;
          self.write_document_declaration = true;
        }
      }
      XmlEvent::ProcessingInstruction { name, data } => {
        self.flush_pending_start()?;
        let content = match data {
          Some(d) => format!("{} {}", name, d),
          None => name.to_string(),
        };
        self
          .writer
          .write_event(Event::PI(quick_xml::events::BytesPI::new(content.as_str())))
          .map_err(map_io_error)?;
      }
      XmlEvent::Comment(data) => {
        self.flush_pending_start()?;
        self
          .writer
          .write_event(Event::Comment(BytesText::from_escaped(data)))
          .map_err(map_io_error)?;
      }
    }

    Ok(())
  }

  fn flush_pending_start(&mut self) -> xml::writer::Result<()> {
    if let Some((_, pending_start)) = self.pending_start.take() {
      self
        .writer
        .write_event(Event::Start(pending_start))
        .map_err(map_io_error)?;
    }

    Ok(())
  }
}

fn build_writer<W: Write>(writer: W) -> Writer<W> {
  let mut writer = Writer::new(writer);

  writer.config_mut().add_space_before_slash_in_empty_elements = true;
  writer
}

fn format_xml(xml: &str, indent: &str) -> Result<String, String> {
  let mut reader = quick_xml::Reader::from_str(xml);
  reader.config_mut().trim_text(false);
  reader.config_mut().expand_empty_elements = false;

  let mut writer = Writer::new(Cursor::new(Vec::new()));
  writer.config_mut().add_space_before_slash_in_empty_elements = true;
  let mut buf = Vec::new();
  let mut depth = 0usize;
  let mut wrote_any = false;
  let mut last_was_text = false;

  loop {
    buf.clear();
    match reader
      .read_event_into(&mut buf)
      .map_err(|e| e.to_string())?
    {
      Event::Decl(event) => {
        writer
          .write_event(Event::Decl(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = false;
      }
      Event::Start(event) => {
        if wrote_any && !last_was_text {
          write_indent(&mut writer, indent, depth)?;
        }
        writer
          .write_event(Event::Start(event.into_owned()))
          .map_err(|e| e.to_string())?;
        depth += 1;
        wrote_any = true;
        last_was_text = false;
      }
      Event::Empty(event) => {
        if wrote_any && !last_was_text {
          write_indent(&mut writer, indent, depth)?;
        }
        writer
          .write_event(Event::Empty(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = false;
      }
      Event::End(event) => {
        depth = depth.saturating_sub(1);
        if !last_was_text {
          write_indent(&mut writer, indent, depth)?;
        }
        writer
          .write_event(Event::End(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = false;
      }
      Event::Text(event) => {
        writer
          .write_event(Event::Text(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = true;
      }
      Event::CData(event) => {
        writer
          .write_event(Event::CData(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = true;
      }
      Event::Comment(event) => {
        if wrote_any && !last_was_text {
          write_indent(&mut writer, indent, depth)?;
        }
        writer
          .write_event(Event::Comment(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = false;
      }
      Event::PI(event) => {
        if wrote_any && !last_was_text {
          write_indent(&mut writer, indent, depth)?;
        }
        writer
          .write_event(Event::PI(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = false;
      }
      Event::DocType(event) => {
        if wrote_any && !last_was_text {
          write_indent(&mut writer, indent, depth)?;
        }
        writer
          .write_event(Event::DocType(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = false;
      }
      Event::GeneralRef(event) => {
        writer
          .write_event(Event::GeneralRef(event.into_owned()))
          .map_err(|e| e.to_string())?;
        wrote_any = true;
        last_was_text = true;
      }
      Event::Eof => break,
    }
  }

  let bytes = writer.into_inner().into_inner();
  String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn write_indent<W: Write>(
  writer: &mut Writer<W>,
  indent: &str,
  depth: usize,
) -> Result<(), String> {
  writer
    .get_mut()
    .write_all(b"\n")
    .map_err(|e| e.to_string())?;
  for _ in 0..depth {
    writer
      .get_mut()
      .write_all(indent.as_bytes())
      .map_err(|e| e.to_string())?;
  }
  Ok(())
}

fn format_name(name: &Name<'_>) -> String {
  match name.prefix {
    Some(prefix) => format!("{}:{}", prefix, name.local_name),
    None => name.local_name.to_string(),
  }
}

fn format_owned_name(name: &OwnedName) -> String {
  match &name.prefix {
    Some(prefix) => format!("{}:{}", prefix, name.local_name),
    None => name.local_name.clone(),
  }
}

fn map_io_error(error: IoError) -> xml::writer::Error {
  xml::writer::Error::Io(error)
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
