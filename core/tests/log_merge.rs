//! The merge that turns the log lines of several containers or nodes into
//! one stream in the order they were written.

use std::time::Duration;

use mm::domain::observability::{LogMerge, LogRecord, LogSource, log_instant, merged_tail};
use mm::domain::runtime::ServiceIdentity;
use mm::domain::workspace::ProjectId;

fn line(service: &str, container: &str, timestamp: Option<&str>, message: &str) -> LogRecord {
    LogRecord {
        identity: ServiceIdentity {
            project_id: ProjectId(uuid::Uuid::from_u128(7)),
            environment: "production".to_owned(),
            stack: "app".to_owned(),
            module: None,
            service: service.to_owned(),
        },
        sequence: 0,
        source: LogSource::Stdout,
        node: None,
        container: Some(container.to_owned()),
        timestamp: timestamp.map(str::to_owned),
        message: message.to_owned(),
        lossy_utf8: false,
        truncated: false,
        reconnect: 0,
    }
}

fn at(second: u32, message: &str) -> LogRecord {
    line(
        "web",
        "shop-web-1",
        Some(&format!("2026-11-14T05:11:{second:02}.000000000Z")),
        message,
    )
}

fn messages(records: &[LogRecord]) -> Vec<&str> {
    records
        .iter()
        .map(|record| record.message.as_str())
        .collect()
}

const NOW: Duration = Duration::from_secs(100);
const HOLD: Duration = Duration::from_millis(400);

