use super::*;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, TimePeriod};
use crate::domain::services::resource_usage::availability::calculate;
use chrono::TimeZone;

/// 表示のテストは日本時間で行う（時刻の見え方が環境に左右されないように）
const TOKYO: Option<&str> = Some("Asia/Tokyo");

/// 日本時間のその日の時刻
fn jst(day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, day, hour, 0, 0).unwrap() - chrono::Duration::hours(9)
}

fn now() -> DateTime<Utc> {
    jst(1, 14)
}

fn window() -> TimePeriod {
    TimePeriod::new(now(), jst(8, 14)).unwrap()
}

fn gpu(server: &str, device_number: u32) -> Resource {
    Resource::Gpu(Gpu::new(
        server.to_string(),
        device_number,
        "A100 80GB PCIe".to_string(),
    ))
}

fn room(name: &str) -> Resource {
    Resource::Room {
        name: name.to_string(),
    }
}

fn email(address: &str) -> EmailAddress {
    EmailAddress::new(address.to_string()).unwrap()
}

fn reservation(
    owner: &str,
    resources: Vec<Resource>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> ResourceUsage {
    ResourceUsage::new(
        email(owner),
        TimePeriod::new(from, to).unwrap(),
        resources,
        None,
    )
    .unwrap()
}

fn availabilities(resources: &[Resource], usages: &[ResourceUsage]) -> Vec<ResourceAvailability> {
    calculate(resources, &window(), usages)
}

fn no_displays() -> HashMap<EmailAddress, String> {
    HashMap::new()
}

/// メッセージ全体を1つの文字列にして、載っているかどうかを見る
fn rendered(content: &SlackMessageContent) -> String {
    format!("{:?}", content)
}

#[test]
fn the_headline_counts_what_is_free_right_now() {
    let resources = vec![gpu("gpu-server-1", 0), gpu("gpu-server-1", 1)];
    let usages = vec![reservation(
        "sato@example.com",
        vec![gpu("gpu-server-1", 1)],
        jst(1, 10),
        jst(1, 18),
    )];

    let headline = headline(&availabilities(&resources, &usages), now());

    assert_eq!(headline, "いま GPU 1台 が空いています");
}

#[test]
fn the_headline_counts_gpus_and_rooms_separately() {
    let resources = vec![gpu("gpu-server-1", 0), room("Meeting Room A")];

    let headline = headline(&availabilities(&resources, &[]), now());

    assert_eq!(
        headline, "いま GPU 1台、部屋 1件 が空いています",
        "台と件は数え方が違うので、ひとつの数にまとめない"
    );
}

#[test]
fn the_headline_omits_a_kind_that_has_nothing_free() {
    let resources = vec![gpu("gpu-server-1", 0), room("Meeting Room A")];
    let usages = vec![reservation(
        "sato@example.com",
        vec![room("Meeting Room A")],
        jst(1, 10),
        jst(1, 16),
    )];

    let headline = headline(&availabilities(&resources, &usages), now());

    assert_eq!(
        headline, "いま GPU 1台 が空いています",
        "0件の種類を並べても読み手の役に立たない"
    );
}

#[test]
fn a_fully_booked_lab_says_so_instead_of_counting_zero() {
    let resources = vec![gpu("gpu-server-1", 0)];
    let usages = vec![reservation(
        "sato@example.com",
        vec![gpu("gpu-server-1", 0)],
        jst(1, 10),
        jst(1, 18),
    )];

    let headline = headline(&availabilities(&resources, &usages), now());

    assert_eq!(headline, "いま空いているリソースはありません");
}

#[test]
fn a_fully_booked_lab_is_told_when_something_frees_up() {
    let resources = vec![gpu("gpu-server-1", 0), gpu("gpu-server-1", 1)];
    let usages = vec![
        reservation(
            "sato@example.com",
            vec![gpu("gpu-server-1", 0)],
            jst(1, 10),
            jst(1, 18),
        ),
        reservation(
            "tanaka@example.com",
            vec![gpu("gpu-server-1", 1)],
            jst(1, 10),
            jst(1, 16),
        ),
    ];

    let line = earliest_free_line(&availabilities(&resources, &usages), now(), TOKYO).unwrap();

    assert!(
        line.contains("16:00"),
        "待てば使えるなら、いつ空くかまで伝える: {line}"
    );
    assert!(
        line.contains('1'),
        "どれが空くのかが分からなければ待ちようがない: {line}"
    );
}

#[test]
fn nothing_extra_is_said_when_something_is_already_free() {
    let resources = vec![gpu("gpu-server-1", 0)];

    let line = earliest_free_line(&availabilities(&resources, &[]), now(), TOKYO);

    assert_eq!(line, None, "空きがあるなら待ち時間の話は要らない");
}

#[test]
fn a_reservation_holding_several_gpus_takes_one_line() {
    let resources = vec![
        gpu("gpu-server-1", 0),
        gpu("gpu-server-1", 1),
        gpu("gpu-server-1", 2),
    ];
    let usages = vec![reservation(
        "sato@example.com",
        vec![gpu("gpu-server-1", 1), gpu("gpu-server-1", 2)],
        jst(1, 10),
        jst(1, 18),
    )];

    let lines = ongoing_gpu_lines(
        &availabilities(&resources, &usages),
        now(),
        TOKYO,
        &no_displays(),
    )
    .unwrap();

    assert_eq!(
        lines.lines().count(),
        2,
        "見出しと、まとめられた1行だけになる: {lines}"
    );
    assert!(
        lines.contains("gpu-server-1 1,2"),
        "1件の予約が押さえている2枚は並べて書く: {lines}"
    );
}

#[test]
fn separate_reservations_take_separate_lines() {
    let resources = vec![gpu("gpu-server-1", 0), gpu("gpu-server-1", 1)];
    let usages = vec![
        reservation(
            "sato@example.com",
            vec![gpu("gpu-server-1", 0)],
            jst(1, 10),
            jst(1, 18),
        ),
        reservation(
            "tanaka@example.com",
            vec![gpu("gpu-server-1", 1)],
            jst(1, 10),
            jst(1, 16),
        ),
    ];

    let lines = ongoing_gpu_lines(
        &availabilities(&resources, &usages),
        now(),
        TOKYO,
        &no_displays(),
    )
    .unwrap();

    assert_eq!(lines.lines().count(), 3, "見出しと2件: {lines}");
    assert!(lines.contains("sato@example.com"), "{lines}");
    assert!(lines.contains("tanaka@example.com"), "{lines}");
}

#[test]
fn a_reserver_is_shown_by_their_display_name_when_it_is_known() {
    let resources = vec![gpu("gpu-server-1", 0)];
    let usages = vec![reservation(
        "sato@example.com",
        vec![gpu("gpu-server-1", 0)],
        jst(1, 10),
        jst(1, 18),
    )];
    let displays = HashMap::from([(email("sato@example.com"), "<@U01SATO>".to_string())]);

    let lines = ongoing_gpu_lines(
        &availabilities(&resources, &usages),
        now(),
        TOKYO,
        &displays,
    )
    .unwrap();

    assert!(lines.contains("<@U01SATO>"), "{lines}");
    assert!(
        !lines.contains("sato@example.com"),
        "表示名が分かるならメールアドレスは出さない: {lines}"
    );
}

#[test]
fn no_ongoing_lines_when_every_gpu_is_free() {
    let resources = vec![gpu("gpu-server-1", 0)];

    let lines = ongoing_gpu_lines(
        &availabilities(&resources, &[]),
        now(),
        TOKYO,
        &no_displays(),
    );

    assert_eq!(lines, None);
}

#[test]
fn a_free_room_is_listed_without_a_reserver() {
    let resources = vec![room("Meeting Room A")];

    let lines = room_lines(
        &availabilities(&resources, &[]),
        now(),
        TOKYO,
        &no_displays(),
    )
    .unwrap();

    assert!(lines.contains("✅ Meeting Room A"), "{lines}");
}

#[test]
fn a_busy_room_carries_its_reserver_and_end_time_on_the_same_line() {
    let resources = vec![room("Meeting Room A")];
    let usages = vec![reservation(
        "sato@example.com",
        vec![room("Meeting Room A")],
        jst(1, 10),
        jst(1, 16),
    )];

    let lines = room_lines(
        &availabilities(&resources, &usages),
        now(),
        TOKYO,
        &no_displays(),
    )
    .unwrap();

    assert!(lines.contains("🔴 Meeting Room A"), "{lines}");
    assert!(lines.contains("16:00"), "{lines}");
}

#[test]
fn no_room_section_when_no_rooms_are_configured() {
    let resources = vec![gpu("gpu-server-1", 0)];

    let lines = room_lines(
        &availabilities(&resources, &[]),
        now(),
        TOKYO,
        &no_displays(),
    );

    assert_eq!(lines, None);
}

#[test]
fn an_end_time_today_is_just_the_clock() {
    assert_eq!(format_moment(jst(1, 18), now(), TOKYO), "18:00");
}

#[test]
fn an_end_time_tomorrow_says_so() {
    assert_eq!(format_moment(jst(2, 9), now(), TOKYO), "明日 09:00");
}

#[test]
fn an_end_time_further_out_carries_its_date() {
    assert_eq!(
        format_moment(jst(5, 9), now(), TOKYO),
        "8/5 09:00",
        "「4日後」では読み手が数え直すことになる"
    );
}

#[test]
fn the_message_answers_before_it_details() {
    let resources = vec![
        gpu("gpu-server-1", 0),
        gpu("gpu-server-1", 1),
        room("Meeting Room A"),
    ];
    let usages = vec![reservation(
        "sato@example.com",
        vec![gpu("gpu-server-1", 1)],
        jst(1, 10),
        jst(1, 18),
    )];

    let content = build(
        &availabilities(&resources, &usages),
        now(),
        TOKYO,
        &no_displays(),
    );

    assert_eq!(
        content.text.as_deref(),
        Some("いま GPU 1台、部屋 1件 が空いています"),
        "通知やプレビューには答えの一文が出る"
    );

    let text = rendered(&content);
    assert!(text.contains("gpu-server-1"), "表が載る: {text}");
    assert!(text.contains("使用中のGPU"), "内訳が載る: {text}");
    assert!(text.contains("部屋"), "部屋が載る: {text}");
}

#[test]
fn a_lab_with_no_resources_configured_says_so() {
    let content = build(&[], now(), TOKYO, &no_displays());

    assert_eq!(
        content.text.as_deref(),
        Some("予約できるリソースが設定されていません。"),
        "空の表を見せても、設定が抜けていることは伝わらない"
    );
}
