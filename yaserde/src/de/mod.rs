//! Generic data structure deserialization framework.
//!

use crate::YaDeserialize;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{PrefixDeclaration, QName, ResolveResult};
use quick_xml::NsReader;
use std::collections::VecDeque;
use std::io::{BufReader, Read};
use xml::attribute::OwnedAttribute;
use xml::name::OwnedName;
use xml::namespace::Namespace;
use xml::reader::{EventReader, XmlEvent};

pub fn from_str<T: YaDeserialize>(s: &str) -> Result<T, String> {
  from_reader(s.as_bytes())
}

pub fn from_reader<R: Read, T: YaDeserialize>(reader: R) -> Result<T, String> {
  <T as YaDeserialize>::deserialize(&mut Deserializer::new_from_reader(reader))
}

pub struct Deserializer<R: Read> {
  depth: usize,
  reader: NsReader<BufReader<R>>,
  buf: Vec<u8>,
  peeked: Option<XmlEvent>,
  pending: VecDeque<XmlEvent>,
  eof: bool,
  started: bool,
}

impl<R: Read> Deserializer<R> {
  fn with_reader(reader: NsReader<BufReader<R>>) -> Self {
    Deserializer {
      depth: 0,
      reader,
      buf: Vec::new(),
      peeked: None,
      pending: VecDeque::new(),
      eof: false,
      started: false,
    }
  }

  pub fn new(reader: EventReader<R>) -> Self {
    Self::new_from_reader(reader.into_inner())
  }

  pub fn new_from_reader(reader: R) -> Self {
    let mut reader = NsReader::from_reader(BufReader::new(reader));
    let config = reader.config_mut();
    config.trim_text(false);
    config.expand_empty_elements = false;
    config.check_end_names = true;

    Self::with_reader(reader)
  }

  fn qname_to_owned_name(
    qname: QName<'_>,
    resolve: ResolveResult<'_>,
  ) -> Result<OwnedName, String> {
    let (local_name, prefix) = qname.decompose();
    let namespace = match resolve {
      ResolveResult::Bound(namespace) => {
        Some(String::from_utf8_lossy(namespace.as_ref()).into_owned())
      }
      ResolveResult::Unbound => None,
      ResolveResult::Unknown(prefix) => {
        return Err(format!(
          "unknown namespace prefix '{}'",
          String::from_utf8_lossy(&prefix)
        ));
      }
    };

    Ok(OwnedName {
      local_name: String::from_utf8_lossy(local_name.as_ref()).into_owned(),
      namespace,
      prefix: prefix.map(|prefix| String::from_utf8_lossy(prefix.as_ref()).into_owned()),
    })
  }

  fn build_namespace(reader: &NsReader<BufReader<R>>) -> Namespace {
    let mut namespace = Namespace::empty();

    for (prefix, uri) in reader.resolver().bindings() {
      let prefix = match prefix {
        PrefixDeclaration::Default => String::new(),
        PrefixDeclaration::Named(prefix) => String::from_utf8_lossy(prefix).into_owned(),
      };
      let uri = String::from_utf8_lossy(uri.as_ref()).into_owned();
      namespace.force_put(prefix, uri);
    }

    namespace
  }

  fn attributes_to_owned(
    reader: &NsReader<BufReader<R>>,
    bytes_start: &BytesStart<'_>,
  ) -> Result<Vec<OwnedAttribute>, String> {
    let mut attributes = Vec::new();

    for attribute in bytes_start.attributes() {
      let attribute = attribute.map_err(|e| e.to_string())?;
      if attribute.key.as_namespace_binding().is_some() {
        continue;
      }

      let name = Self::qname_to_owned_name(
        attribute.key,
        reader.resolver().resolve_attribute(attribute.key).0,
      )?;
      let value = attribute
        .unescape_value()
        .map_err(|e| e.to_string())?
        .into_owned();

      attributes.push(OwnedAttribute { name, value });
    }

    Ok(attributes)
  }

  fn start_element_to_xml_event(
    reader: &NsReader<BufReader<R>>,
    bytes_start: &BytesStart<'_>,
  ) -> Result<XmlEvent, String> {
    let qname = bytes_start.name();
    let name = Self::qname_to_owned_name(qname, reader.resolver().resolve_element(qname).0)?;
    let attributes = Self::attributes_to_owned(reader, bytes_start)?;
    let namespace = Self::build_namespace(reader);

    Ok(XmlEvent::StartElement {
      name,
      attributes,
      namespace,
    })
  }

  pub fn peek(&mut self) -> Result<&XmlEvent, String> {
    if self.peeked.is_none() {
      let event = self.inner_next()?;
      self.peeked = Some(event);
    }

    if let Some(ref next) = self.peeked {
      Ok(next)
    } else {
      Err("unable to peek next item".into())
    }
  }

