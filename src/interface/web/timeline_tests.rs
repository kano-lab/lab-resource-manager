use super::*;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, UsageId};
use crate::domain::common::EmailAddress;
use crate::infrastructure::config::{DeviceConfig, RoomConfig, ServerConfig};
use chrono::Duration;

fn config() -> ResourceConfig {
    ResourceConfig {
        servers: vec![ServerConfig {
            name: "Thalys".to_string(),
            calendar_id: "thalys@example.com".to_string(),
            devices: vec![
                DeviceConfig {
                    id: 0,
                    model: "A100".to_string(),
                },
                DeviceConfig {
                    id: 1,
                    model: "A100".to_string(),
                },
            ],
            notifications: vec![],
        }],
        rooms: vec![RoomConfig {
            name: "会議室".to_string(),
            calendar_id: "room@example.com".to_string(),
            notifications: vec![],
        }],
    }
}

fn at(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, day, hour, 0, 0).unwrap()
}

fn window(from: DateTime<Utc>, to: DateTime<Utc>) -> TimePeriod {
    TimePeriod::new(from, to).unwrap()
}

fn usage(
    owner: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    resources: Vec<Resource>,
) -> ResourceUsage {
    ResourceUsage::reconstruct(
        UsageId::from_string(format!("{}-{}", owner, from.timestamp())),
        EmailAddress::new(owner.to_string()).unwrap(),
        window(from, to),
        resources,
        None,
    )
    .unwrap()
}

fn gpu(server: &str, device: u32, model: &str) -> Resource {
    Resource::Gpu(Gpu::new(server.to_string(), device, model.to_string()))
}

fn tokyo() -> Tz {
    chrono_tz::Asia::Tokyo
}

fn row<'a>(timeline: &'a Timeline, label: &str) -> &'a TimelineRow {
    timeline
        .rows
        .iter()
        .find(|row| row.label == label)
        .unwrap_or_else(|| panic!("行が見つかりません: {}", label))
}

#[test]
fn every_configured_resource_gets_a_row_even_without_reservations() {
    let timeline = build(
        &config(),
        &[],
        &window(at(1, 0), at(8, 0)),
        at(1, 0),
        tokyo(),
    );

    let labels: Vec<&str> = timeline.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, vec!["GPU#0 (A100)", "GPU#1 (A100)", "会議室"]);
    assert!(timeline.rows.iter().all(|row| row.is_empty()));
}

#[test]
fn a_differing_model_name_still_lands_on_the_same_row() {
    // 永続化層から復元された予約が設定と違うモデル名を持つ状況。
    // 行を決めるのはサーバー名とデバイス番号なので、同じGPUとして扱われる。
    let usages = vec![usage(
        "kawaguchi@example.com",
        at(2, 0),
        at(2, 6),
        vec![gpu("Thalys", 0, "NVIDIA A100 80GB")],
    )];

    let timeline = build(
        &config(),
        &usages,
        &window(at(1, 0), at(8, 0)),
        at(1, 0),
        tokyo(),
    );

    assert_eq!(row(&timeline, "GPU#0 (A100)").lanes.len(), 1);
    assert!(row(&timeline, "GPU#1 (A100)").is_empty());
}

#[test]
fn overlapping_reservations_are_split_into_separate_lanes() {
    let usages = vec![
        usage(
            "a@example.com",
            at(2, 0),
            at(2, 12),
            vec![gpu("Thalys", 0, "A100")],
        ),
        usage(
            "b@example.com",
            at(2, 6),
            at(2, 18),
            vec![gpu("Thalys", 0, "A100")],
        ),
    ];

    let timeline = build(
        &config(),
        &usages,
        &window(at(1, 0), at(8, 0)),
        at(1, 0),
        tokyo(),
    );

    let target = row(&timeline, "GPU#0 (A100)");
    assert_eq!(target.lanes.len(), 2);
    assert_eq!(target.lanes[0].len(), 1);
    assert_eq!(target.lanes[1].len(), 1);
}

