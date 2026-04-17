#[macro_use]
extern crate yaserde_derive;

use std::io::Cursor;

use yaserde::{YaDeserialize, YaSerialize};

#[derive(Debug, PartialEq, YaDeserialize, YaSerialize)]
#[yaserde(rename = "item")]
struct Item {
  value: String,
}

#[test]
fn deserializer_new_accepts_event_reader() {
  let xml = "<item><value>hello</value></item>";
  let reader = yaserde::__xml::reader::EventReader::new(xml.as_bytes());
  let mut deserializer = yaserde::de::Deserializer::new(reader);

  let item = Item::deserialize(&mut deserializer).unwrap();

  assert_eq!(
    item,
    Item {
      value: "hello".into()
    }
  );
}

#[test]
fn serializer_new_accepts_event_writer() {
  let item = Item {
    value: "hello".into(),
  };
  let buffer = Cursor::new(Vec::new());
  let writer = yaserde::__xml::writer::EventWriter::new(buffer);
  let mut serializer = yaserde::ser::Serializer::new(writer);

  item.serialize(&mut serializer).unwrap();

  let buffer = serializer.into_inner();
  let xml = String::from_utf8(buffer.into_inner()).unwrap();
  assert_eq!(
    xml,
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><item><value>hello</value></item>"
  );
}

#[test]
fn to_string_with_config_preserves_exact_indent_string() {
  let item = Item {
    value: "hello".into(),
  };
  let config = yaserde::ser::Config {
    perform_indent: true,
    write_document_declaration: true,
    indent_string: Some("->".into()),
  };

  let xml = yaserde::ser::to_string_with_config(&item, &config).unwrap();

  assert_eq!(
    xml,
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<item>\n-><value>hello</value>\n</item>"
  );
}
