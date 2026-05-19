use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::io::Cursor;

#[macro_use]
extern crate yaserde_derive;

// ── FastXML address-book schema ──────────────────────────────────────────

#[derive(Debug, PartialEq, YaDeserialize)]
struct Database {
    person: Vec<Person>,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct Person {
    #[yaserde(attribute = true)]
    id: String,
    name: String,
    email: String,
    phone: String,
    address: Address,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct Address {
    #[yaserde(attribute = true)]
    city: String,
    #[yaserde(attribute = true)]
    state: String,
    #[yaserde(attribute = true)]
    zip: String,
    #[yaserde(attribute = true)]
    country: String,
    line1: String,
    line2: String,
}

// ── Synthetic types for targeted micro-benchmarks ────────────────────────

/// Flat struct with a mix of attributes and child elements.
#[derive(Debug, PartialEq, YaDeserialize)]
struct Simple {
    #[yaserde(attribute = true)]
    id: i32,
    #[yaserde(attribute = true)]
    active: bool,
    name: String,
    score: f64,
}

/// Nested struct two levels deep.
#[derive(Debug, PartialEq, YaDeserialize)]
struct Nested {
    #[yaserde(attribute = true)]
    version: String,
    header: Header,
    body: Body,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct Header {
    timestamp: u64,
    sender: String,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct Body {
    items: Vec<Item>,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct Item {
    #[yaserde(attribute = true)]
    index: u32,
    value: String,
}

/// Namespaced struct.
#[derive(Debug, PartialEq, YaDeserialize)]
#[yaserde(
    rename = "Envelope",
    namespaces = { "s" = "urn:test" },
    prefix = "s"
)]
struct NamespacedEnvelope {
    #[yaserde(attribute = true)]
    id: String,
    #[yaserde(rename = "child", prefix = "s")]
    child: String,
}

/// Enum variant deserialization.
#[derive(Debug, PartialEq, YaDeserialize)]
enum Status {
    Active,
    Inactive,
    Pending,
}

impl Default for Status {
    fn default() -> Self {
        Status::Active
    }
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct StatusHolder {
    status: Status,
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn de_xml_rs<T: yaserde::YaDeserialize>(xml: &str) -> T {
    let parser = yaserde::xml::XmlRsReader::from_reader(xml.as_bytes());
    yaserde::de::from_reader_with_parser(parser).unwrap()
}

fn de_quick_xml<T: yaserde::YaDeserialize>(xml: &str) -> T {
    let parser = yaserde::xml::QuickXmlReader::from_reader(Cursor::new(xml));
    yaserde::de::from_reader_with_parser(parser).unwrap()
}

// ── FastXML address-book benchmarks ───────────────────────────────────────

const ADDRESS_SMALL: &str = include_str!("data/address-small.xml");
const ADDRESS_MIDDLE: &str = include_str!("data/address-middle.xml");

/// The big file is 18 MB so we read it at group-start rather than embedding
/// it with `include_str!`.
fn address_big() -> &'static String {
    use std::sync::OnceLock;
    static DATA: OnceLock<String> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(dir.join("benches/data/address-big.xml"))
            .expect("failed to read address-big.xml")
    })
}

fn bench_address_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize/address-small");
    group.sample_size(50);
    group.bench_function("xml-rs", |b| {
        b.iter(|| {
            let _: Database = de_xml_rs(black_box(ADDRESS_SMALL));
        })
    });
    group.bench_function("quick-xml", |b| {
        b.iter(|| {
            let _: Database = de_quick_xml(black_box(ADDRESS_SMALL));
        })
    });
    group.finish();
}

fn bench_address_middle(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize/address-middle");
    group.sample_size(20);
    group.bench_function("xml-rs", |b| {
        b.iter(|| {
            let _: Database = de_xml_rs(black_box(ADDRESS_MIDDLE));
        })
    });
    group.bench_function("quick-xml", |b| {
        b.iter(|| {
            let _: Database = de_quick_xml(black_box(ADDRESS_MIDDLE));
        })
    });
    group.finish();
}

fn bench_address_big(c: &mut Criterion) {
    let big = address_big();
    let mut group = c.benchmark_group("deserialize/address-big");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(30));
    group.bench_function("xml-rs", |b| {
        b.iter(|| {
            let _: Database = de_xml_rs(black_box(big));
        })
    });
    group.bench_function("quick-xml", |b| {
        b.iter(|| {
            let _: Database = de_quick_xml(black_box(big));
        })
    });
    group.finish();
}

