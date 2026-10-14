use std::borrow::Cow;
use std::io::{self, Write};
use xml::common::XmlVersion;
use xml::name::Name;
use xml::writer::{EmitterConfig, Error, EventWriter, XmlEvent};
use yaserde::ser::{Config, Serializer};
use yaserde::xml::{XmlAttribute, XmlName, XmlNamespace, XmlWriteEvent};

fn legacy(config: &Config) -> Serializer<Vec<u8>> {
  Serializer::new(EventWriter::new_with_config(
    Vec::new(),
    EmitterConfig::new()
      .perform_indent(config.perform_indent)
      .write_document_declaration(config.write_document_declaration)
      .indent_string(config.indent_string.clone().unwrap_or_else(|| "  ".into())),
  ))
}

fn compare(config: &Config, emit: impl Fn(&mut Serializer<Vec<u8>>)) -> String {
  let mut explicit = legacy(config);
  let mut selected = Serializer::new_from_writer(Vec::new(), config);
  emit(&mut explicit);
  emit(&mut selected);
  let expected = explicit.into_inner();
  let actual = selected.into_inner();
  assert_eq!(
    String::from_utf8_lossy(&actual),
    String::from_utf8_lossy(&expected)
  );
  String::from_utf8(actual).unwrap()
}

fn namespace(bindings: &[(&str, &str)]) -> XmlNamespace {
  XmlNamespace(
    bindings
      .iter()
      .map(|(p, u)| (p.to_string(), u.to_string()))
      .collect(),
  )
}

fn start<W: Write>(s: &mut Serializer<W>, name: &str) {
  s.write_start_element(name, Vec::new(), XmlNamespace::empty())
    .unwrap();
}

#[test]
fn every_legacy_variant_interleaved_with_neutral_events() {
  let output = compare(&Config::default(), |s| {
    s.write(XmlEvent::StartDocument {
      version: XmlVersion::Version11,
      encoding: Some("UTF-8"),
      standalone: Some(true),
    })
    .unwrap();
    s.write(XmlEvent::processing_instruction("before", Some("a='b'")))
      .unwrap();
    s.write(
      XmlEvent::start_element("p:root")
        .ns("p", "urn:test")
        .attr("id", "<&\"'\n\r"),
    )
    .unwrap();
    s.write_event(XmlWriteEvent::StartElement {
      name: Cow::Borrowed("p:child"),
      attributes: vec![XmlAttribute::new(
        XmlName::qualified("a", "urn:test", Some("p")),
        "&amp;",
      )],
      namespace: namespace(&[("p", "urn:test")]),
    })
    .unwrap();
    s.write_event(XmlWriteEvent::Characters(Cow::Borrowed(
      " '雪' <&> \" &amp; ",
    )))
    .unwrap();
    s.write(XmlEvent::cdata("a]]>b]]>c")).unwrap();
    s.write_cdata("").unwrap();
    s.write_event(XmlWriteEvent::CData(Cow::Borrowed("<&>")))
      .unwrap();
    s.write(XmlEvent::comment("note -")).unwrap();
    s.write(XmlEvent::processing_instruction("inside", None))
      .unwrap();
    s.write_event(XmlWriteEvent::EndElement).unwrap();
    s.write(XmlEvent::end_element().name("p:root")).unwrap();
  });
  let mut text = String::new();
  for event in xml::reader::EventReader::from_str(&output) {
    match event.unwrap() {
      xml::reader::XmlEvent::Characters(t) | xml::reader::XmlEvent::CData(t) => text.push_str(&t),
      xml::reader::XmlEvent::StartElement {
        name, attributes, ..
      } if name.local_name == "child" => {
        assert_eq!(attributes[0].value, "&amp;");
        assert_eq!(attributes[0].name.namespace.as_deref(), Some("urn:test"));
      }
      _ => {}
    }
  }
  assert_eq!(text, " '雪' <&> \" &amp; a]]>b]]>c<&>");
}

#[test]
fn empty_text_is_not_an_empty_element() {
  let output = compare(
    &Config {
      write_document_declaration: false,
      ..Config::default()
    },
    |s| {
      start(s, "empty");
      s.write_end_element().unwrap();
      start(s, "text");
      s.write_characters("").unwrap();
      s.write_end_element().unwrap();
      start(s, "legacy");
      s.write(XmlEvent::characters("")).unwrap();
      s.write_end_element().unwrap();
      start(s, "cdata");
      s.write_cdata("").unwrap();
      s.write_end_element().unwrap();
    },
  );
  assert_eq!(
    output,
    "<empty /><text></text><legacy></legacy><cdata><![CDATA[]]></cdata>"
  );
}

