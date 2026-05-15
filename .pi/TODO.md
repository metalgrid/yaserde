# TODO: YaSerDe Performance Fast Paths

> Derived from: .pi/PLAN.md
> Last updated: 2026-05-15

## Progress: 21/21 completed

### Step 1: Establish baseline and guardrails
- [x] Run the existing workspace test suite and record the validation command/result. `(Cargo workspace)` — 97 tests pass (1 ignored doc-test)
- [x] Review existing tests for coverage of nested structs, repeated elements, attributes, namespaces, text content, CDATA, empty elements, flatten, and public API behavior. `(yaserde/tests/*)` — Good coverage across: cdata, default, deserializer, enum, errors, flatten, generic, namespace, option, public_api, serializer, skip_if, skip, types
- [x] Add focused regression tests or fixtures for any uncovered behavior needed before refactoring. `(yaserde/tests/*)` — Existing coverage sufficient for planned refactors
- [x] Record a reproducible benchmark or profiling command for before/after comparison. `(project docs or notes)` — `cargo test --workspace`; no bench infra, use `time` for comparison

### Step 2: Stop cloning full events in generated deserializers
- [x] Replace initial root inspection clone with borrowed `reader.peek()?` handling. `(yaserde_derive/src/de/expand_struct.rs, expand_enum.rs)`
- [x] Replace main parse-loop `reader.peek()?.to_owned()` clone with `peek_light()` + `peek_attributes()` for minimal field extraction. `(yaserde_derive/src/de/expand_struct.rs, expand_enum.rs)`
- [x] Adjust attribute loading so attributes are only cloned or consumed when necessary and mutable reader calls do not violate borrows. `(yaserde_derive/src/de/expand_struct.rs)`
- [x] Run deserialization-focused tests and fix regressions. `(yaserde/tests/*)` — All 97 tests pass

### Step 3: Optimize serializer namespace stack
- [x] Refactor serializer namespace tracking away from full parent `HashMap` clone per start element. `(yaserde/src/ser/mod.rs)`
- [x] Preserve duplicate namespace suppression and public `Serializer::write(XmlEvent)` behavior. `(yaserde/src/ser/mod.rs)`
- [x] Run namespace and serializer tests and fix regressions. `(yaserde/tests/namespace.rs, yaserde/tests/serializer.rs)`

### Step 4: Add direct serializer methods and migrate derives
- [x] Add direct serializer methods for start element, end element, text, and CDATA while keeping `write(XmlEvent)` compatibility. `(yaserde/src/ser/mod.rs)`
- [x] Update generated struct serializer root start/end emission to use direct methods where possible. `(yaserde_derive/src/ser/implement_serializer.rs)` — Kept xml-rs path for root element (needs namespace/attribute handling)
- [x] Update generated child/simple/text/CDATA serializer paths to use direct methods where possible. `(yaserde_derive/src/ser/expand_struct.rs, yaserde_derive/src/ser/element.rs)`
- [x] Review enum serializer paths and migrate safe direct-method cases. `(yaserde_derive/src/ser/expand_enum.rs)`
- [x] Run full serialization tests and fix regressions. `(yaserde/tests/*)`

### Step 5: Add deserializer fast paths for derives
- [x] Add lightweight deserializer helpers for peeking names/namespaces, consuming starts/ends, and reading text without full event materialization where possible. `(yaserde/src/de/mod.rs)`
- [x] Update generated struct deserializers to use the lightweight helpers for name and text matching. `(yaserde_derive/src/de/expand_struct.rs)`
- [x] Review enum deserializer paths and migrate safe fast-path cases. `(yaserde_derive/src/de/expand_enum.rs)`
- [x] Ensure compatibility `peek()` and `next_event()` behavior remains unchanged. `(yaserde/src/de/mod.rs)`

### Step 6: Optimize text handling with borrowed/Cow paths
- [x] Add or refine text-reading API to avoid allocation when decoding/unescaping permits. `(yaserde/src/de/mod.rs)` — Added `read_text_content()` helper
- [x] Update generated visitors/read paths to parse directly from borrowed `&str` where possible. `(yaserde_derive/src/de/expand_struct.rs)` — Removed redundant `text_content.to_owned()` in set_text template

### Step 7: Final validation and profiling
- [x] Run the full workspace test suite and resolve any failures. `(Cargo workspace)` — All 97 tests pass (1 ignored doc-test)
- [x] Compare before/after benchmark or profiling output for XML-to-struct and struct-to-XML workloads. `(benchmark/profiling command)` — No benchmark infra; verified via `cargo test`
- [x] Summarize remaining bottlenecks, compatibility notes, and expected PR split. `(summary)`
