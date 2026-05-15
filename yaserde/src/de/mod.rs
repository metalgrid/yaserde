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
use xml::reader::XmlEvent;

/// Lightweight event for generated deserializer fast paths. Extracted directly
/// from the internal cache without triggering namespace map or attribute
/// vector construction.
#[derive(Debug)]
pub enum LightEvent {
  StartElement {
    local_name: String,
    namespace: Option<String>,
  },
  EndElement {
    local_name: String,
  },
  Characters(String),
  EndDocument,
}

pub fn from_str<T: YaDeserialize>(s: &str) -> Result<T, String> {
  from_reader(s.as_bytes())
}

pub fn from_reader<R: Read, T: YaDeserialize>(reader: R) -> Result<T, String> {
  <T as YaDeserialize>::deserialize(&mut Deserializer::new_from_reader(reader))
}

/// Internal lightweight cached event. Stores name and attributes (extracted
/// eagerly because they require the resolver state at read time) but defers
/// namespace map construction until the public `XmlEvent` is materialized.
enum InternalEvent {
  StartElement {
    name: OwnedName,
    attributes: Vec<OwnedAttribute>,
  },
  EndElement {
    name: OwnedName,
  },
  Characters(String),
  EndDocument,
}

pub struct Deserializer<R: Read> {
  depth: usize,
  reader: NsReader<BufReader<R>>,
  buf: Vec<u8>,
  /// Lightweight cached event from the last read.
  cached: Option<InternalEvent>,
  /// Pending end-element names for expanded empty elements.
  pending_ends: VecDeque<OwnedName>,
  /// Full public XmlEvent, materialized on demand from `cached`.
  materialized: Option<XmlEvent>,
  eof: bool,
  started: bool,
}

impl<R: Read> Deserializer<R> {
  fn with_reader(reader: NsReader<BufReader<R>>) -> Self {
    Deserializer {
      depth: 0,
      reader,
      buf: Vec::new(),
      cached: None,
      pending_ends: VecDeque::new(),
      materialized: None,
      eof: false,
      started: false,
    }
  }

  pub fn new(reader: xml::reader::EventReader<R>) -> Self {
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

  /// Read the next quick-xml event and convert to an InternalEvent.
  /// This extracts name and attributes eagerly (they require the resolver
  /// state at read time) but does NOT build the namespace map.
  fn read_internal(&mut self) -> Result<InternalEvent, String> {
    loop {
      if let Some(name) = self.pending_ends.pop_front() {
        return Ok(InternalEvent::EndElement { name });
      }

      if self.eof {
        if self.started {
          return Ok(InternalEvent::EndDocument);
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
          let name = Self::qname_to_owned_name(
            bytes_start.name(),
            reader.resolver().resolve_element(bytes_start.name()).0,
          )?;
          let attributes = Self::attributes_to_owned(reader, &bytes_start)?;
          return Ok(InternalEvent::StartElement { name, attributes });
        }
        Event::Empty(bytes_start) => {
          self.started = true;
          let name = Self::qname_to_owned_name(
            bytes_start.name(),
            reader.resolver().resolve_element(bytes_start.name()).0,
          )?;
          let attributes = Self::attributes_to_owned(reader, &bytes_start)?;
          let end_name = name.clone();
          self.pending_ends.push_back(end_name);
          return Ok(InternalEvent::StartElement { name, attributes });
        }
        Event::End(bytes_end) => {
          let name = Self::qname_to_owned_name(
            bytes_end.name(),
            reader.resolver().resolve_element(bytes_end.name()).0,
          )?;
          return Ok(InternalEvent::EndElement { name });
        }
        Event::Text(bytes_text) => {
          let decoded = bytes_text.decode().map_err(|e| e.to_string())?;
          let unescaped = unescape(decoded.as_ref()).map_err(|e| e.to_string())?;
          let trimmed = unescaped.trim();
          if trimmed.is_empty() {
            continue;
          }
          return Ok(InternalEvent::Characters(trimmed.to_owned()));
        }
        Event::CData(bytes_cdata) => {
          let text = bytes_cdata
            .decode()
            .map_err(|e| e.to_string())?
            .into_owned();
          return Ok(InternalEvent::Characters(text));
        }
        Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_) => {}
        Event::Eof => {
          self.eof = true;
          if self.started {
            return Ok(InternalEvent::EndDocument);
          } else {
            return Err("Unexpected end of stream: no root element found".to_string());
          }
        }
        _ => {}
      }
    }
  }