#[test]
fn mixed_content_and_arbitrary_indent_strings() {
  for indent in [None, Some(""), Some("\t"), Some("→雪 ")] {
    for perform_indent in [false, true] {
      compare(
        &Config {
          perform_indent,
          indent_string: indent.map(Into::into),
          ..Config::default()
        },
        |s| {
          start(s, "root");
          start(s, "nested");
          start(s, "empty");
          s.write_end_element().unwrap();
          s.write_end_element().unwrap();
          s.write_characters("before").unwrap();
          start(s, "inline");
          s.write_characters("").unwrap();
          s.write_end_element().unwrap();
          s.write_characters("after").unwrap();
          s.write(XmlEvent::comment(" comment ")).unwrap();
          s.write(XmlEvent::processing_instruction("pi", Some("")))
            .unwrap();
          s.write_end_element().unwrap();
        },
      );
    }
  }
}

#[test]
fn namespace_dedup_shadow_and_sibling_restoration() {
  let output = compare(&Config::default(), |s| {
    let outer = namespace(&[
      ("p", "urn:outer"),
      ("", "urn:default"),
      ("xml", "http://www.w3.org/XML/1998/namespace"),
    ]);
    s.write_start_element("p:root", Vec::new(), outer.clone())
      .unwrap();
    s.write_start_element("p:same", Vec::new(), outer.clone())
      .unwrap();
    s.write_end_element().unwrap();
    s.write_start_element("p:shadow", Vec::new(), namespace(&[("p", "urn:inner")]))
      .unwrap();
    s.write_start_element("p:again", Vec::new(), namespace(&[("p", "urn:inner")]))
      .unwrap();
    s.write_end_element().unwrap();
    s.write_end_element().unwrap();
    s.write_start_element("p:sibling", Vec::new(), outer)
      .unwrap();
    s.write_end_element().unwrap();
    s.write_end_element().unwrap();
    s.write_start_element("p:fragment", Vec::new(), namespace(&[("p", "urn:outer")]))
      .unwrap();
    s.write_end_element().unwrap();
  });
  assert_eq!(output.matches("xmlns:p=\"urn:outer\"").count(), 2);
  assert_eq!(output.matches("xmlns:p=\"urn:inner\"").count(), 1);
  assert_eq!(output.matches("xmlns=\"urn:default\"").count(), 1);
}

#[test]
fn declaration_timing_and_fragments() {
  for declaration in [false, true] {
    let config = Config {
      write_document_declaration: declaration,
      perform_indent: true,
      ..Config::default()
    };
    for first in [
      XmlEvent::comment("leading"),
      XmlEvent::cdata("leading"),
      XmlEvent::characters(""),
      XmlEvent::processing_instruction("pi", None),
    ] {
      compare(&config, |s| {
        s.write(first.clone()).unwrap();
        start(s, "one");
        s.write_end_element().unwrap();
        start(s, "two");
        s.write_end_element().unwrap();
      });
    }
  }
  let mut inner = Serializer::new_for_inner(Vec::new());
  inner.write_characters("before").unwrap();
  start(&mut inner, "child");
  inner.write_end_element().unwrap();
  inner.write_cdata("after").unwrap();
  assert_eq!(inner.into_inner(), b"before<child /><![CDATA[after]]>");
}

#[test]
fn declaration_defaults_and_duplicate_errors() {
  compare(&Config::default(), |s| {
    s.write(XmlEvent::StartDocument {
      version: XmlVersion::Version10,
      encoding: None,
      standalone: Some(false),
    })
    .unwrap();
    assert!(matches!(
      s.write(XmlEvent::StartDocument {
        version: XmlVersion::Version10,
        encoding: None,
        standalone: None
      }),
      Err(Error::DocumentStartAlreadyEmitted)
    ));
    start(s, "root");
    s.write_end_element().unwrap();
  });
}

#[test]
fn names_preserve_literal_vs_split_identity() {
  compare(&Config::default(), |s| {
    start(s, "p:literal");
    s.write(XmlEvent::EndElement {
      name: Some(Name::local("p:literal")),
    })
    .unwrap();
    s.write(XmlEvent::start_element(Name::qualified("split", "urn:p", Some("p"))).ns("p", "urn:p"))
      .unwrap();
    s.write(XmlEvent::EndElement {
      name: Some(Name::qualified("split", "urn:p", Some("p"))),
    })
    .unwrap();
  });
  for explicit in [false, true] {
    let mut s = if explicit {
      legacy(&Config::default())
    } else {
      Serializer::new_from_writer(Vec::new(), &Config::default())
    };
    assert!(matches!(
      s.write(XmlEvent::end_element()),
      Err(Error::LastElementNameNotAvailable)
    ));
    start(&mut s, "p:literal");
    assert!(matches!(
      s.write(XmlEvent::end_element().name("p:literal")),
      Err(Error::EndElementNameIsNotEqualToLastStartElementName)
    ));
  }
}

