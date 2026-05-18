//! Generic data structure deserialization framework.
//!

use crate::xml::{XmlAttribute, XmlEventReader, XmlLightEvent, XmlName, XmlReadEvent};
use crate::YaDeserialize;
use std::io::Read;

pub fn from_str<T: YaDeserialize>(s: &str) -> Result<T, String> {
  from_reader(s.as_bytes())
}

#[cfg(feature = "quick-xml-backend")]
pub fn from_reader<R: Read, T: YaDeserialize>(reader: R) -> Result<T, String> {
  from_reader_with_parser(crate::xml::QuickXmlReader::from_reader(
    std::io::BufReader::new(reader),
  ))
}

#[cfg(not(feature = "quick-xml-backend"))]
pub fn from_reader<R: Read, T: YaDeserialize>(reader: R) -> Result<T, String> {
  from_reader_with_parser(crate::xml::XmlRsReader::from_reader(reader))
}

pub fn from_reader_with_parser<P, T>(parser: P) -> Result<T, String>
where
  P: XmlEventReader,
  T: YaDeserialize,
{
  <T as YaDeserialize>::deserialize(&mut Deserializer::new(parser))
}

pub fn from_reader_dyn<T>(parser: Box<dyn XmlEventReader>) -> Result<T, String>
where
  T: YaDeserialize,
{
  from_reader_with_parser(parser)
}

pub struct Deserializer<P: XmlEventReader> {
  depth: usize,
  reader: P,
  peeked: Option<XmlReadEvent>,
}

impl<P: XmlEventReader> Deserializer<P> {
  pub fn new(reader: P) -> Self {
    Deserializer {
      depth: 0,
      reader,
      peeked: None,
    }
  }

  pub fn peek(&mut self) -> Result<&XmlReadEvent, String> {
    if self.peeked.is_none() {
      self.peeked = Some(self.inner_next()?);
    }

    if let Some(ref next) = self.peeked {
      Ok(next)
    } else {
      Err("unable to peek next item".into())
    }
  }

  pub fn peek_light(&mut self) -> Result<XmlLightEvent, String> {
    if let Some(ref peeked) = self.peeked {
      return Ok(XmlLightEvent::from(peeked));
    }

    if self.reader.supports_light_events() {
      self.reader.peek_light_event()
    } else {
      Ok(XmlLightEvent::from(self.peek()?))
    }
  }

  pub fn inner_next(&mut self) -> Result<XmlReadEvent, String> {
    self.reader.next_event()
  }

  pub fn next_event(&mut self) -> Result<XmlReadEvent, String> {
    let next_event = if let Some(peeked) = self.peeked.take() {
      peeked
    } else {
      self.inner_next()?
    };
    self.update_depth_for_read_event(&next_event);
    log::debug!("Fetched {:?}, new depth {}", next_event, self.depth);
    Ok(next_event)
  }

  pub fn next_light_event(&mut self) -> Result<XmlLightEvent, String> {
    let next_event = if let Some(peeked) = self.peeked.take() {
      XmlLightEvent::from(&peeked)
    } else if self.reader.supports_light_events() {
      self.reader.next_light_event()?
    } else {
      XmlLightEvent::from(&self.inner_next()?)
    };
    self.update_depth_for_light_event(&next_event);
    log::debug!("Fetched {:?}, new depth {}", next_event, self.depth);
    Ok(next_event)
  }

  pub fn peek_attributes(&mut self) -> Result<Option<Vec<XmlAttribute>>, String> {
    if let Some(XmlReadEvent::StartElement { attributes, .. }) = &self.peeked {
      return Ok(Some(attributes.clone()));
    }

    if self.reader.supports_light_events() {
      self.reader.peek_attributes()
    } else {
      match self.peek()? {
        XmlReadEvent::StartElement { attributes, .. } => Ok(Some(attributes.clone())),
        _ => Ok(None),
      }
    }
  }

  pub fn skip_element(&mut self, mut cb: impl FnMut(&XmlReadEvent)) -> Result<(), String> {
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
    if let Ok(XmlReadEvent::StartElement { name, .. }) = self.next_event() {
      let result = f(self)?;
      self.expect_end_element(&name)?;
      Ok(result)
    } else {
      Err("Internal error: Bad Event".to_string())
    }
  }

  pub fn read_inner_text_light(&mut self) -> Result<Option<String>, String> {
    if self.peeked.is_none() && self.reader.supports_light_events() {
      let text = self.reader.read_inner_text()?;
      if text.is_none() {
        self.depth += 1;
      }
      return Ok(text);
    }

    if let Ok(XmlReadEvent::StartElement { name, .. }) = self.next_event() {
      match self.peek()? {
        XmlReadEvent::Characters(_) => {
          let text = if let XmlReadEvent::Characters(text) = self.next_event()? {
            text
          } else {
            unreachable!()
          };
          self.expect_end_element(&name)?;
          Ok(Some(text))
        }
        XmlReadEvent::EndElement { .. } => Ok(None),
        _ => Err(format!("Expected text content in <{}>", name.local_name)),
      }
    } else {
      Err("Internal error: Bad Event".to_string())
    }
  }

  pub fn expect_end_element(&mut self, start_name: &XmlName) -> Result<(), String> {
    if let XmlReadEvent::EndElement { name, .. } = self.next_event()? {
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

  fn update_depth_for_read_event(&mut self, event: &XmlReadEvent) {
    match event {
      XmlReadEvent::StartElement { .. } => self.depth += 1,
      XmlReadEvent::EndElement { .. } => self.depth -= 1,
      _ => {}
    }
  }

  fn update_depth_for_light_event(&mut self, event: &XmlLightEvent) {
    match event {
      XmlLightEvent::StartElement { .. } => self.depth += 1,
      XmlLightEvent::EndElement { .. } => self.depth -= 1,
      _ => {}
    }
  }
}
