use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Read;

/// Parser-neutral owned XML name used by YaSerDe.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct XmlName {
  pub local_name: String,
  pub namespace: Option<String>,
  pub prefix: Option<String>,
}

impl XmlName {
  pub fn local<S: Into<String>>(local_name: S) -> Self {
    Self {
      local_name: local_name.into(),
      namespace: None,
      prefix: None,
    }
  }

  pub fn qualified<S1, S2, S3>(local_name: S1, namespace: S2, prefix: Option<S3>) -> Self
  where
    S1: Into<String>,
    S2: Into<String>,
    S3: Into<String>,
  {
    Self {
      local_name: local_name.into(),
      namespace: Some(namespace.into()),
      prefix: prefix.map(Into::into),
    }
  }

  pub fn prefix_ref(&self) -> Option<&str> {
    self.prefix.as_deref()
  }

  pub fn namespace_ref(&self) -> Option<&str> {
    self.namespace.as_deref()
  }

  pub(crate) fn to_xml_rs(&self) -> ::xml::name::OwnedName {
    ::xml::name::OwnedName {
      local_name: self.local_name.clone(),
      namespace: self.namespace.clone(),
      prefix: self.prefix.clone(),
    }
  }
}

impl std::fmt::Display for XmlName {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Some(prefix) = &self.prefix {
      write!(f, "{}:{}", prefix, self.local_name)
    } else {
      write!(f, "{}", self.local_name)
    }
  }
}

impl From<::xml::name::OwnedName> for XmlName {
  fn from(name: ::xml::name::OwnedName) -> Self {
    Self {
      local_name: name.local_name,
      namespace: name.namespace,
      prefix: name.prefix,
    }
  }
}

/// Parser-neutral owned XML attribute used by YaSerDe.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct XmlAttribute {
  pub name: XmlName,
  pub value: String,
}

impl XmlAttribute {
  pub fn new<S: Into<String>>(name: XmlName, value: S) -> Self {
    Self {
      name,
      value: value.into(),
    }
  }

  pub(crate) fn to_xml_rs(&self) -> ::xml::attribute::OwnedAttribute {
    ::xml::attribute::OwnedAttribute::new(self.name.to_xml_rs(), self.value.clone())
  }
}

impl From<::xml::attribute::OwnedAttribute> for XmlAttribute {
  fn from(attribute: ::xml::attribute::OwnedAttribute) -> Self {
    Self {
      name: attribute.name.into(),
      value: attribute.value,
    }
  }
}

/// Parser-neutral namespace mapping.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct XmlNamespace(pub BTreeMap<String, String>);