#[test]
fn explicit_legacy_writer_preserves_configuration_and_existing_state() {
  fn prepared() -> EventWriter<Vec<u8>> {
    let mut writer = EmitterConfig::new()
      .write_document_declaration(false)
      .perform_indent(true)
      .indent_string("_")
      .line_separator("\r\n")
      .normalize_empty_elements(false)
      .autopad_comments(false)
      .cdata_to_characters(true)
      .create_writer(Vec::new());
    writer
      .write(XmlEvent::start_element("p:root").ns("p", "urn:p"))
      .unwrap();
    writer
  }
  let mut expected = prepared();
  expected
    .write(XmlEvent::start_element("p:child").ns("p", "urn:p"))
    .unwrap();
  expected.write(XmlEvent::cdata("<&>")).unwrap();
  expected.write(XmlEvent::end_element()).unwrap();
  expected.write(XmlEvent::comment("hi")).unwrap();
  expected.write(XmlEvent::end_element()).unwrap();
  let mut actual = Serializer::new(prepared());
  actual
    .write_start_element("p:child", Vec::new(), namespace(&[("p", "urn:p")]))
    .unwrap();
  actual.write_cdata("<&>").unwrap();
  actual.write_end_element().unwrap();
  actual.write(XmlEvent::comment("hi")).unwrap();
  actual.write_end_element().unwrap();
  assert_eq!(actual.into_inner(), expected.into_inner());
}

struct LimitedSink {
  remaining: usize,
  bytes: Vec<u8>,
}
impl Write for LimitedSink {
  fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
    if self.remaining == 0 {
      return Err(io::Error::new(io::ErrorKind::BrokenPipe, "test sink"));
    }
    let n = bytes.len().min(self.remaining);
    self.bytes.extend_from_slice(&bytes[..n]);
    self.remaining -= n;
    Ok(n)
  }
  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

#[test]
fn every_output_byte_is_written_by_a_fallible_event_not_into_inner() {
  let events = vec![
    XmlEvent::start_element("root").attr("a", "<&>").into(),
    XmlEvent::characters("<&>"),
    XmlEvent::cdata("a]]>b"),
    XmlEvent::comment("hi"),
    XmlEvent::processing_instruction("pi", Some("data")),
    XmlEvent::start_element("empty").into(),
    XmlEvent::end_element().into(),
    XmlEvent::end_element().into(),
  ];
  let mut good = Serializer::new_from_writer(Vec::new(), &Config::default());
  for event in &events {
    good.write(event.clone()).unwrap();
  }
  let expected = good.into_inner();
  for remaining in 0..expected.len() {
    let mut s = Serializer::new_from_writer(
      LimitedSink {
        remaining,
        bytes: Vec::new(),
      },
      &Config::default(),
    );
    let error = events
      .iter()
      .find_map(|event| s.write(event.clone()).err())
      .expect("write must fail");
    assert!(matches!(error, Error::Io(ref e) if e.kind() == io::ErrorKind::BrokenPipe));
    assert_eq!(s.into_inner().bytes, expected[..remaining]);
  }
  let mut s = Serializer::new_from_writer(
    LimitedSink {
      remaining: 0,
      bytes: Vec::new(),
    },
    &Config::default(),
  );
  assert!(s
    .write_start_element("root", Vec::new(), XmlNamespace::empty())
    .unwrap_err()
    .contains("test sink"));
}

#[test]
fn unfinished_start_is_not_silently_buffered_or_flushed() {
  let config = Config {
    write_document_declaration: false,
    ..Config::default()
  };
  let output = compare(&config, |s| start(s, "unfinished"));
  assert_eq!(output, "<unfinished");
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_preserves_attribute_whitespace_and_default_namespace_unbinding() {
  let mut s = Serializer::new_from_writer(Vec::new(), &Config::default());
  let value = "雪\t\n\r<&>\"'&amp;";
  s.write_start_element(
    "root",
    vec![XmlAttribute::new(XmlName::local("a"), value)],
    namespace(&[("", "urn:outer"), ("p", "urn:p?x=1&y=2")]),
  )
  .unwrap();
  s.write_start_element("unbound", Vec::new(), namespace(&[("", "")]))
    .unwrap();
  start(&mut s, "still_unbound");
  s.write_end_element().unwrap();
  s.write_end_element().unwrap();
  start(&mut s, "restored");
  s.write_end_element().unwrap();
  s.write_end_element().unwrap();
  let output = String::from_utf8(s.into_inner()).unwrap();
  assert!(output.contains("&#x9;&#xA;&#xD;"));
  assert_eq!(output.matches("xmlns=\"\"").count(), 1);
  for event in xml::reader::EventReader::from_str(&output) {
    if let xml::reader::XmlEvent::StartElement {
      name,
      attributes,
      namespace,
    } = event.unwrap()
    {
      match name.local_name.as_str() {
        "root" => {
          assert_eq!(attributes[0].value, value);
          assert_eq!(namespace.get("p"), Some("urn:p?x=1&y=2"));
        }
        "unbound" | "still_unbound" => assert_eq!(name.namespace, None),
        "restored" => assert_eq!(name.namespace.as_deref(), Some("urn:outer")),
        _ => unreachable!(),
      }
    }
  }
}