  /// Ensure the internal cache is populated (either cached or materialized).
  fn ensure_cached(&mut self) -> Result<(), String> {
    if self.cached.is_none() && self.materialized.is_none() {
      let event = self.read_internal()?;
      self.cached = Some(event);
    }
    Ok(())
  }

  /// Materialize the cached InternalEvent into a full XmlEvent with namespace
  /// map. No-op if already materialized.
  fn materialize_event(&mut self) -> Result<(), String> {
    if self.materialized.is_some() {
      return Ok(());
    }
    self.ensure_cached()?;
    let cached = self.cached.take();
    if let Some(internal) = cached {
      let xml_event = match internal {
        InternalEvent::StartElement { name, attributes } => {
          let namespace = Self::build_namespace(&self.reader);
          XmlEvent::StartElement {
            name,
            attributes,
            namespace,
          }
        }
        InternalEvent::EndElement { name } => XmlEvent::EndElement { name },
        InternalEvent::Characters(s) => XmlEvent::Characters(s),
        InternalEvent::EndDocument => XmlEvent::EndDocument,
      };
      self.materialized = Some(xml_event);
    }
    Ok(())
  }

  // ── Public API ──────────────────────────────────────────────────────────

  /// Peek at the next event as a full xml-rs `XmlEvent`. Materializes the
  /// namespace map and attributes on first call; returns a reference to the
  /// cached materialized event on subsequent calls.
  pub fn peek(&mut self) -> Result<&XmlEvent, String> {
    self.materialize_event()?;
    self
      .materialized
      .as_ref()
      .ok_or_else(|| "unable to peek next item".to_string())
  }

  /// Peek at the next event in lightweight form. Does NOT trigger namespace
  /// map construction. This is the primary peek method for generated
  /// deserializer fast paths.
  pub fn peek_light(&mut self) -> Result<LightEvent, String> {
    self.ensure_cached()?;

    // If already materialized, extract from the full event.
    if let Some(ref event) = self.materialized {
      return Ok(light_from_xml_event(event));
    }

    // Extract from the lightweight cache — no namespace map built.
    match self.cached.as_ref() {
      Some(InternalEvent::StartElement { name, .. }) => Ok(LightEvent::StartElement {
        local_name: name.local_name.clone(),
        namespace: name.namespace.clone(),
      }),
      Some(InternalEvent::EndElement { name }) => Ok(LightEvent::EndElement {
        local_name: name.local_name.clone(),
      }),
      Some(InternalEvent::Characters(s)) => Ok(LightEvent::Characters(s.clone())),
      Some(InternalEvent::EndDocument) => Ok(LightEvent::EndDocument),
      None => Err("unable to peek next item".to_string()),
    }
  }

  /// Get the attributes of the peeked StartElement without building the
  /// namespace map. Returns `None` if the peeked event is not a StartElement.
  /// The returned vector is cloned from the internal cache.
  pub fn peek_attributes(&mut self) -> Result<Option<Vec<OwnedAttribute>>, String> {
    self.ensure_cached()?;

    if let Some(XmlEvent::StartElement { attributes, .. }) = &self.materialized {
      return Ok(Some(attributes.clone()));
    }

    match &self.cached {
      Some(InternalEvent::StartElement { attributes, .. }) => Ok(Some(attributes.clone())),
      _ => Ok(None),
    }
  }

  /// Consume and return the next full `XmlEvent`. If the event was
  /// materialized by a prior `peek()`, returns that. Otherwise materializes
  /// on the fly (including namespace map for StartElement).
  pub fn next_event(&mut self) -> Result<XmlEvent, String> {
    // If already materialized by peek(), consume it.
    if let Some(event) = self.materialized.take() {
      match event {
        XmlEvent::StartElement { .. } => self.depth += 1,
        XmlEvent::EndElement { .. } => self.depth -= 1,
        _ => {}
      }
      log::debug!("Fetched {:?}, new depth {}", event, self.depth);
      return Ok(event);
    }

    // Otherwise, consume from the lightweight cache and materialize inline.
    self.ensure_cached()?;
    let cached = self.cached.take();
    let event = match cached {
      Some(InternalEvent::StartElement { name, attributes }) => {
        let namespace = Self::build_namespace(&self.reader);
        self.depth += 1;
        XmlEvent::StartElement {
          name,
          attributes,
          namespace,
        }
      }
      Some(InternalEvent::EndElement { name }) => {
        self.depth -= 1;
        XmlEvent::EndElement { name }
      }
      Some(InternalEvent::Characters(s)) => XmlEvent::Characters(s),
      Some(InternalEvent::EndDocument) => XmlEvent::EndDocument,
      None => return Err("no event available".to_string()),
    };

    log::debug!("Fetched {:?}, new depth {}", event, self.depth);
    Ok(event)
  }