impl XmlNamespace {
  pub fn empty() -> Self {
    Self(BTreeMap::new())
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  pub fn put<P, U>(&mut self, prefix: P, uri: U) -> bool
  where
    P: Into<String>,
    U: Into<String>,
  {
    self.0.insert(prefix.into(), uri.into()).is_none()
  }

  pub fn force_put<P, U>(&mut self, prefix: P, uri: U) -> Option<String>
  where
    P: Into<String>,
    U: Into<String>,
  {
    self.0.insert(prefix.into(), uri.into())
  }

  pub fn extend(&mut self, other: &Self) {
    self.0.extend(other.0.clone());
  }

  pub(crate) fn to_xml_rs(&self) -> ::xml::namespace::Namespace {
    ::xml::namespace::Namespace(self.0.clone())
  }
}

impl From<::xml::namespace::Namespace> for XmlNamespace {
  fn from(namespace: ::xml::namespace::Namespace) -> Self {
    Self(namespace.0)
  }
}

/// Parser-neutral XML events produced by YaSerDe reader backends.
#[derive(Clone, Debug, PartialEq)]
pub enum XmlReadEvent {
  StartElement {
    name: XmlName,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  },
  EndElement {
    name: XmlName,
  },
  Characters(String),
  EndDocument,
}

impl XmlReadEvent {
  pub fn write_to_xml_rs<W: std::io::Write>(
    &self,
    writer: &mut ::xml::writer::EventWriter<W>,
  ) -> ::xml::writer::Result<()> {
    match self {
      XmlReadEvent::StartElement {
        name,
        attributes,
        namespace,
      } => {
        let name = name.to_xml_rs();
        let attributes: Vec<_> = attributes.iter().map(XmlAttribute::to_xml_rs).collect();
        let borrowed_attributes: Vec<_> = attributes.iter().map(|a| a.borrow()).collect();
        let namespace = namespace.to_xml_rs();
        writer.write(::xml::writer::events::XmlEvent::StartElement {
          name: name.borrow(),
          attributes: Cow::Owned(borrowed_attributes),
          namespace: Cow::Owned(namespace),
        })
      }
      XmlReadEvent::EndElement { name } => {
        let name = name.to_xml_rs();
        writer.write(::xml::writer::events::XmlEvent::EndElement {
          name: Some(name.borrow()),
        })
      }
      XmlReadEvent::Characters(text) => writer.write(::xml::writer::XmlEvent::characters(text)),
      XmlReadEvent::EndDocument => Ok(()),
    }
  }
}

/// Parser-neutral lightweight XML event for generated deserializer fast paths.
#[derive(Clone, Debug, PartialEq)]
pub enum XmlLightEvent {
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

impl From<&XmlReadEvent> for XmlLightEvent {
  fn from(event: &XmlReadEvent) -> Self {
    match event {
      XmlReadEvent::StartElement { name, .. } => XmlLightEvent::StartElement {
        local_name: name.local_name.clone(),
        namespace: name.namespace.clone(),
      },
      XmlReadEvent::EndElement { name } => XmlLightEvent::EndElement {
        local_name: name.local_name.clone(),
      },
      XmlReadEvent::Characters(text) => XmlLightEvent::Characters(text.clone()),
      XmlReadEvent::EndDocument => XmlLightEvent::EndDocument,
    }
  }
}

/// Parser-neutral XML write events. Serialization still uses the xml-rs writer backend
/// initially, but this type is the boundary for backend-neutral writers.
#[derive(Clone, Debug, PartialEq)]
pub enum XmlWriteEvent<'a> {
  StartElement {
    name: Cow<'a, str>,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  },
  EndElement,
  Characters(Cow<'a, str>),
  CData(Cow<'a, str>),
}

/// Low-level pull-reader trait implemented by XML parser backends.
pub trait XmlEventReader {
  fn next_event(&mut self) -> Result<XmlReadEvent, String>;

  fn supports_light_events(&self) -> bool {
    false
  }

  fn peek_light_event(&mut self) -> Result<XmlLightEvent, String> {
    Err("light events are not supported by this backend".to_string())
  }

  fn next_light_event(&mut self) -> Result<XmlLightEvent, String> {
    Ok(XmlLightEvent::from(&self.next_event()?))
  }

  fn peek_attributes(&mut self) -> Result<Option<Vec<XmlAttribute>>, String> {
    Err("attribute peeking is not supported by this backend".to_string())
  }

  fn read_inner_text(&mut self) -> Result<Option<String>, String> {
    Err("inner text fast path is not supported by this backend".to_string())
  }
}

impl<T: XmlEventReader + ?Sized> XmlEventReader for Box<T> {
  fn next_event(&mut self) -> Result<XmlReadEvent, String> {
    self.as_mut().next_event()
  }

  fn supports_light_events(&self) -> bool {
    self.as_ref().supports_light_events()
  }

  fn peek_light_event(&mut self) -> Result<XmlLightEvent, String> {
    self.as_mut().peek_light_event()
  }

  fn next_light_event(&mut self) -> Result<XmlLightEvent, String> {
    self.as_mut().next_light_event()
  }

  fn peek_attributes(&mut self) -> Result<Option<Vec<XmlAttribute>>, String> {
    self.as_mut().peek_attributes()
  }

  fn read_inner_text(&mut self) -> Result<Option<String>, String> {
    self.as_mut().read_inner_text()
  }
}

/// Low-level writer trait for XML emitter backends.
pub trait XmlEventWriter {
  fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String>;
}

impl<T: XmlEventWriter + ?Sized> XmlEventWriter for Box<T> {
  fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String> {
    self.as_mut().write_event(event)
  }
}

/// `xml-rs` reader adapter. This preserves the parser configuration YaSerDe used
/// before introducing parser abstraction.
pub struct XmlRsReader<R: Read> {
  inner: ::xml::reader::EventReader<R>,
}

impl<R: Read> XmlRsReader<R> {
  pub fn new(inner: ::xml::reader::EventReader<R>) -> Self {
    Self { inner }
  }

  pub fn from_reader(reader: R) -> Self {
    let config = ::xml::reader::ParserConfig::new()
      .trim_whitespace(true)
      .whitespace_to_characters(true)
      .cdata_to_characters(true)
      .ignore_comments(true)
      .coalesce_characters(true);

    Self::new(::xml::reader::EventReader::new_with_config(reader, config))
  }
}

impl<R: Read> XmlEventReader for XmlRsReader<R> {
  fn next_event(&mut self) -> Result<XmlReadEvent, String> {
    loop {
      match self.inner.next() {
        Ok(next) => match next {
          ::xml::reader::XmlEvent::StartDocument { .. }
          | ::xml::reader::XmlEvent::ProcessingInstruction { .. }
          | ::xml::reader::XmlEvent::Comment(_) => {}
          ::xml::reader::XmlEvent::StartElement {
            name,
            attributes,
            namespace,
          } => {
            return Ok(XmlReadEvent::StartElement {
              name: name.into(),
              attributes: attributes.into_iter().map(Into::into).collect(),
              namespace: namespace.into(),
            });
          }
          ::xml::reader::XmlEvent::EndElement { name } => {
            return Ok(XmlReadEvent::EndElement { name: name.into() });
          }
          ::xml::reader::XmlEvent::Characters(text)
          | ::xml::reader::XmlEvent::CData(text)
          | ::xml::reader::XmlEvent::Whitespace(text) => return Ok(XmlReadEvent::Characters(text)),
          ::xml::reader::XmlEvent::EndDocument => return Ok(XmlReadEvent::EndDocument),
        },
        Err(msg) => return Err(msg.msg().to_string()),
      }
    }
  }
}

/// Transitional `xml-rs` writer adapter.
pub struct XmlRsWriter<W: std::io::Write> {
  inner: ::xml::writer::EventWriter<W>,
}

impl<W: std::io::Write> XmlRsWriter<W> {
  pub fn new(inner: ::xml::writer::EventWriter<W>) -> Self {
    Self { inner }
  }

  pub fn into_inner(self) -> W {
    self.inner.into_inner()
  }
}

impl<W: std::io::Write> XmlEventWriter for XmlRsWriter<W> {
  fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String> {
    match event {
      XmlWriteEvent::StartElement {
        name,
        attributes,
        namespace,
      } => {
        let name = XmlName::local(name.into_owned()).to_xml_rs();
        let attributes: Vec<_> = attributes.iter().map(XmlAttribute::to_xml_rs).collect();
        let borrowed_attributes: Vec<_> = attributes.iter().map(|a| a.borrow()).collect();
        let namespace = namespace.to_xml_rs();
        self
          .inner
          .write(::xml::writer::events::XmlEvent::StartElement {
            name: name.borrow(),
            attributes: Cow::Owned(borrowed_attributes),
            namespace: Cow::Owned(namespace),
          })
          .map_err(|e| e.to_string())
      }
      XmlWriteEvent::EndElement => self
        .inner
        .write(::xml::writer::XmlEvent::end_element())
        .map_err(|e| e.to_string()),
      XmlWriteEvent::Characters(text) => self
        .inner
        .write(::xml::writer::XmlEvent::characters(&text))
        .map_err(|e| e.to_string()),
      XmlWriteEvent::CData(text) => self
        .inner
        .write(::xml::writer::events::XmlEvent::cdata(&text))
        .map_err(|e| e.to_string()),
    }
  }
}

#[cfg(feature = "quick-xml-backend")]
mod quick_backend {
  use super::*;
  use quick_xml::events::{BytesStart, Event};
  use quick_xml::name::{PrefixDeclaration, QName, ResolveResult};
  use quick_xml::reader::NsReader;
  use std::collections::VecDeque;
  use std::io::BufRead;

  enum InternalEvent {
    StartElement {
      name: XmlName,
      attributes: Vec<XmlAttribute>,
    },
    EndElement {
      name: XmlName,
    },
    Characters(String),
    EndDocument,
  }

  pub struct QuickXmlReader<R: BufRead> {
    inner: NsReader<R>,
    buf: Vec<u8>,
    cached: Option<InternalEvent>,
    materialized: Option<XmlReadEvent>,
    pending_ends: VecDeque<XmlName>,
    eof: bool,
    seen_any: bool,
  }

  impl<R: BufRead> QuickXmlReader<R> {
    pub fn from_reader(reader: R) -> Self {
      let mut inner = NsReader::from_reader(reader);
      inner.config_mut().trim_text(false);
      inner.config_mut().expand_empty_elements = false;
      inner.config_mut().check_end_names = true;
      Self {
        inner,
        buf: Vec::new(),
        cached: None,
        materialized: None,
        pending_ends: VecDeque::new(),
        eof: false,
        seen_any: false,
      }
    }

    fn namespace_map(&self) -> XmlNamespace {
      let mut namespace = XmlNamespace::empty();
      for (prefix, uri) in self.inner.resolver().bindings() {
        let prefix = match prefix {
          PrefixDeclaration::Default => String::new(),
          PrefixDeclaration::Named(prefix) => String::from_utf8_lossy(prefix).into_owned(),
        };
        namespace.force_put(prefix, String::from_utf8_lossy(uri.0).into_owned());
      }
      namespace
    }

    fn name_from_qname_with_resolve(
      name: QName<'_>,
      resolve: ResolveResult<'_>,
    ) -> Result<XmlName, String> {
      let (local, prefix) = name.decompose();
      let namespace = match resolve {
        ResolveResult::Bound(ns) => Some(String::from_utf8_lossy(ns.0).into_owned()),
        ResolveResult::Unbound => None,
        ResolveResult::Unknown(prefix) => {
          return Err(format!(
            "unknown namespace prefix {}",
            String::from_utf8_lossy(&prefix)
          ));
        }
      };
      Ok(XmlName {
        local_name: String::from_utf8_lossy(local.as_ref()).into_owned(),
        namespace,
        prefix: prefix.map(|p| String::from_utf8_lossy(p.as_ref()).into_owned()),
      })
    }

    fn attributes_from_start(
      reader: &NsReader<R>,
      start: &BytesStart<'_>,
    ) -> Result<Vec<XmlAttribute>, String> {
      let mut attributes = Vec::new();
      for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        if attr.key.as_namespace_binding().is_some() {
          continue;
        }
        let (resolve, _) = reader.resolver().resolve_attribute(attr.key);
        attributes.push(XmlAttribute {
          name: Self::name_from_qname_with_resolve(attr.key, resolve)?,
          value: attr
            .unescape_value()
            .map_err(|e| e.to_string())?
            .into_owned(),
        });
      }
      Ok(attributes)
    }

    fn map_quick_error(error: quick_xml::Error) -> String {
      let msg = error.to_string();
      if let Some(rest) = msg.strip_prefix("ill-formed document: expected `</") {
        if let Some((expected, tail)) = rest.split_once("`, but `</") {
          if let Some((found, _)) = tail.split_once(">` was found") {
            return format!(
              "Unexpected closing tag: {} != {}",
              found.trim_end_matches('>'),
              expected.trim_end_matches('>')
            );
          }
        }
      }
      msg
    }

    fn read_internal(&mut self) -> Result<InternalEvent, String> {
      loop {
        if let Some(name) = self.pending_ends.pop_front() {
          return Ok(InternalEvent::EndElement { name });
        }

        if self.eof {
          if self.seen_any {
            return Ok(InternalEvent::EndDocument);
          }
          return Err("Unexpected end of stream: no root element found".to_string());
        }

        self.buf.clear();
        let reader = &mut self.inner;
        let event = reader
          .read_event_into(&mut self.buf)
          .map_err(Self::map_quick_error)?;
        match event {
          Event::Start(start) => {
            self.seen_any = true;
            let (resolve, _) = reader.resolver().resolve_element(start.name());
            let name = Self::name_from_qname_with_resolve(start.name(), resolve)?;
            let attributes = Self::attributes_from_start(reader, &start)?;
            return Ok(InternalEvent::StartElement { name, attributes });
          }
          Event::Empty(start) => {
            self.seen_any = true;
            let (resolve, _) = reader.resolver().resolve_element(start.name());
            let name = Self::name_from_qname_with_resolve(start.name(), resolve)?;
            let attributes = Self::attributes_from_start(reader, &start)?;
            self.pending_ends.push_back(name.clone());
            return Ok(InternalEvent::StartElement { name, attributes });
          }
          Event::End(end) => {
            let (resolve, _) = reader.resolver().resolve_element(end.name());
            let name = Self::name_from_qname_with_resolve(end.name(), resolve)?;
            return Ok(InternalEvent::EndElement { name });
          }
          Event::Text(text) => {
            let decoded = text.decode().map_err(|e| e.to_string())?;
            let unescaped =
              quick_xml::escape::unescape(decoded.as_ref()).map_err(|e| e.to_string())?;
            let trimmed = unescaped.trim();
            if trimmed.is_empty() {
              continue;
            }
            return Ok(InternalEvent::Characters(trimmed.to_string()));
          }
          Event::CData(cdata) => {
            return Ok(InternalEvent::Characters(
              cdata.decode().map_err(|e| e.to_string())?.into_owned(),
            ));
          }
          Event::Eof => {
            self.eof = true;
            if self.seen_any {
              return Ok(InternalEvent::EndDocument);
            }
            return Err("Unexpected end of stream: no root element found".to_string());
          }
          Event::Decl(_)
          | Event::PI(_)
          | Event::DocType(_)
          | Event::Comment(_)
          | Event::GeneralRef(_) => {}
        }
      }
    }

    fn ensure_cached(&mut self) -> Result<(), String> {
      if self.cached.is_none() && self.materialized.is_none() {
        let event = self.read_internal()?;
        self.cached = Some(event);
      }
      Ok(())
    }

    fn materialize_cached(&mut self) -> Result<(), String> {
      if self.materialized.is_some() {
        return Ok(());
      }
      self.ensure_cached()?;
      if let Some(cached) = self.cached.take() {
        self.materialized = Some(match cached {
          InternalEvent::StartElement { name, attributes } => XmlReadEvent::StartElement {
            name,
            attributes,
            namespace: self.namespace_map(),
          },
          InternalEvent::EndElement { name } => XmlReadEvent::EndElement { name },
          InternalEvent::Characters(text) => XmlReadEvent::Characters(text),
          InternalEvent::EndDocument => XmlReadEvent::EndDocument,
        });
      }
      Ok(())
    }

    fn light_from_internal(event: &InternalEvent) -> XmlLightEvent {
      match event {
        InternalEvent::StartElement { name, .. } => XmlLightEvent::StartElement {
          local_name: name.local_name.clone(),
          namespace: name.namespace.clone(),
        },
        InternalEvent::EndElement { name } => XmlLightEvent::EndElement {
          local_name: name.local_name.clone(),
        },
        InternalEvent::Characters(text) => XmlLightEvent::Characters(text.clone()),
        InternalEvent::EndDocument => XmlLightEvent::EndDocument,
      }
    }
  }

  impl<R: BufRead> XmlEventReader for QuickXmlReader<R> {
    fn next_event(&mut self) -> Result<XmlReadEvent, String> {
      if let Some(event) = self.materialized.take() {
        return Ok(event);
      }
      self.materialize_cached()?;
      self
        .materialized
        .take()
        .ok_or_else(|| "no event available".to_string())
    }

    fn supports_light_events(&self) -> bool {
      true
    }

    fn peek_light_event(&mut self) -> Result<XmlLightEvent, String> {
      self.ensure_cached()?;
      if let Some(event) = &self.materialized {
        return Ok(XmlLightEvent::from(event));
      }
      self
        .cached
        .as_ref()
        .map(Self::light_from_internal)
        .ok_or_else(|| "unable to peek next item".to_string())
    }

    fn next_light_event(&mut self) -> Result<XmlLightEvent, String> {
      if let Some(event) = self.materialized.take() {
        return Ok(XmlLightEvent::from(&event));
      }
      self.ensure_cached()?;
      self
        .cached
        .take()
        .as_ref()
        .map(Self::light_from_internal)
        .ok_or_else(|| "no event available".to_string())
    }

    fn peek_attributes(&mut self) -> Result<Option<Vec<XmlAttribute>>, String> {
      self.ensure_cached()?;
      if let Some(XmlReadEvent::StartElement { attributes, .. }) = &self.materialized {
        return Ok(Some(attributes.clone()));
      }
      match &self.cached {
        Some(InternalEvent::StartElement { attributes, .. }) => Ok(Some(attributes.clone())),
        _ => Ok(None),
      }
    }

    fn read_inner_text(&mut self) -> Result<Option<String>, String> {
      self.ensure_cached()?;

      let start_name = if let Some(event) = self.materialized.take() {
        match event {
          XmlReadEvent::StartElement { name, .. } => name,
          other => {
            self.materialized = Some(other);
            return Err("Internal error: expected StartElement".to_string());
          }
        }
      } else {
        match self.cached.take() {
          Some(InternalEvent::StartElement { name, .. }) => name,
          Some(other) => {
            self.cached = Some(other);
            return Err("Internal error: expected StartElement".to_string());
          }
          None => return Err("no event available".to_string()),
        }
      };

      let mut text = String::new();
      loop {
        match self.read_internal()? {
          InternalEvent::Characters(chunk) => text.push_str(&chunk),
          InternalEvent::EndElement { name } if name == start_name => {
            if text.is_empty() {
              self.cached = Some(InternalEvent::EndElement { name });
              return Ok(None);
            }
            return Ok(Some(text));
          }
          InternalEvent::EndElement { name } => {
            return Err(format!(
              "End tag </{}> didn't match the start tag <{}>",
              name.local_name, start_name.local_name
            ));
          }
          other @ InternalEvent::StartElement { .. } => {
            self.cached = Some(other);
            return Err(format!(
              "Expected text content in <{}>",
              start_name.local_name
            ));
          }
          InternalEvent::EndDocument => {
            return Err(format!(
              "Unexpected end of document in <{}>",
              start_name.local_name
            ));
          }
        }
      }
    }
  }
}

#[cfg(feature = "quick-xml-backend")]
pub use quick_backend::QuickXmlReader;
