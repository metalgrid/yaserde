# Implementing a Custom XML Backend

YaSerDe's parser layer is built around two traits — `XmlEventReader` for deserialization and
`XmlEventWriter` for serialization — plus a set of shared value types.  Implementing these
traits lets you plug in any XML parser or emitter while keeping all derived (`#[derive(YaDeserialize)]`)
code working unchanged.

This document walks through every type and method you need to implement, with a complete
worked example at the end.

---

## Table of Contents

1. [Shared Value Types](#shared-value-types)
2. [Deserialization — `XmlEventReader`](#deserialization--xmleventreader)
   - [Required method](#required-method)
   - [Light-event fast path (optional)](#light-event-fast-path-optional)
   - [Peek / one-element look-ahead](#peek--one-element-look-ahead)
3. [Serialization — `XmlEventWriter`](#serialization--xmleventwriter)
4. [Wiring It Up](#wiring-it-up)
5. [Full Example — A Minimal Reader](#full-example--a-minimal-reader)
6. [Testing Your Backend](#testing-your-backend)

---

## Shared Value Types

All backends exchange data through the types defined in `yaserde::xml`.  Your backend must
produce (reader) or consume (writer) these types — **never** expose parser-native types
through the trait boundary.

| Type | Role |
|------|------|
| `XmlName` | Qualified XML name: `{ local_name, namespace, prefix }`. |
| `XmlAttribute` | A name + value pair: `{ name: XmlName, value: String }`. |
| `XmlNamespace` | Prefix → URI map (`BTreeMap<String, String>` wrapper). |
| `XmlReadEvent` | Full reader event (see below). |
| `XmlLightEvent` | Lightweight reader event — no attributes or namespace map (see below). |
| `XmlWriteEvent` | Writer event. |

### `XmlReadEvent` — full deserialization event

```rust
pub enum XmlReadEvent {
    StartElement {
        name: XmlName,
        attributes: Vec<XmlAttribute>,
        namespace: XmlNamespace,   // in-scope bindings at this element
    },
    EndElement {
        name: XmlName,
    },
    Characters(String),
    EndDocument,
}
```

Every reader **must** produce these.  `namespace` on `StartElement` contains all namespace
bindings that are in scope *including* those declared on the element itself.

### `XmlLightEvent` — fast-path deserialization event

```rust
pub enum XmlLightEvent {
    StartElement {
        local_name: String,
        namespace: Option<String>,
    },
    EndElement {
        local_name: String,
    },
    Characters(String),
    EndDocument,
}
```

The derive codegen uses light events for non-flatten structs and enums to avoid allocating
attribute vectors and namespace maps for every element.  Supporting them is optional but
recommended for performance.

### `XmlWriteEvent` — serialization event

```rust
pub enum XmlWriteEvent<'a> {
    StartElement {
        name: Cow<'a, str>,
        attributes: Vec<XmlAttribute>,
        namespace: XmlNamespace,
    },
    EndElement,
    Characters(Cow<'a, str>),
    CData(Cow<'a, str>),
}
```

---

## Deserialization — `XmlEventReader`

### Trait definition

```rust
pub trait XmlEventReader {
    // ── Required ──────────────────────────────────────────
    fn next_event(&mut self) -> Result<XmlReadEvent, String>;

    // ── Optional light-event fast paths ───────────────────
    fn supports_light_events(&self) -> bool { false }

    fn peek_light_event(&mut self) -> Result<XmlLightEvent, String> {
        Err("light events are not supported by this backend".into())
    }

    fn next_light_event(&mut self) -> Result<XmlLightEvent, String> {
        Ok(XmlLightEvent::from(&self.next_event()?))
    }

    fn peek_attributes(&mut self) -> Result<Option<Vec<XmlAttribute>>, String> {
        Err("attribute peeking is not supported by this backend".into())
    }

    fn read_inner_text(&mut self) -> Result<Option<String>, String> {
        Err("inner text fast path is not supported by this backend".into())
    }
}
```

The blanket `impl XmlEventReader for Box<T>` is already provided, so boxed trait objects
work automatically.

### Required method

#### `fn next_event(&mut self) -> Result<XmlReadEvent, String>`

Advance the parser and return the next event.  You **must**:

- Skip over processing instructions, comments, doctype declarations, and the XML
  declaration — they should never be surfaced.
- Coalesce `Text` and `CData` into `Characters` variants.
- Return `EndDocument` when the stream is exhausted.
- Return a human-readable `Err(String)` on malformed input.

### Light-event fast path (optional)

When `supports_light_events()` returns `true`, the generated deserializer calls the
light-event methods instead of `next_event()` for non-flatten types.  This lets your
backend skip building the `Vec<XmlAttribute>` and `XmlNamespace` map for every element
when they are not needed.

#### `fn supports_light_events(&self) -> bool`

Return `true` if your backend implements the remaining optional methods.

#### `fn peek_light_event(&mut self) -> Result<XmlLightEvent, String>`

Return the next event without consuming it.  Must support being called multiple times
in a row (i.e. maintain a peek buffer).

#### `fn next_light_event(&mut self) -> Result<XmlLightEvent, String>`

Consume and return the next light event.  The default implementation falls back to
`next_event()` + `From` conversion, so even non-light backends work — they just don't
get the performance benefit.

#### `fn peek_attributes(&mut self) -> Result<Option<Vec<XmlAttribute>>, String>`

Peek at the next event.  If it is a `StartElement`, return its attributes.  Otherwise
return `Ok(None)`.  This is called *before* consuming the start element, so your backend
needs to be able to inspect attributes without advancing the parser.

#### `fn read_inner_text(&mut self) -> Result<Option<String>, String>`

Consume a just-peeked `StartElement`, read all text content until the matching
`EndElement`, and return:

| Content | Return |
|---------|--------|
| `<tag>hello</tag>` | `Ok(Some("hello".into()))` |
| `<tag></tag>` | `Ok(None)` |
| `<tag><child/>…</tag>` | `Err("Expected text content in <tag>")` |

The matching `EndElement` must **not** be consumed — it should remain in the peek buffer
so that the caller's depth tracking stays correct.

### Peek / one-element look-ahead

The `Deserializer<P>` wrapper maintains its own `peeked: Option<XmlReadEvent>` cache.
When your backend supports light events, the `Deserializer` will prefer `peek_light_event()`
and `peek_attributes()` over materializing a full `XmlReadEvent`.  Your backend is
responsible for its own internal one-element look-ahead to satisfy these peek calls.

---

## Serialization — `XmlEventWriter`

```rust
pub trait XmlEventWriter {
    fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String>;
}
```

A single method.  Receive a `XmlWriteEvent` and emit the corresponding XML.  The
implementation must handle all four variants (`StartElement`, `EndElement`, `Characters`,
`CData`).  Return `Err(String)` on I/O errors or well-formedness violations.

> **Note:** Serialization is currently less pluggable than deserialization.  The
> `Serializer<W>` struct in `yaserde::ser` wraps an `xml::writer::EventWriter` directly.
> The `XmlEventWriter` trait exists as the future abstraction boundary.  For now,
> implementing `XmlEventWriter` prepares your backend for when serialization is fully
> decoupled.

---

## Wiring It Up

### Static dispatch (fastest)

```rust
use yaserde::de::from_reader_with_parser;

let my_parser = MyCustomReader::from_reader(std::io::Cursor::new(xml_string));
let value: MyStruct = from_reader_with_parser(my_parser)?;
```

`from_reader_with_parser` is generic over `P: XmlEventReader`, so there is no vtable
overhead.

### Dynamic dispatch (runtime selection)

```rust
use yaserde::de::from_reader_dyn;

let parser: Box<dyn yaserde::xml::XmlEventReader> = Box::new(
    MyCustomReader::from_reader(std::io::Cursor::new(xml_string)),
);
let value: MyStruct = from_reader_dyn(parser)?;
```

### Choosing the default backend

YaSerDe selects the default reader based on Cargo features:

| Feature flag | `from_reader` uses |
|---|---|
| *(none)* | `XmlRsReader` (xml-rs) |
| `quick-xml-backend` | `QuickXmlReader` (quick-xml) |

Your custom backend is always available explicitly via `from_reader_with_parser` or
`from_reader_dyn` regardless of which default is active.

---

## Full Example — A Minimal Reader

Below is a complete, working `XmlEventReader` backed by a hand-written pull parser for
tiny XML snippets.  It supports light events and attribute peeking.

```rust
use std::io::Read;
use yaserde::xml::{
    XmlAttribute, XmlEventReader, XmlLightEvent, XmlName, XmlNamespace, XmlReadEvent,
};

/// A toy reader that parses a single element with text content:
///   `<tag attr="val">text</tag>`
///
/// It only handles the happy path for demonstration purposes.
pub struct ToyReader<R: Read> {
    inner: R,
    /// One-element look-ahead buffer (holds the full event).
    buffered: Option<XmlReadEvent>,
}

impl<R: Read> ToyReader<R> {
    pub fn from_reader(inner: R) -> Self {
        Self {
            inner,
            buffered: None,
        }
    }

    /// Read the entire input into a string and parse it.
    /// A real backend would use a streaming parser instead.
    fn parse_next(&mut self) -> Result<XmlReadEvent, String> {
        let mut buf = String::new();
        self.inner.read_to_string(&mut buf).map_err(|e| e.to_string())?;
        let trimmed = buf.trim();

        if trimmed.is_empty() {
            return Ok(XmlReadEvent::EndDocument);
        }

        // Extremely simplified: parse <name attr="val">text</name>
        // A production backend would use a proper XML parser library.
        if let Some(rest) = trimmed.strip_prefix('<') {
            if let Some(tag_content) = rest.strip_suffix('>') {
                if tag_content.starts_with('/') {
                    // End element
                    let local_name = tag_content[1..].to_string();
                    return Ok(XmlReadEvent::EndElement {
                        name: XmlName::local(local_name),
                    });
                }
                // Start element — extract name and attributes
                let mut parts = tag_content.splitn(2, ' ');
                let local_name = parts.next().unwrap().to_string();
                let mut attributes = Vec::new();
                if let Some(attr_str) = parts.next() {
                    for pair in attr_str.split_whitespace() {
                        if let Some((k, v)) = pair.split_once('=') {
                            let v = v.trim_matches('"');
                            attributes.push(XmlAttribute::new(
                                XmlName::local(k.to_string()),
                                v.to_string(),
                            ));
                        }
                    }
                }
                return Ok(XmlReadEvent::StartElement {
                    name: XmlName::local(local_name),
                    attributes,
                    namespace: XmlNamespace::empty(),
                });
            }
        }

        // Treat as character content
        Ok(XmlReadEvent::Characters(trimmed.to_string()))
    }

    fn ensure_buffered(&mut self) -> Result<(), String> {
        if self.buffered.is_none() {
            self.buffered = Some(self.parse_next()?);
        }
        Ok(())
    }
}

impl<R: Read> XmlEventReader for ToyReader<R> {
    fn next_event(&mut self) -> Result<XmlReadEvent, String> {
        if let Some(event) = self.buffered.take() {
            return Ok(event);
        }
        self.parse_next()
    }

    // ── Light-event support ──

    fn supports_light_events(&self) -> bool {
        true
    }

    fn peek_light_event(&mut self) -> Result<XmlLightEvent, String> {
        self.ensure_buffered()?;
        Ok(XmlLightEvent::from(
            self.buffered.as_ref().unwrap(),
        ))
    }

    fn next_light_event(&mut self) -> Result<XmlLightEvent, String> {
        self.ensure_buffered()?;
        let event = self.buffered.take().unwrap();
        Ok(XmlLightEvent::from(&event))
    }

    fn peek_attributes(&mut self) -> Result<Option<Vec<XmlAttribute>>, String> {
        self.ensure_buffered()?;
        match &self.buffered {
            Some(XmlReadEvent::StartElement { attributes, .. }) => Ok(Some(attributes.clone())),
            _ => Ok(None),
        }
    }

    fn read_inner_text(&mut self) -> Result<Option<String>, String> {
        // Consume the buffered StartElement
        self.ensure_buffered()?;
        let name = match self.buffered.take() {
            Some(XmlReadEvent::StartElement { name, .. }) => name,
            other => {
                self.buffered = other;
                return Err("Internal error: expected StartElement".into());
            }
        };

        // Read until the matching EndElement
        let event = self.parse_next()?;
        match event {
            XmlReadEvent::Characters(text) => {
                // Expect EndElement next
                let end = self.parse_next()?;
                match end {
                    XmlReadEvent::EndElement { name: end_name } if end_name == name => {
                        Ok(Some(text))
                    }
                    XmlReadEvent::EndElement { name: end_name } => Err(format!(
                        "End tag </{}> didn't match <{}>",
                        end_name.local_name, name.local_name
                    )),
                    other => {
                        self.buffered = Some(other);
                        Err(format!("Expected text content in <{}>", name.local_name))
                    }
                }
            }
            XmlReadEvent::EndElement { name: end_name } if end_name == name => {
                // Empty element — leave the EndElement consumed
                Ok(None)
            }
            other => {
                self.buffered = Some(other);
                Err(format!("Expected text content in <{}>", name.local_name))
            }
        }
    }
}
```

### Using it

```rust
use yaserde::de::from_reader_with_parser;

let xml = "<Root><msg>hello</msg></Root>";
let reader = ToyReader::from_reader(std::io::Cursor::new(xml));
let value: MyStruct = from_reader_with_parser(reader).unwrap();
```

---

## Testing Your Backend

YaSerDe's test suite provides a reusable set of parity checks.  You can validate your
backend against the same cases that `XmlRsReader` and `QuickXmlReader` are tested with:

```rust
#[test]
fn my_backend_parity() {
    use yaserde::de::from_reader_with_parser;

    // Simple text round-trip
    let xml = "<Root><item>hello world</item></Root>";
    let result: Root = from_reader_with_parser(
        MyReader::from_reader(xml.as_bytes())
    ).unwrap();
    assert_eq!(result, Root { item: "hello world".into() });

    // Namespaced element
    let xml = r#"<s:Envelope xmlns:s="urn:test" id="1">
                    <s:child>value</s:child>
                 </s:Envelope>"#;
    let result: NamespacedEnvelope = from_reader_with_parser(
        MyReader::from_reader(xml.as_bytes())
    ).unwrap();
    assert_eq!(result, NamespacedEnvelope { id: "1".into(), child: "value".into() });

    // Empty element → None
    let xml = "<Root><item/></Root>";
    let result: Root = from_reader_with_parser(
        MyReader::from_reader(xml.as_bytes())
    ).unwrap();
    assert_eq!(result, Root { item: String::new() });

    // Flatten
    let xml = "<FlattenOuter><known>k</known><other>o</other></FlattenOuter>";
    let result: FlattenOuter = from_reader_with_parser(
        MyReader::from_reader(xml.as_bytes())
    ).unwrap();
    assert_eq!(result, FlattenOuter {
        known: "k".into(),
        extra: FlattenExtra { other: "o".into() },
    });
}
```

### Key behaviors to verify

| Scenario | Expected behavior |
|----------|-------------------|
| Whitespace-only text between elements | Skipped (not emitted as `Characters`) |
| Mixed text + CDATA | Coalesced into a single `Characters` event |
| `<!-- comments -->` | Silently skipped |
| Self-closing `<tag/>` | Emits `StartElement` + `EndElement` |
| Mismatched close tag | `Err` with a descriptive message |
| Unexpected EOF | `Err("Unexpected end of stream: no root element found")` |
| Namespace prefixes | Resolved and stored in `XmlName.namespace` |
| Duplicate attribute names | Your choice — error or last-wins |
| Empty element text (`<a></a>`) | `read_inner_text()` returns `Ok(None)` |

---

## Summary

| What to implement | When |
|---|---|
| `next_event()` | **Always** — minimum viable reader |
| `supports_light_events()` → `true` + light methods | For better performance with derived code |
| `XmlEventWriter::write_event()` | For custom serialization backends (future-proofing) |

The trait defaults ensure that implementing **only** `next_event()` gives you a working
backend.  Add the light-event methods when you want the derive macros to take the fast
path through your parser.