#[test]
fn reservations_that_do_not_overlap_share_a_lane() {
    let usages = vec![
        usage(
            "a@example.com",
            at(2, 0),
            at(2, 6),
            vec![gpu("Thalys", 0, "A100")],
        ),
        usage(
            "b@example.com",
            at(3, 0),
            at(3, 6),
            vec![gpu("Thalys", 0, "A100")],
        ),
    ];

    let timeline = build(
        &config(),
        &usages,
        &window(at(1, 0), at(8, 0)),
        at(1, 0),
        tokyo(),
    );

    let target = row(&timeline, "GPU#0 (A100)");
    assert_eq!(target.lanes.len(), 1);
    assert_eq!(target.lanes[0].len(), 2);
}

#[test]
fn a_reservation_of_several_resources_appears_on_every_row() {
    let usages = vec![usage(
        "a@example.com",
        at(2, 0),
        at(2, 6),
        vec![gpu("Thalys", 0, "A100"), gpu("Thalys", 1, "A100")],
    )];

    let timeline = build(
        &config(),
        &usages,
        &window(at(1, 0), at(8, 0)),
        at(1, 0),
        tokyo(),
    );

    assert_eq!(row(&timeline, "GPU#0 (A100)").lanes[0].len(), 1);
    assert_eq!(row(&timeline, "GPU#1 (A100)").lanes[0].len(), 1);
}

#[test]
fn a_reservation_reaching_beyond_the_window_is_clipped_and_marked() {
    let usages = vec![usage(
        "a@example.com",
        at(1, 0),
        at(10, 0),
        vec![gpu("Thalys", 0, "A100")],
    )];

    let timeline = build(
        &config(),
        &usages,
        &window(at(2, 0), at(8, 0)),
        at(2, 0),
        tokyo(),
    );

    let block = &row(&timeline, "GPU#0 (A100)").lanes[0][0];
    assert_eq!(block.start_ratio, 0.0);
    assert_eq!(block.end_ratio, 1.0);
    assert!(block.clipped_start);
    assert!(block.clipped_end);
}

#[test]
fn a_reservation_of_an_unconfigured_resource_still_gets_a_row() {
    // 設定から外れたサーバーの予約が黙って消えると、画面が「空いている」と嘘をつく。
    let usages = vec![usage(
        "a@example.com",
        at(2, 0),
        at(2, 6),
        vec![gpu("Retired", 0, "K80")],
    )];

    let timeline = build(
        &config(),
        &usages,
        &window(at(1, 0), at(8, 0)),
        at(1, 0),
        tokyo(),
    );

    let extra = timeline.rows.last().unwrap();
    assert_eq!(extra.group, "設定にないリソース");
    assert_eq!(extra.lanes[0].len(), 1);
}

#[test]
fn only_the_local_part_of_the_owners_address_is_shown() {
    let usages = vec![usage(
        "kawaguchi@example.com",
        at(2, 0),
        at(2, 6),
        vec![gpu("Thalys", 0, "A100")],
    )];

    let timeline = build(
        &config(),
        &usages,
        &window(at(1, 0), at(8, 0)),
        at(1, 0),
        tokyo(),
    );

    assert_eq!(
        row(&timeline, "GPU#0 (A100)").lanes[0][0].owner,
        "kawaguchi"
    );
}

#[test]
fn positions_are_ratios_of_the_displayed_window() {
    let usages = vec![usage(
        "a@example.com",
        at(3, 0),
        at(5, 0),
        vec![gpu("Thalys", 0, "A100")],
    )];

    // 表示範囲は8日間。3日目からの2日間は 2/8 の位置から 2/8 の幅。
    let timeline = build(
        &config(),
        &usages,
        &window(at(1, 0), at(9, 0)),
        at(1, 0),
        tokyo(),
    );

    let block = &row(&timeline, "GPU#0 (A100)").lanes[0][0];
    assert!((block.start_ratio - 0.25).abs() < 1e-9);
    assert!((block.width_ratio() - 0.25).abs() < 1e-9);
}

