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
}

impl<T: XmlEventReader + ?Sized> XmlEventReader for Box<T> {
  fn next_event(&mut self) -> Result<XmlReadEvent, String> {
    self.as_mut().next_event()
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
  use std::io::BufRead;

  pub struct QuickXmlReader<R: BufRead> {
    inner: NsReader<R>,
    pending: Option<XmlReadEvent>,
    seen_any: bool,
  }

  impl<R: BufRead> QuickXmlReader<R> {
    pub fn from_reader(reader: R) -> Self {
      let mut inner = NsReader::from_reader(reader);
      inner.config_mut().trim_text(false);
      inner.config_mut().expand_empty_elements = true;
      inner.config_mut().check_end_names = true;
      Self {
        inner,
        pending: None,
        seen_any: false,
      }
    }

    fn namespace_map(&self) -> XmlNamespace {
      let mut namespace = XmlNamespace::empty();
      for (prefix, uri) in self.inner.prefixes() {
        let prefix = match prefix {
          PrefixDeclaration::Default => String::new(),
          PrefixDeclaration::Named(prefix) => String::from_utf8_lossy(prefix).into_owned(),
        };
        namespace.force_put(prefix, String::from_utf8_lossy(uri.0).into_owned());
      }
      namespace
    }

    fn name_from_qname(&self, name: QName<'_>, attribute: bool) -> Result<XmlName, String> {
      let (ns, local) = if attribute {
        self.inner.resolve_attribute(name)
      } else {
        self.inner.resolve_element(name)
      };
      let (_, prefix) = name.decompose();
      let namespace = match ns {
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

    fn attributes_from_start(&self, start: &BytesStart<'_>) -> Result<Vec<XmlAttribute>, String> {
      let mut attributes = Vec::new();
      for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        if attr.key.as_namespace_binding().is_some() {
          continue;
        }
        attributes.push(XmlAttribute {
          name: self.name_from_qname(attr.key, true)?,
          value: attr
            .unescape_value()
            .map_err(|e| e.to_string())?
            .into_owned(),
        });
      }
      Ok(attributes)
    }

    fn event_to_owned(&mut self, event: Event<'_>) -> Result<Option<XmlReadEvent>, String> {
      let converted = match event {
        Event::Start(start) => Ok(Some(XmlReadEvent::StartElement {
          name: self.name_from_qname(start.name(), false)?,
          attributes: self.attributes_from_start(&start)?,
          namespace: self.namespace_map(),
        })),
        Event::End(end) => Ok(Some(XmlReadEvent::EndElement {
          name: self.name_from_qname(end.name(), false)?,
        })),
        Event::Text(text) => Ok(Some(XmlReadEvent::Characters(
          text.unescape().map_err(|e| e.to_string())?.into_owned(),
        ))),
        Event::CData(cdata) => Ok(Some(XmlReadEvent::Characters(
          cdata.decode().map_err(|e| e.to_string())?.into_owned(),
        ))),
        Event::Eof => {
          if self.seen_any {
            Ok(Some(XmlReadEvent::EndDocument))
          } else {
            Err("Unexpected end of stream: no root element found".to_string())
          }
        }
        Event::Decl(_) | Event::PI(_) | Event::DocType(_) | Event::Comment(_) => Ok(None),
        Event::Empty(_) => unreachable!("expand_empty_elements=true should avoid Empty events"),
      }?;

      if converted.is_some() && !matches!(converted, Some(XmlReadEvent::EndDocument)) {
        self.seen_any = true;
      }

      Ok(converted)
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
  }

  impl<R: BufRead> XmlEventReader for QuickXmlReader<R> {
    fn next_event(&mut self) -> Result<XmlReadEvent, String> {
      if let Some(event) = self.pending.take() {
        return Ok(event);
      }

      let mut buf = Vec::new();
      loop {
        buf.clear();
        let event = self
          .inner
          .read_event_into(&mut buf)
          .map_err(Self::map_quick_error)?;
        if let Some(event) = self.event_to_owned(event)? {
          match event {
            XmlReadEvent::Characters(mut text) => loop {
              buf.clear();
              let next = self
                .inner
                .read_event_into(&mut buf)
                .map_err(Self::map_quick_error)?;
              match self.event_to_owned(next)? {
                Some(XmlReadEvent::Characters(more)) => text.push_str(&more),
                Some(other) => {
                  let text = text.trim().to_string();
                  if text.is_empty() {
                    return Ok(other);
                  }
                  self.pending = Some(other);
                  return Ok(XmlReadEvent::Characters(text));
                }
                None => {}
              }
            },
            other => return Ok(other),
          }
        }
      }
    }
  }
}

#[cfg(feature = "quick-xml-backend")]
pub use quick_backend::QuickXmlReader;
