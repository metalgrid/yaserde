use std::{io::Read, io::Write};

use crate::{de, ser};

pub fn serialize_primitives<S, W: Write>(
  self_bypass: &S,
  default_name: &str,
  writer: &mut ser::Serializer<W>,
  serialize_function: impl FnOnce(&S) -> String,
) -> Result<(), String> {
  let name = writer
    .get_start_event_name()
    .unwrap_or_else(|| default_name.to_string());

  if !writer.skip_start_end() {
    writer.write_start_element(
      &name,
      ::std::iter::empty::<(::std::string::String, ::std::string::String)>(),
      ::std::iter::empty::<(::std::string::String, ::std::string::String)>(),
    )?;
  }

  let content = serialize_function(self_bypass);
  writer.write_text(&content)?;

  if !writer.skip_start_end() {
    writer.write_end_element(&name)?;
  }

  Ok(())
}

pub fn deserialize_primitives<S, R: Read>(
  reader: &mut de::Deserializer<R>,
  deserialize_function: impl FnOnce(&str) -> Result<S, String>,
) -> Result<S, String> {
  if let Ok(xml::reader::XmlEvent::StartElement { .. }) = reader.peek() {
    reader.next_event()?;
  } else {
    return Err("Start element not found".to_string());
  }

  if let Ok(xml::reader::XmlEvent::Characters(ref text)) = reader.peek() {
    deserialize_function(text)
  } else {
    deserialize_function("")
  }
}