#[test]
fn lines_of_two_sources_come_out_in_the_order_they_were_written() {
    let mut merge = LogMerge::following(2, HOLD);
    merge.push(0, at(1, "a1"), NOW);
    merge.push(0, at(4, "a4"), NOW);
    merge.push(1, at(2, "b2"), NOW);
    merge.push(1, at(3, "b3"), NOW);
    merge.push(1, at(5, "b5"), NOW);
    let released = merge.release(NOW);
    // Everything up to what both sources have shown: source 0 may still
    // deliver a line between 4 and 5, so b5 waits.
    assert_eq!(messages(&released), ["a1", "b2", "b3", "a4"]);
    assert_eq!(
        released
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    // After the hold nothing older has come, so it goes out.
    assert!(merge.release(NOW + HOLD / 2).is_empty());
    assert_eq!(merge.next_release(), Some(NOW + HOLD));
    assert_eq!(messages(&merge.release(NOW + HOLD)), ["b5"]);
}

#[test]
fn lines_of_one_instant_keep_the_order_of_their_sources_and_of_arrival() {
    let mut merge = LogMerge::complete(3);
    merge.push(2, at(7, "c-first"), NOW);
    merge.push(2, at(7, "c-second"), NOW);
    merge.push(0, at(7, "a"), NOW);
    merge.push(1, at(7, "b"), NOW);
    for source in 0..3 {
        merge.end(source);
    }
    assert_eq!(
        messages(&merge.release(NOW)),
        ["a", "b", "c-first", "c-second"]
    );
    assert!(merge.is_finished());
}

#[test]
fn a_line_that_arrives_late_is_released_and_not_dropped() {
    let mut merge = LogMerge::following(2, HOLD);
    merge.push(0, at(10, "a10"), NOW);
    merge.push(1, at(11, "b11"), NOW);
    assert_eq!(messages(&merge.release(NOW)), ["a10"]);
    assert_eq!(messages(&merge.release(NOW + HOLD)), ["b11"]);
    // Source 0 was slow: its line is older than what already went out.
    merge.push(0, at(9, "a9-late"), NOW + HOLD);
    assert_eq!(messages(&merge.release(NOW + HOLD)), ["a9-late"]);
}

#[test]
fn a_source_that_ends_no_longer_holds_the_others_back() {
    let mut merge = LogMerge::following(3, HOLD);
    merge.push(0, at(1, "a1"), NOW);
    merge.push(1, at(2, "b2"), NOW);
    // Source 2 is a node that never answers: the others wait for it only
    // as long as the hold.
    assert!(merge.release(NOW).is_empty());
    merge.end(2);
    assert_eq!(messages(&merge.release(NOW)), ["a1"]);
    merge.end(0);
    assert_eq!(messages(&merge.release(NOW)), ["b2"]);
    assert!(!merge.is_finished());
    merge.end(1);
    assert!(merge.is_finished());
}

#[test]
fn a_read_that_ends_releases_nothing_out_of_order_however_long_a_source_takes() {
    let mut merge = LogMerge::complete(2);
    merge.push(0, at(5, "a5"), NOW);
    assert!(merge.release(NOW + Duration::from_secs(3600)).is_empty());
    assert_eq!(merge.next_release(), None);
    merge.push(1, at(4, "b4"), NOW);
    merge.end(1);
    assert_eq!(messages(&merge.release(NOW)), ["b4", "a5"]);
}

#[test]
fn a_line_without_a_timestamp_stays_behind_the_line_it_followed() {
    let mut merge = LogMerge::complete(2);
    merge.push(0, at(3, "a3"), NOW);
    merge.push(0, line("web", "shop-web-1", None, "a3-continued"), NOW);
    merge.push(1, at(2, "b2"), NOW);
    merge.push(1, at(4, "b4"), NOW);
    merge.end(0);
    merge.end(1);
    assert_eq!(
        messages(&merge.release(NOW)),
        ["b2", "a3", "a3-continued", "b4"]
    );
}

#[test]
fn a_container_that_starts_later_joins_the_merge() {
    let mut merge = LogMerge::following(1, HOLD);
    merge.push(0, at(1, "old-1"), NOW);
    let new = merge.add_source();
    assert_eq!(new, 1);
    merge.push(new, at(2, "new-2"), NOW);
    merge.push(0, at(3, "old-3"), NOW);
    assert_eq!(messages(&merge.release(NOW)), ["old-1", "new-2"]);
}

#[test]
fn the_tail_of_several_sources_is_the_newest_lines_of_all_of_them() {
    let merged = merged_tail(
        vec![
            vec![at(1, "a1"), at(3, "a3"), at(5, "a5")],
            vec![at(2, "b2"), at(4, "b4")],
            Vec::new(),
        ],
        3,
    );
    assert_eq!(messages(&merged), ["a3", "b4", "a5"]);
    assert_eq!(
        merged
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

#[test]
fn an_engine_timestamp_is_an_instant_and_other_text_is_not() {
    assert_eq!(
        log_instant("2026-11-14T05:11:18.123456789Z"),
        Some((1_794_633_078, 123_456_789))
    );
    assert!(log_instant("2026-11-14T05:11:18.5+01:00").is_some());
    assert_eq!(log_instant("GET"), None);
    assert_eq!(log_instant(""), None);
}

#[test]
fn a_container_is_named_the_way_compose_prints_it() {
    let record = line("web", "shop-production-app-web-1", None, "x");
    assert_eq!(record.container_label(), Some("web-1"));
    // A service whose name appears in the project's name too.
    let record = line("app", "app-production-app-app-2", None, "x");
    assert_eq!(record.container_label(), Some("app-2"));
    // An old project with underscores, and a name that follows no pattern.
    assert_eq!(
        line("web", "shop_web_3", None, "x").container_label(),
        Some("web_3")
    );
    assert_eq!(
        line("web", "custom-name", None, "x").container_label(),
        Some("custom-name")
    );
    let mut unnamed = line("web", "x", None, "x");
    unnamed.container = None;
    assert_eq!(unnamed.container_label(), None);
}

#[test]
fn what_mm_says_about_a_node_is_a_line_of_its_own_kind() {
    let record = LogRecord::about_node(
        line("", "x", None, "x").identity,
        "app-2",
        "no answer: did not answer (Connection timed out)".to_owned(),
    );
    assert_eq!(record.source, LogSource::Runtime);
    assert_eq!(record.node.as_deref(), Some("app-2"));
    assert_eq!(record.container, None);
    let json = serde_json::to_value(&record).unwrap_or_default();
    assert_eq!(json["source"], "runtime");
    assert_eq!(json["node"], "app-2");
}
