#[macro_use]
extern crate yaserde_derive;

use yaserde::de::from_str;

#[derive(Debug, PartialEq, YaDeserialize)]
struct Items {
  item: Vec<String>,
}

#[test]
fn large_document_deserialization_smoke() {
  let mut xml = String::from("<Items>");
  for idx in 0..512 {
    xml.push_str("<item>");
    xml.push_str(&idx.to_string());
    xml.push_str("</item>");
  }
  xml.push_str("</Items>");

  let loaded: Items = from_str(&xml).unwrap();
  assert_eq!(loaded.item.len(), 512);
  assert_eq!(loaded.item[0], "0");
  assert_eq!(loaded.item[511], "511");
}
