use yaserde::ser::{serialize_with_event_writer, Serializer};
use yaserde::xml::{
  LegacyXmlError, LegacyXmlEvent, XmlAttribute, XmlEventWriter, XmlNamespace, XmlWriteEvent,
};
use yaserde::YaSerialize;

// Deliberately not an io::Write: injection operates on XML events.
#[derive(Default)]
struct Recording {
  neutral: Vec<XmlWriteEvent<'static>>,
  legacy: Vec<String>,
  fail: bool,
}
impl XmlEventWriter for Recording {
  fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String> {
    if self.fail {
      return Err("neutral failure".into());
    }
    self.neutral.push(match event {
      XmlWriteEvent::StartElement {
        name,
        attributes,
        namespace,
      } => XmlWriteEvent::StartElement {
        name: name.into_owned().into(),
        attributes,
        namespace,
      },
      XmlWriteEvent::Characters(t) => XmlWriteEvent::Characters(t.into_owned().into()),
      XmlWriteEvent::CData(t) => XmlWriteEvent::CData(t.into_owned().into()),
      XmlWriteEvent::EndElement => XmlWriteEvent::EndElement,
    });
    Ok(())
  }
  fn write_legacy_event(&mut self, event: LegacyXmlEvent<'_>) -> Result<(), LegacyXmlError> {
    if self.fail {
      return Err(LegacyXmlError::DocumentStartAlreadyEmitted);
    }
    self.legacy.push(format!("{event:?}"));
    Ok(())
  }
}

#[derive(yaserde_derive::YaSerialize)]
struct Derived {
  value: u32,
}

struct Handwritten;
impl YaSerialize for Handwritten {
  fn serialize<E: XmlEventWriter>(&self, s: &mut Serializer<E>) -> Result<(), String> {
    s.write(LegacyXmlEvent::comment("handwritten"))
      .map_err(|e| e.to_string())?;
    s.write_cdata("raw")
  }
  fn serialize_attributes(
    &self,
    a: Vec<XmlAttribute>,
    n: XmlNamespace,
  ) -> Result<(Vec<XmlAttribute>, XmlNamespace), String> {
    Ok((a, n))
  }
}

#[test]
fn derived_primitives_and_handwritten_models_reach_injected_emitter() {
  let mut recorder =
    serialize_with_event_writer(&Derived { value: 42 }, Recording::default()).unwrap();
  assert!(
    matches!(&recorder.neutral[0], XmlWriteEvent::StartElement { name, .. } if name == "Derived")
  );
  assert!(recorder
    .neutral
    .contains(&XmlWriteEvent::Characters("42".into())));
  serialize_with_event_writer(&7u32, &mut recorder).unwrap();
  serialize_with_event_writer(&Handwritten, &mut recorder).unwrap();
  assert!(recorder
    .neutral
    .contains(&XmlWriteEvent::Characters("7".into())));
  assert!(recorder
    .neutral
    .contains(&XmlWriteEvent::CData("raw".into())));
  assert!(recorder.legacy[0].contains("handwritten"));
  let mut s = Serializer::new(&mut recorder);
  yaserde::primitives::serialize_primitives(&9u32, "primitive", &mut s, |n| n.to_string())
    .unwrap();
  assert!(recorder.neutral.contains(&XmlWriteEvent::Characters("9".into())));
}

#[test]
fn all_raw_variants_and_names_reach_boxed_emitter() {
  let mut s = Serializer::new(Box::new(Recording::default()));
  let name = xml::name::Name::qualified("root", "urn:test", Some("p"));
  for event in [
    LegacyXmlEvent::StartDocument {
      version: xml::common::XmlVersion::Version11,
      encoding: Some("UTF-8"),
      standalone: Some(true),
    },
    LegacyXmlEvent::start_element(name)
      .ns("p", "urn:test")
      .attr("a", "value")
      .into(),
    LegacyXmlEvent::characters("text"),
    LegacyXmlEvent::cdata("cdata"),
    LegacyXmlEvent::comment("comment"),
    LegacyXmlEvent::processing_instruction("pi", Some("data")),
    LegacyXmlEvent::end_element().name(name).into(),
  ] {
    s.write(event).unwrap();
  }
  s.write_end_element().unwrap();
  let recorder = s.into_inner();
  assert_eq!(recorder.legacy.len(), 7);
  assert!(recorder.legacy[1].contains("urn:test"));
  assert!(recorder.legacy[6].contains("urn:test"));
  assert_eq!(recorder.neutral, vec![XmlWriteEvent::EndElement]);
  let mut erased: Box<dyn XmlEventWriter> = recorder;
  erased.write_event(XmlWriteEvent::EndElement).unwrap();
  erased
    .write_legacy_event(LegacyXmlEvent::comment("boxed"))
    .unwrap();
}

#[test]
fn borrowed_emitter_recovers_partial_events_and_preserves_error_types() {
  let mut recorder = Recording::default();
  serialize_with_event_writer(&1u32, &mut recorder).unwrap();
  recorder.fail = true;
  assert_eq!(
    serialize_with_event_writer(&2u32, &mut recorder).err(),
    Some("neutral failure".into())
  );
  let mut s = Serializer::new(&mut recorder);
  assert!(matches!(
    s.write(LegacyXmlEvent::comment("fail")),
    Err(LegacyXmlError::DocumentStartAlreadyEmitted)
  ));
  assert_eq!(
    s.into_inner().neutral,
    vec![XmlWriteEvent::Characters("1".into())]
  );
  recorder.fail = false;
  serialize_with_event_writer(&3u32, &mut recorder).unwrap();
  assert_eq!(recorder.neutral.len(), 2);
}

#[test]
fn constructors_select_concrete_default_emitter() {
  use yaserde::ser::Config;
  use yaserde::xml::{DefaultXmlWriter, XmlRsWriter};
  let selected: DefaultXmlWriter<Vec<u8>> =
    Serializer::new_from_writer(Vec::new(), &Config::default()).into_inner();
  let inner: DefaultXmlWriter<Vec<u8>> = Serializer::new_for_inner(Vec::new()).into_inner();
  #[cfg(feature = "quick-xml-backend")]
  let _: (
    yaserde::xml::QuickXmlWriter<_>,
    yaserde::xml::QuickXmlWriter<_>,
  ) = (selected, inner);
  #[cfg(not(feature = "quick-xml-backend"))]
  let _: (XmlRsWriter<_>, XmlRsWriter<_>) = (selected, inner);
  let _: XmlRsWriter<_> =
    Serializer::new(XmlRsWriter::new(xml::EventWriter::new(Vec::new()))).into_inner();
}