  pub fn inner_next(&mut self) -> Result<XmlEvent, String> {
    loop {
      if let Some(event) = self.pending.pop_front() {
        return Ok(event);
      }

      if self.eof {
        if self.started {
          return Ok(XmlEvent::EndDocument);
        } else {
          return Err("Unexpected end of stream: no root element found".to_string());
        }
      }

      self.buf.clear();
      let reader = &mut self.reader;
      let next = reader
        .read_event_into(&mut self.buf)
        .map_err(|e| translate_error(&e))?;
      match next {
        Event::Start(bytes_start) => {
          self.started = true;
          return Self::start_element_to_xml_event(reader, &bytes_start);
        }
        Event::Empty(bytes_start) => {
          self.started = true;
          let start_event = Self::start_element_to_xml_event(reader, &bytes_start)?;
          let qname = bytes_start.name();
          let name = Self::qname_to_owned_name(qname, reader.resolver().resolve_element(qname).0)?;
          self.pending.push_back(XmlEvent::EndElement { name });
          return Ok(start_event);
        }
        Event::End(bytes_end) => {
          let qname = bytes_end.name();
          let name = Self::qname_to_owned_name(qname, reader.resolver().resolve_element(qname).0)?;
          return Ok(XmlEvent::EndElement { name });
        }
        Event::Text(bytes_text) => {
          let decoded = bytes_text.decode().map_err(|e| e.to_string())?;
          let unescaped = unescape(decoded.as_ref()).map_err(|e| e.to_string())?;
          let trimmed = unescaped.trim();
          if trimmed.is_empty() {
            continue;
          }
          return Ok(XmlEvent::Characters(trimmed.to_owned()));
        }
        Event::CData(bytes_cdata) => {
          let text = bytes_cdata
            .decode()
            .map_err(|e| e.to_string())?
            .into_owned();
          return Ok(XmlEvent::Characters(text));
        }
        Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_) => {}
        Event::Eof => {
          self.eof = true;
          if self.started {
            return Ok(XmlEvent::EndDocument);
          } else {
            return Err("Unexpected end of stream: no root element found".to_string());
          }
        }
        _ => {}
      }
    }
  }

  pub fn next_event(&mut self) -> Result<XmlEvent, String> {
    let next_event = if let Some(peeked) = self.peeked.take() {
      peeked
    } else {
      self.inner_next()?
    };
    match next_event {
      XmlEvent::StartElement { .. } => {
        self.depth += 1;
      }
      XmlEvent::EndElement { .. } => {
        self.depth -= 1;
      }
      _ => {}
    }
    log::debug!("Fetched {:?}, new depth {}", next_event, self.depth);
    Ok(next_event)
  }

  pub fn skip_element(&mut self, mut cb: impl FnMut(&XmlEvent)) -> Result<(), String> {
    let depth = self.depth;

    while self.depth >= depth {
      cb(&self.next_event()?);
    }

    Ok(())
  }

  pub fn depth(&self) -> usize {
    self.depth
  }

  pub fn read_inner_value<T, F: FnOnce(&mut Self) -> Result<T, String>>(
    &mut self,
    f: F,
  ) -> Result<T, String> {
    if let Ok(XmlEvent::StartElement { name, .. }) = self.next_event() {
      let result = f(self)?;
      self.expect_end_element(&name)?;
      Ok(result)
    } else {
      Err("Internal error: Bad Event".to_string())
    }
  }

  pub fn expect_end_element(&mut self, start_name: &OwnedName) -> Result<(), String> {
    if let XmlEvent::EndElement { name, .. } = self.next_event()? {
      if name == *start_name {
        Ok(())
      } else {
        Err(format!(
          "End tag </{}> didn't match the start tag <{}>",
          name.local_name, start_name.local_name
        ))
      }
    } else {
      Err(format!("Unexpected token </{}>", start_name.local_name))
    }
  }
}

fn translate_error(e: &quick_xml::Error) -> String {
  let msg = e.to_string();
  if let Some(rest) = msg.strip_prefix("ill-formed document: expected `</") {
    if let Some(end_idx) = rest.find(">`") {
      let expected = &rest[..end_idx];
      let remaining = &rest[end_idx + 2..];
      if let Some(found_start) = remaining.find("but `</") {
        let after = &remaining[found_start + 7..];
        if let Some(end_idx2) = after.find(">`") {
          let found = &after[..end_idx2];
          return format!("Unexpected closing tag: {} != {}", found, expected);
        }
      }
    }
  }
  msg
}
