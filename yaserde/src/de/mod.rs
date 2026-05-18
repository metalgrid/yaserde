//! Generic data structure deserialization framework.
//!

use crate::xml::{XmlEventReader, XmlName, XmlReadEvent};
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

  pub fn inner_next(&mut self) -> Result<XmlReadEvent, String> {
    self.reader.next_event()
  }

  pub fn next_event(&mut self) -> Result<XmlReadEvent, String> {
    let next_event = if let Some(peeked) = self.peeked.take() {
      peeked
    } else {
      self.inner_next()?
    };
    match next_event {
      XmlReadEvent::StartElement { .. } => {
        self.depth += 1;
      }
      XmlReadEvent::EndElement { .. } => {
        self.depth -= 1;
      }
      _ => {}
    }
    log::debug!("Fetched {:?}, new depth {}", next_event, self.depth);
    Ok(next_event)
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
}