#[test]
fn the_current_time_marker_is_absent_when_it_falls_outside_the_window() {
    let inside = build(
        &config(),
        &[],
        &window(at(1, 0), at(8, 0)),
        at(4, 0),
        tokyo(),
    );
    assert!(inside.now_ratio.is_some());

    let outside = build(
        &config(),
        &[],
        &window(at(1, 0), at(8, 0)),
        at(20, 0),
        tokyo(),
    );
    assert!(outside.now_ratio.is_none());
}

#[test]
fn the_ticks_include_the_day_boundaries() {
    let timeline = build(
        &config(),
        &[],
        &window(at(1, 0), at(4, 0)),
        at(1, 0),
        tokyo(),
    );

    let day_boundaries: Vec<&Tick> = timeline
        .ticks
        .iter()
        .filter(|tick| tick.is_day_boundary)
        .collect();

    assert!(!day_boundaries.is_empty());
    assert!(
        timeline
            .ticks
            .iter()
            .all(|tick| (0.0..=1.0).contains(&tick.ratio))
    );
}

#[test]
fn every_day_gets_its_boundary_even_across_a_clock_shift() {
    // 夏時間の始まる日は現地の一日が23時間しかない。時間を一定間隔で足していくと
    // 刻みが日付の変わり目からずれ、その日の見出しが消える。
    let berlin = chrono_tz::Europe::Berlin;
    let start = berlin
        .with_ymd_and_hms(2026, 3, 28, 0, 0, 0)
        .unwrap()
        .with_timezone(&Utc);
    let end = start + Duration::days(3);

    let timeline = build(&config(), &[], &window(start, end), start, berlin);

    let boundaries: Vec<&str> = timeline
        .ticks
        .iter()
        .filter(|tick| tick.is_day_boundary)
        .map(|tick| tick.label.as_str())
        .collect();

    // 72時間ぶんを表示する。夏時間の始まる日は現地の一日が23時間しかないので、
    // 現地では3日と1時間にまたがり、4つの日付が視野に入る。
    assert_eq!(
        boundaries,
        vec!["3/28 (Sat)", "3/29 (Sun)", "3/30 (Mon)", "3/31 (Tue)"]
    );
}

#[test]
fn the_far_end_of_the_window_does_not_get_a_tick() {
    // 終端は表示範囲の終わりであって内側ではない。ここに見出しを置くと、
    // 表示していない日まで並んでいるように見える。
    let timeline = build(
        &config(),
        &[],
        &window(at(1, 0), at(4, 0)),
        at(1, 0),
        tokyo(),
    );

    let labels: Vec<&str> = timeline
        .ticks
        .iter()
        .filter(|tick| tick.is_day_boundary)
        .map(|tick| tick.label.as_str())
        .collect();

    // 8/1 0:00 から3日間なら、見出しは8/1・8/2・8/3の3つ。8/4は出ない
    assert_eq!(labels.len(), 3, "見出しが{:?}", labels);
}

#[test]
fn a_day_without_a_midnight_still_gets_its_heading() {
    // ベイルートの夏時間は真夜中に始まる。2026-03-29の0時は存在せず、
    // その日は1時から始まる。0時ちょうどだけを見ていると見出しを落とす。
    let beirut = chrono_tz::Asia::Beirut;
    let start = beirut
        .with_ymd_and_hms(2026, 3, 28, 0, 0, 0)
        .unwrap()
        .with_timezone(&Utc);

    let timeline = build(
        &config(),
        &[],
        &window(start, start + Duration::days(3)),
        start,
        beirut,
    );

    let boundaries: Vec<&str> = timeline
        .ticks
        .iter()
        .filter(|tick| tick.is_day_boundary)
        .map(|tick| tick.label.as_str())
        .collect();

    assert!(
        boundaries.contains(&"3/29 (Sun)"),
        "0時のない日の見出しが落ちている: {:?}",
        boundaries
    );
}

#[test]
fn the_start_of_a_day_falls_back_to_the_first_hour_that_exists() {
    let beirut = chrono_tz::Asia::Beirut;
    let date = chrono::NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();

    let at = day_start(date, beirut).expect("その日に存在する時刻があるはず");

    assert_eq!(at.format("%Y-%m-%d %H:%M").to_string(), "2026-03-29 01:00");
}
