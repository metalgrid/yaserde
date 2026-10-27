# yaserde &emsp; [![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/yaserde.svg
[crates.io]: https://crates.io/crates/yaserde

**Yet Another Serializer/Deserializer specialized for XML**

## Goal
This library will support XML de/ser-ializing with all specific features.

## Supported types

- [x] Struct
- [x] Vec<AnyType>
- [x] Enum
- [x] Enum with complex types
- [x] Option
- [x] String
- [x] bool
- [x] number (u8, i8, u32, i32, f32, f64)

## Attributes

- [x] **attribute**: this field is defined as an attribute
- [x] **default**: defines the default function to init the field
- [x] **flatten**: Flatten the contents of the field
- [x] **namespace**: defines the namespace of the field
- [x] **rename**: be able to rename a field
- [x] **root**: rename the based element. Used only at the XML root.
- [x] **skip_serializing**: Exclude this field from the serialized output. [More details...](docs/skip_serializing.md)
- [x] **skip_serializing_if**: Skip the serialisation for this field if the condition is true.  [More details...](docs/skip_serializing.md)
- [x] **text**: this field match to the text content

## Custom De/Ser-rializer

Any type can define a custom deserializer and/or serializer.
To implement it, define the implementation of YaDeserialize/YaSerialize

```rust
impl YaDeserialize for MyType {
  fn deserialize<P: yaserde::xml::XmlEventReader>(
    reader: &mut yaserde::de::Deserializer<P>,
  ) -> Result<Self, String> {
    // match on yaserde::xml::XmlReadEvent values from reader.peek()/next_event()
    // deserializer code
  }
}
```

```rust

impl YaSerialize for MyType {
  fn serialize<E: yaserde::xml::XmlEventWriter>(&self, writer: &mut yaserde::ser::Serializer<E>) -> Result<(), String> {
    // serializer code
  }
}
```

## XML backends

YaSerDe's deserializer is generic over `yaserde::xml::XmlEventReader`, so parser backends can be selected explicitly:

```rust
let parser = yaserde::xml::XmlRsReader::from_reader(xml.as_bytes());
let value: MyType = yaserde::de::from_reader_with_parser(parser)?;
```

Enable the faster quick-xml backend with:

```toml
yaserde = { version = "...", features = ["quick-xml-backend"] }
```

When `quick-xml-backend` is enabled, `yaserde::de::from_str` and `from_reader` use quick-xml by default. Generated deserializers automatically use parser-neutral light-event and simple-text fast paths when available, avoiding full XML event materialization for common leaf fields while keeping the backend swappable.

The quick-xml reader coalesces adjacent ordinary text, XML references and CDATA
before deserialization, including text split by ignored comments or processing
instructions. References decode exactly once; entity spelling inside CDATA stays
literal. Spaces between fragments are retained, outer ordinary-text whitespace is
trimmed, and CDATA/reference whitespace is preserved. Coalescing stops at element
boundaries. Unknown or malformed references return a parse error.

The same feature selects native quick-xml serialization for `to_string`, `to_string_with_config`, writer-based helpers, and content-only serialization. It takes precedence even when the default `xml-rs-backend` feature is also enabled (including all-features builds). Without `quick-xml-backend`, serialization and deserialization continue to use xml-rs.

**Low-level breaking change:** `Serializer<E: XmlEventWriter>` now owns an event emitter,
not a byte sink. Handwritten `YaSerialize::serialize` implementations must replace the
`W: Write` bound with `E: yaserde::xml::XmlEventWriter` (and regenerate derives together).
`Serializer::new(XmlRsWriter::new(xml_writer))` retains an explicit xml-rs writer's
configuration and existing state. `serializer.into_inner()` now returns the emitter;
call its `into_inner()` again to extract the byte sink. High-level string, config,
byte-writer and content-only helper signatures remain unchanged.

Inject custom emitters or policy wrappers without implementing `std::io::Write`:

```rust,ignore
let emitter = yaserde::xml::DefaultXmlWriter::from_writer(
    Vec::new(), &yaserde::ser::Config::default(),
);
let emitter = yaserde::ser::serialize_with_event_writer(&model, emitter)?;
let bytes = emitter.into_inner();
```

`DefaultXmlWriter<W>` is a compile-time alias for `QuickXmlWriter<W>` when enabled,
otherwise `XmlRsWriter<W>`. Both are also available explicitly. Implement both
`XmlEventWriter::write_event` and `write_legacy_event`; neutral helpers and raw legacy
events always pass through the injected emitter. Legacy events/errors are re-exported
as `yaserde::xml::{LegacyXmlEvent, LegacyXmlError}`. `Box<T>` and `&mut T` forward both
methods; passing `&mut emitter` preserves access to partial output after errors.

The quick writer preserves YaSerDe's formatting, empty-element, namespace-scope and escaping conventions. It additionally emits attribute tabs as character references and writes `xmlns=""` when unbinding an inherited default namespace, avoiding content/namespace loss in xml-rs 0.8. The xml-rs dependency remains for the compatibility API and deserialization's flatten capture. Namespace policy belongs in application-owned event-writer wrappers, not in the library.

For runtime selection, pass a `Box<dyn yaserde::xml::XmlEventReader>` to `yaserde::de::from_reader_dyn`.

Migration note: custom deserializers should use `yaserde::xml::XmlReadEvent` instead of matching `xml::reader::XmlEvent` directly.

See [Implementing a Custom XML Backend](docs/custom-backend.md) for the reader/writer contract and low-level API migration.
