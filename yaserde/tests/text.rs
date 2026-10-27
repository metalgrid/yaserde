#![cfg(feature = "quick-xml-backend")]

use yaserde::de::from_str;
use yaserde::xml::{QuickXmlReader, XmlEventReader, XmlLightEvent, XmlReadEvent};
use yaserde_derive::YaDeserialize;

#[derive(Debug, PartialEq, YaDeserialize)]
struct Value {
  #[yaserde(attribute = true, prefix = "xsi", rename = "type")]
  xsi_type: Option<String>,
  #[yaserde(text = true)]
  content: Option<String>,
}

#[test]
fn derived_text_decodes_references_once_without_losing_fragments() {
  for (input, expected) in [
    ("44 &apos;C", "44 'C"),
    ("54 &apos;C", "54 'C"),
    ("44 'C", "44 'C"),
    ("44 C", "44 C"),
    ("&amp;&lt;&gt;&quot;&apos;", "&<>\"'"),
    ("44 &#176;C", "44 °C"),
    ("54 &#xB0;C", "54 °C"),
    ("&#39;C", "'C"),
    ("44&#x27;", "44'"),
    ("&amp;apos;", "&apos;"),
    ("left &apos; right", "left ' right"),
    ("left <!-- ignored --> &apos; right", "left  ' right"),
    ("left<?ignored value?> &apos; right", "left ' right"),
    (" \n 44 &apos;C \t ", "44 'C"),
    ("left <!-- ignored --> \t ", "left"),
    ("<![CDATA[  &apos;  ]]>", "  &apos;  "),
    ("left <![CDATA[&apos;]]> right", "left &apos; right"),
    ("&#32;left&#x20;", " left "),
  ] {
    let xml = format!(
      r#"<Value xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="xsd:string">{}</Value>"#,
      input
    );
    let value: Value = from_str(&xml).unwrap();
    assert_eq!(value.content.as_deref(), Some(expected), "{}", input);
    assert_eq!(value.xsi_type.as_deref(), Some("xsd:string"));
  }
}

#[test]
fn derived_text_rejects_unknown_or_malformed_references() {
  for input in [
    "&unknown;",
    "&#x110000;",
    "&#0;",
    "&#invalid;",
    "trailing &",
  ] {
    let xml = format!("<Value>{}</Value>", input);
    assert!(from_str::<Value>(&xml).is_err(), "{}", input);
  }
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct Leaves {
  item: Vec<String>,
}

#[test]
fn simple_text_fast_path_preserves_values_and_sibling_boundaries() {
  let value: Leaves = from_str(
    "<Leaves><item>44 &apos;C</item><item/><item>left <![CDATA[&apos;]]> right</item></Leaves>",
  )
  .unwrap();
  assert_eq!(value.item, ["44 'C", "left &apos; right"]);
}

#[test]
fn text_lookahead_preserves_nested_boundaries_and_namespace_scope() {
  let mut reader = QuickXmlReader::from_reader(
    b"<root>left &amp;<child xmlns=\"urn:child\"/>right</root>".as_slice(),
  );
  assert!(
    matches!(reader.next_event().unwrap(), XmlReadEvent::StartElement { name, .. } if name.local_name == "root")
  );
  assert_eq!(
    reader.peek_light_event().unwrap(),
    XmlLightEvent::Characters("left &".into())
  );
  assert_eq!(
    reader.next_event().unwrap(),
    XmlReadEvent::Characters("left &".into())
  );
  let XmlReadEvent::StartElement {
    name, namespace, ..
  } = reader.next_event().unwrap()
  else {
    panic!("expected nested child");
  };
  assert_eq!(name.local_name, "child");
  assert_eq!(name.namespace.as_deref(), Some("urn:child"));
  assert_eq!(namespace.0.get("").map(String::as_str), Some("urn:child"));
  assert!(
    matches!(reader.next_event().unwrap(), XmlReadEvent::EndElement { name } if name.local_name == "child")
  );
  assert_eq!(
    reader.next_event().unwrap(),
    XmlReadEvent::Characters("right".into())
  );
  assert!(
    matches!(reader.next_event().unwrap(), XmlReadEvent::EndElement { name } if name.local_name == "root" && name.namespace.is_none())
  );
  assert_eq!(reader.next_event().unwrap(), XmlReadEvent::EndDocument);
}
