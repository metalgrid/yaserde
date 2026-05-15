# Development Plan: YaSerDe Performance Fast Paths

> Created: 2026-05-15
> Status: LOCKED
> Do not modify this file. To change direction, create a new plan.

## Goal

Improve YaSerDe XML serialization and deserialization performance by reducing unnecessary owned xml-rs event construction, event cloning, namespace map cloning, and string allocation, while preserving the existing public compatibility APIs.

## Context

The current implementation parses and writes XML with `quick-xml`, but the internal and generated derive paths still route through xml-rs-style owned events. In deserialization, `yaserde/src/de/mod.rs` eagerly converts every quick-xml start tag into `xml::reader::XmlEvent::StartElement` with `OwnedName`, owned attributes, and a full `Namespace` map. Generated deserializers in `yaserde_derive/src/de/expand_struct.rs` then clone these events with `reader.peek()?.to_owned()` in the parse loop. In serialization, `yaserde/src/ser/mod.rs` clones the whole namespace scope per start element, and generated serializers in `yaserde_derive/src/ser/*` construct xml-rs events that `Serializer::write` converts back into quick-xml events. The compatibility APIs should remain available, but generated derive code can use faster internal paths.

## Steps

### Step 1: Establish baseline and guardrails

Run the existing workspace test suite and identify representative coverage for nested structs, repeated elements, attributes, namespaces, text content, CDATA, empty elements, flatten, and public API behavior. Add focused tests or fixtures only if existing coverage is insufficient for the planned refactors. Record reproducible test and benchmark/profiling commands for validating each stage.

### Step 2: Stop cloning full events in generated deserializers

Update `yaserde_derive/src/de/expand_struct.rs` so generated deserializers borrow `reader.peek()?` instead of cloning the entire `XmlEvent` in the main parse loop and initial root inspection. Extract only minimal values, such as local name and namespace, before calling mutable reader methods. Keep attribute handling correct and only clone attribute data when necessary for generated attribute loading.

### Step 3: Optimize serializer namespace stack

Update `yaserde/src/ser/mod.rs` to avoid cloning the entire parent namespace `HashMap` for every start element. Replace the full-scope stack with per-element namespace frames/deltas or an equivalent active-scope mechanism. Preserve namespace declaration behavior and public `Serializer::write(XmlEvent)` compatibility.

### Step 4: Add direct serializer methods and migrate derives

Add direct serializer methods in `yaserde/src/ser/mod.rs`, such as start element, end element, text, and CDATA helpers, backed by the same quick-xml writer logic. Keep `Serializer::write(XmlEvent)` as a compatibility wrapper. Update generated serializer code in `yaserde_derive/src/ser/implement_serializer.rs`, `yaserde_derive/src/ser/expand_struct.rs`, and related serializer modules where appropriate to call direct methods instead of constructing xml-rs events when possible.

### Step 5: Add deserializer fast paths for derives

Add lightweight deserializer helpers in `yaserde/src/de/mod.rs` for derive-generated code, such as peeking the current element name/namespace, consuming start/end events, and reading text without requiring full xml-rs event materialization. Update generated deserializers in `yaserde_derive/src/de/expand_struct.rs` and related deserializer modules where appropriate to use these helpers. Continue supporting the existing `peek()` and `next_event()` APIs for compatibility.

### Step 6: Optimize text handling with borrowed/Cow paths

After the deserializer fast path shape is stable, add text-reading APIs that avoid allocation where quick-xml can provide decoded/unescaped text without requiring ownership. Ensure generated visitors can continue parsing from `&str`. Preserve trimming and escaping semantics from the existing implementation.

### Step 7: Final validation and profiling

Run the full test suite and compare before/after performance on representative XML-to-struct and struct-to-XML workloads. Validate namespace, attribute, flatten, CDATA, empty element, text content, and public API behavior. Confirm profiling no longer shows the original clone/allocation hotspots as dominant costs, or document remaining bottlenecks.

## Risks & Considerations

- Borrow checker constraints in generated deserializer code may require careful extraction of owned names before mutable reader calls.
- Namespace behavior is subtle; tests must cover default namespaces, prefixes, repeated declarations, and inherited namespace scopes.
- Flatten support depends on writing unused events through xml-rs compatibility paths and may not immediately benefit from fast paths.
- Empty element handling must continue to produce a start event followed by a matching end event semantically.
- Borrowed/Cow text APIs must respect quick-xml buffer lifetimes and cannot return references that outlive reader buffers.
- Public compatibility APIs must remain available even if derives stop using them internally.

## Dependencies

Step 1 should happen first to establish guardrails. Step 2 and Step 3 are independent low-risk improvements and can be implemented before the larger fast-path work. Step 4 should precede broad serializer derive migration. Step 5 should precede Step 6 because borrowed text APIs depend on the final deserializer helper shape. Step 7 depends on all implementation steps.
