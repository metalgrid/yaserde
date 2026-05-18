#[macro_use]
extern crate yaserde_derive;

use yaserde::de::{from_reader_dyn, from_reader_with_parser};
use yaserde::xml::XmlRsReader;

#[derive(Debug, PartialEq, YaDeserialize)]
struct Root {
  item: String,
}

const XML: &str = "<Root><item>a<![CDATA[b]]>c</item></Root>";

#[test]
fn xml_rs_backend_can_be_selected_explicitly() {
  let parser = XmlRsReader::from_reader(XML.as_bytes());
  let loaded: Root = from_reader_with_parser(parser).unwrap();
  assert_eq!(loaded, Root { item: "abc".into() });
}

#[test]
fn xml_rs_backend_can_be_selected_dynamically() {
  let parser = Box::new(XmlRsReader::from_reader(XML.as_bytes()));
  let loaded: Root = from_reader_dyn(parser).unwrap();
  assert_eq!(loaded, Root { item: "abc".into() });
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_backend_can_be_selected_explicitly() {
  let parser = yaserde::xml::QuickXmlReader::from_reader(std::io::Cursor::new(XML));
  let loaded: Root = from_reader_with_parser(parser).unwrap();
  assert_eq!(loaded, Root { item: "abc".into() });
}

#[derive(Debug, PartialEq, YaDeserialize)]
#[yaserde(
  rename = "Envelope",
  namespaces = {
    "s" = "urn:test",
  },
  prefix = "s"
)]
struct NamespacedEnvelope {
  #[yaserde(attribute = true)]
  id: String,
  #[yaserde(rename = "child", prefix = "s")]
  child: String,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct FlattenOuter {
  known: String,
  #[yaserde(flatten = true)]
  extra: FlattenExtra,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct FlattenExtra {
  other: String,
}

#[derive(Debug, PartialEq, YaDeserialize)]
enum Choice {
  A,
  B,
}

impl Default for Choice {
  fn default() -> Self {
    Choice::A
  }
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct ChoiceHolder {
  choice: Choice,
}

fn assert_xml_rs_parity_cases() {
  let spaced = "<Root><!-- skip --><item> a <![CDATA[b]]> c </item></Root>";
  let loaded: Root = from_reader_with_parser(XmlRsReader::from_reader(spaced.as_bytes())).unwrap();
  assert_eq!(
    loaded,
    Root {
      item: "a b c".into()
    }
  );

  let namespaced = r#"<s:Envelope xmlns:s="urn:test" id="1"><s:child>value</s:child></s:Envelope>"#;
  let loaded: NamespacedEnvelope =
    from_reader_with_parser(XmlRsReader::from_reader(namespaced.as_bytes())).unwrap();
  assert_eq!(
    loaded,
    NamespacedEnvelope {
      id: "1".into(),
      child: "value".into(),
    }
  );

  let flattened = "<FlattenOuter><known>k</known><other>o</other></FlattenOuter>";
  let loaded: FlattenOuter =
    from_reader_with_parser(XmlRsReader::from_reader(flattened.as_bytes())).unwrap();
  assert_eq!(
    loaded,
    FlattenOuter {
      known: "k".into(),
      extra: FlattenExtra { other: "o".into() },
    }
  );

  let choice = "<ChoiceHolder><choice>B</choice></ChoiceHolder>";
  let loaded: ChoiceHolder =
    from_reader_with_parser(XmlRsReader::from_reader(choice.as_bytes())).unwrap();
  assert_eq!(loaded, ChoiceHolder { choice: Choice::B });
}

#[test]
fn xml_rs_backend_parity_cases() {
  assert_xml_rs_parity_cases();
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_backend_parity_cases() {
  use yaserde::xml::QuickXmlReader;

  let spaced = "<Root><!-- skip --><item> a <![CDATA[b]]> c </item></Root>";
  let loaded: Root =
    from_reader_with_parser(QuickXmlReader::from_reader(std::io::Cursor::new(spaced))).unwrap();
  assert_eq!(
    loaded,
    Root {
      item: "a b c".into()
    }
  );

  let namespaced = r#"<s:Envelope xmlns:s="urn:test" id="1"><s:child>value</s:child></s:Envelope>"#;
  let loaded: NamespacedEnvelope = from_reader_with_parser(QuickXmlReader::from_reader(
    std::io::Cursor::new(namespaced),
  ))
  .unwrap();
  assert_eq!(
    loaded,
    NamespacedEnvelope {
      id: "1".into(),
      child: "value".into(),
    }
  );

  let flattened = "<FlattenOuter><known>k</known><other>o</other></FlattenOuter>";
  let loaded: FlattenOuter =
    from_reader_with_parser(QuickXmlReader::from_reader(std::io::Cursor::new(flattened))).unwrap();
  assert_eq!(
    loaded,
    FlattenOuter {
      known: "k".into(),
      extra: FlattenExtra { other: "o".into() },
    }
  );

  let choice = "<ChoiceHolder><choice>B</choice></ChoiceHolder>";
  let loaded: ChoiceHolder =
    from_reader_with_parser(QuickXmlReader::from_reader(std::io::Cursor::new(choice))).unwrap();
  assert_eq!(loaded, ChoiceHolder { choice: Choice::B });
}
