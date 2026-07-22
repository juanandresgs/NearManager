use std::time::{Duration, Instant};

use near_core::{Location, ProviderId, ResourceMetadata, ResourceRef};
use near_testkit::WorkflowHarness;
use near_ui::{CollectionEntry, CollectionSurface, FarWorkspace, Keymap, SemanticTheme};

const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
const THEME: &str = include_str!("../../../specs/theme.toml");
const REFERENCE_WIDTH: u16 = 100;
const REFERENCE_HEIGHT: u16 = 30;
const WARMUP_ITERATIONS: usize = 100;
const MEASURED_ITERATIONS: usize = 1_000;
const P95_BUDGET: Duration = Duration::from_millis(16);
const LARGE_COLLECTION_NAVIGATION_P95_BUDGET: Duration = Duration::from_micros(250);

#[test]
fn warm_navigation_key_to_semantic_render_p95_stays_below_budget() {
    let mut harness = WorkflowHarness::new(
        FarWorkspace::demo(),
        Keymap::from_toml(KEYMAP).unwrap(),
        SemanticTheme::from_toml(THEME).unwrap(),
        REFERENCE_WIDTH,
        REFERENCE_HEIGHT,
    );
    for _ in 0..WARMUP_ITERATIONS {
        harness.key("Down");
        std::hint::black_box(harness.snapshot());
    }
    let mut samples = Vec::with_capacity(MEASURED_ITERATIONS);
    for _ in 0..MEASURED_ITERATIONS {
        let started = Instant::now();
        harness.key("Down");
        std::hint::black_box(harness.snapshot());
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    assert!(
        p95 < P95_BUDGET,
        "100x30 warm navigation p95 {p95:?} exceeded {P95_BUDGET:?}"
    );
}

#[test]
fn navigation_latency_is_independent_of_a_hundred_thousand_item_collection() {
    let entries = (0..100_000)
        .map(|index| {
            let name = format!("record-{index:06}");
            CollectionEntry::new(
                ResourceRef {
                    provider: ProviderId::from("performance.records"),
                    location: Location::new(format!("perf://records/{name}")),
                },
                ResourceMetadata {
                    name,
                    ..ResourceMetadata::default()
                },
                "record",
            )
        })
        .collect();
    let mut surface = CollectionSurface::new(
        "performance.records",
        "performance.collection",
        "Records",
        Location::new("perf://records"),
        entries,
    );
    surface.set_cursor(50_000);
    let mut samples = Vec::with_capacity(MEASURED_ITERATIONS);
    for iteration in 0..MEASURED_ITERATIONS {
        let rows = if iteration % 2 == 0 { 1 } else { -1 };
        let started = Instant::now();
        surface.move_cursor(rows);
        std::hint::black_box(surface.cursor());
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    assert!(
        p95 < LARGE_COLLECTION_NAVIGATION_P95_BUDGET,
        "100k-item cursor movement p95 {p95:?} exceeded {LARGE_COLLECTION_NAVIGATION_P95_BUDGET:?}"
    );
}