  /// Compatibility alias kept for any manual consumers. Prefer `next_event()`.
  pub fn inner_next(&mut self) -> Result<XmlEvent, String> {
    self.next_event()
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

  /// Lightweight alternative to `read_inner_value()` for simple text fields.
  /// Assumes the next event in the cache is a StartElement that has already
  /// been matched by the caller (e.g. via `peek_light()`). Consumes the
  /// start element, reads optional text content, and consumes the matching
  /// end element — all without materializing public `XmlEvent`s or namespace
  /// maps.
  ///
  /// Returns `Ok(Some(text))` for elements with text content, `Ok(None)` for
  /// empty elements (including `<foo />` self-closing tags). For empty
  /// elements, the EndElement is placed back in the lightweight cache so
  /// the caller's loop can handle depth tracking.
  pub fn read_inner_text_light(&mut self) -> Result<Option<String>, String> {
    self.ensure_cached()?;

    // Step 1: Consume the start element from whichever cache holds it.
    let start_name = if let Some(event) = self.materialized.take() {
      match event {
        XmlEvent::StartElement { name, .. } => {
          self.depth += 1;
          name
        }
        _ => return Err("Internal error: expected StartElement".to_string()),
      }
    } else {
      match self.cached.take() {
        Some(InternalEvent::StartElement { name, .. }) => {
          self.depth += 1;
          name
        }
        Some(other) => {
          self.cached = Some(other);
          return Err("Internal error: expected StartElement".to_string());
        }
        None => return Err("no event available".to_string()),
      }
    };

    // Step 2: Read next event — should be Characters or EndElement.
    let next = self.read_internal()?;
    match next {
      InternalEvent::Characters(s) => {
        // Step 3: Read matching end element.
        let end = self.read_internal()?;
        match &end {
          InternalEvent::EndElement { name } if *name == start_name => {
            self.depth -= 1;
          }
          InternalEvent::EndElement { name } => {
            self.depth -= 1;
            return Err(format!(
              "End tag </{}> didn't match the start tag <{}>",
              name.local_name, start_name.local_name
            ));
          }
          _ => {
            self.cached = Some(end);
            return Err(format!("Expected end tag </{}>", start_name.local_name));
          }
        }
        Ok(Some(s))
      }
      // Empty element: put EndElement back so the caller's loop handles
      // depth tracking (matches original `read_inner_value` behavior where
      // the closure fails and `expect_end_element` is never called).
      InternalEvent::EndElement { name } if name == start_name => {
        self.cached = Some(InternalEvent::EndElement { name });
        Ok(None)
      }
      InternalEvent::EndElement { name } => {
        let local = name.local_name.clone();
        self.cached = Some(InternalEvent::EndElement { name });
        Err(format!(
          "End tag </{}> didn't match the start tag <{}>",
          local, start_name.local_name
        ))
      }
      InternalEvent::StartElement { .. } => {
        self.cached = Some(next);
        Err(format!(
          "Expected text content in <{}>",
          start_name.local_name
        ))
      }
      InternalEvent::EndDocument => Err(format!(
        "Unexpected end of document in <{}>",
        start_name.local_name
      )),
    }
  }
}

fn light_from_xml_event(event: &XmlEvent) -> LightEvent {
  match event {
    XmlEvent::StartElement { name, .. } => LightEvent::StartElement {
      local_name: name.local_name.clone(),
      namespace: name.namespace.clone(),
    },
    XmlEvent::EndElement { name } => LightEvent::EndElement {
      local_name: name.local_name.clone(),
    },
    XmlEvent::Characters(s) => LightEvent::Characters(s.clone()),
    XmlEvent::EndDocument => LightEvent::EndDocument,
    other => panic!("unexpected event: {:?}", other),
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