// ── Synthetic micro-benchmarks ────────────────────────────────────────────

fn bench_simple_struct(c: &mut Criterion) {
    let xml = r#"<Simple id="42" active="true"><name>Alice</name><score>98.6</score></Simple>"#;
    let mut group = c.benchmark_group("deserialize/simple_struct");
    group.bench_function("xml-rs", |b| {
        b.iter(|| { let _: Simple = de_xml_rs(black_box(xml)); })
    });
    group.bench_function("quick-xml", |b| {
        b.iter(|| { let _: Simple = de_quick_xml(black_box(xml)); })
    });
    group.finish();
}

fn bench_nested_struct(c: &mut Criterion) {
    let xml = r#"<Nested version="1.0">
        <header><timestamp>1700000000</timestamp><sender>bench</sender></header>
        <body>
            <items><Item index="0"><value>zero</value></Item>
                   <Item index="1"><value>one</value></Item>
                   <Item index="2"><value>two</value></Item></items>
        </body>
    </Nested>"#;
    let mut group = c.benchmark_group("deserialize/nested_struct");
    group.bench_function("xml-rs", |b| {
        b.iter(|| { let _: Nested = de_xml_rs(black_box(xml)); })
    });
    group.bench_function("quick-xml", |b| {
        b.iter(|| { let _: Nested = de_quick_xml(black_box(xml)); })
    });
    group.finish();
}

fn bench_namespaced(c: &mut Criterion) {
    let xml = r#"<s:Envelope xmlns:s="urn:test" id="123"><s:child>hello</s:child></s:Envelope>"#;
    let mut group = c.benchmark_group("deserialize/namespaced");
    group.bench_function("xml-rs", |b| {
        b.iter(|| { let _: NamespacedEnvelope = de_xml_rs(black_box(xml)); })
    });
    group.bench_function("quick-xml", |b| {
        b.iter(|| { let _: NamespacedEnvelope = de_quick_xml(black_box(xml)); })
    });
    group.finish();
}

fn bench_enum_variant(c: &mut Criterion) {
    let xml = "<StatusHolder><status>Pending</status></StatusHolder>";
    let mut group = c.benchmark_group("deserialize/enum_variant");
    group.bench_function("xml-rs", |b| {
        b.iter(|| { let _: StatusHolder = de_xml_rs(black_box(xml)); })
    });
    group.bench_function("quick-xml", |b| {
        b.iter(|| { let _: StatusHolder = de_quick_xml(black_box(xml)); })
    });
    group.finish();
}

/// Synthetic large document at multiple sizes to show scaling behaviour.
fn bench_large_synthetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialize/large_synthetic");

    for size in [64, 256, 1024].iter() {
        let xml = generate_large_xml(*size);
        group.bench_with_input(BenchmarkId::new("xml-rs", size), &xml, |b, xml| {
            b.iter(|| { let _: LargeDoc = de_xml_rs(black_box(xml)); })
        });
        group.bench_with_input(BenchmarkId::new("quick-xml", size), &xml, |b, xml| {
            b.iter(|| { let _: LargeDoc = de_quick_xml(black_box(xml)); })
        });
    }
    group.finish();
}

// ── Synthetic large doc types ─────────────────────────────────────────────

#[derive(Debug, PartialEq, YaDeserialize)]
struct LargeDoc {
    entry: Vec<Entry>,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct Entry {
    #[yaserde(attribute = true)]
    key: String,
    text: String,
}

fn generate_large_xml(n: usize) -> String {
    let mut s = String::from("<LargeDoc>");
    for i in 0..n {
        s.push_str(&format!(
            r#"<entry key="k{}"><text>value {} with some extra text to bulk it out</text></entry>"#,
            i, i
        ));
    }
    s.push_str("</LargeDoc>");
    s
}

// ── Criterion entry point ─────────────────────────────────────────────────

criterion_group!(
    address_benches,
    bench_address_small,
    bench_address_middle,
    bench_address_big,
);
criterion_group!(
    synthetic_benches,
    bench_simple_struct,
    bench_nested_struct,
    bench_namespaced,
    bench_enum_variant,
    bench_large_synthetic,
);
criterion_main!(address_benches, synthetic_benches);
