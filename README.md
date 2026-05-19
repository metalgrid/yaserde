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
  fn serialize<W: Write>(&self, writer: &mut yaserde::ser::Serializer<W>) -> Result<(), String> {
    // serializer code
  }
}
```

## XML parser backends

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

For runtime selection, pass a `Box<dyn yaserde::xml::XmlEventReader>` to `yaserde::de::from_reader_dyn`.

Migration note: custom deserializers should use `yaserde::xml::XmlReadEvent` instead of matching `xml::reader::XmlEvent` directly.

See [Implementing a Custom XML Backend](docs/custom-backend.md) for details on plugging in your own parser.
