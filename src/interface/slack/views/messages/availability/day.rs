//! ある一日の空き時間帯メッセージ
//!
//! 「いま使えるか」に答える一覧（`super::build`）に対して、こちらは
//! 「その日いつ空いているか」に答えます。予約枠を探す場面のための表示です。
//!
//! # 並べ方
//!
//! 使えるリソースを先に並べ、その日まったく空かないものは末尾に名前だけまとめます。
//! 探しているのは空いている時間であり、埋まっているものを一覧しても手数が増えるだけです。

use super::days::DayOption;
use super::{context, day_actions, format_resource, section};
use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::services::resource_usage::availability::ResourceAvailability;
use crate::infrastructure::notifier::formatter::to_zoned;
use chrono::NaiveDate;
use slack_morphism::prelude::*;

/// ある一日の空き時間帯メッセージを組み立てる
///
/// # 引数
/// * `availabilities` - 対象の日に絞って算出した空き（設定順）
/// * `window` - 対象の日の範囲。今日の場合は現在時刻から始まる
/// * `date` - 対象の日（表示タイムゾーンでの暦日）
/// * `today` - 表示タイムゾーンでの今日
/// * `options` - 日を切り替える選択肢
/// * `timezone` - 時刻表示に使うタイムゾーン名
pub fn build(
    availabilities: &[ResourceAvailability],
    window: &TimePeriod,
    date: NaiveDate,
    today: NaiveDate,
    options: &[DayOption],
    timezone: Option<&str>,
) -> SlackMessageContent {
    let heading = heading(date, today);
    let mut blocks: Vec<SlackBlock> = vec![section(format!("*{}*", heading))];

    let (free, fully_booked): (Vec<_>, Vec<_>) = availabilities
        .iter()
        .partition(|availability| !availability.free_periods().is_empty());

    if free.is_empty() {
        blocks.push(section("この日に空いているリソースはありません。"));
    } else {
        let lines: Vec<String> = free
            .iter()
            .map(|availability| {
                format!(
                    "✅ {} — {}",
                    format_resource(availability.resource()),
                    format_free_periods(availability.free_periods(), window, timezone)
                )
            })
            .collect();
        blocks.push(section(lines.join("\n")));
    }

    if !fully_booked.is_empty() {
        let names: Vec<String> = fully_booked
            .iter()
            .map(|availability| format_resource(availability.resource()))
            .collect();
        blocks.push(context(format!("この日は空きなし: {}", names.join(", "))));
    }

    blocks.push(day_actions(options));

    SlackMessageContent::new()
        .with_text(heading)
        .with_blocks(blocks)
}

/// 見出し
///
/// 今日を見ているときは、過ぎた時間を除いて表示していることを添える。
/// 断らないと、午後に見た「終日」が朝からの意味に読める。
fn heading(date: NaiveDate, today: NaiveDate) -> String {
    let day = super::days::heading_for(date, today);

    if date == today {
        return format!("{} の空き（いま以降）", day);
    }

    format!("{} の空き", day)
}

/// 空き時間帯を並べる
///
/// 対象の日の端に接する区間は、その側の時刻を省く。「0:00〜13:00」と書いても
/// 読み手が受け取るのは「13:00まで」という一点だけである。
fn format_free_periods(
    periods: &[TimePeriod],
    window: &TimePeriod,
    timezone: Option<&str>,
) -> String {
    if periods.len() == 1 && covers_the_whole_window(&periods[0], window) {
        return "終日".to_string();
    }

    periods
        .iter()
        .map(|period| format_period(period, window, timezone))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 対象の日の全体を覆っているか
fn covers_the_whole_window(period: &TimePeriod, window: &TimePeriod) -> bool {
    period.start() <= window.start() && period.end() >= window.end()
}

/// 1つの空き区間
fn format_period(period: &TimePeriod, window: &TimePeriod, timezone: Option<&str>) -> String {
    let starts_at_the_edge = period.start() <= window.start();
    let ends_at_the_edge = period.end() >= window.end();

    let start = to_zoned(period.start(), timezone)
        .format("%H:%M")
        .to_string();
    let end = to_zoned(period.end(), timezone).format("%H:%M").to_string();

    match (starts_at_the_edge, ends_at_the_edge) {
        (true, true) => "終日".to_string(),
        (true, false) => format!("〜{}", end),
        (false, true) => format!("{}〜", start),
        (false, false) => format!("{}〜{}", start, end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
    use crate::domain::common::EmailAddress;
    use crate::domain::services::resource_usage::availability::calculate;
    use chrono::{DateTime, TimeZone, Utc};

    const TOKYO: Option<&str> = Some("Asia/Tokyo");

    /// 日本時間のその日その時刻
    fn jst(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, hour, 0, 0).unwrap() - chrono::Duration::hours(9)
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
    }

    /// 8月3日を丸ごと見る（日本時間の0:00から翌0:00まで）
    fn whole_day() -> TimePeriod {
        TimePeriod::new(jst(3, 0), jst(4, 0)).unwrap()
    }

    fn gpu(device_number: u32) -> Resource {
        Resource::Gpu(Gpu::new(
            "gpu-server-1".to_string(),
            device_number,
            "A100 80GB PCIe".to_string(),
        ))
    }

    fn reservation(
        resources: Vec<Resource>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ResourceUsage {
        ResourceUsage::new(
            EmailAddress::new("sato@example.com".to_string()).unwrap(),
            TimePeriod::new(from, to).unwrap(),
            resources,
            None,
        )
        .unwrap()
    }

    fn availabilities(
        resources: &[Resource],
        usages: &[ResourceUsage],
        window: &TimePeriod,
    ) -> Vec<ResourceAvailability> {
        calculate(resources, window, usages)
    }

    /// メッセージに載っている文言をつないで取り出す
    fn rendered(content: &SlackMessageContent) -> String {
        content
            .blocks
            .iter()
            .flatten()
            .filter_map(|block| match block {
                SlackBlock::Section(section) => section.text.as_ref().map(|text| match text {
                    SlackBlockText::MarkDown(markdown) => markdown.text.clone(),
                    SlackBlockText::Plain(plain) => plain.text.clone(),
                }),
                SlackBlock::Context(context) => Some(
                    context
                        .elements
                        .iter()
                        .filter_map(|element| match element {
                            SlackContextBlockElement::MarkDown(markdown) => {
                                Some(markdown.text.clone())
                            }
                            SlackContextBlockElement::Plain(plain) => Some(plain.text.clone()),
                            SlackContextBlockElement::Image(_) => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_day_with_no_reservations_is_free_all_day() {
        let window = whole_day();
        let availabilities = availabilities(&[gpu(0)], &[], &window);

        let text = rendered(&build(
            &availabilities,
            &window,
            date(3),
            date(2),
            &[],
            TOKYO,
        ));

        assert!(text.contains("終日"), "{text}");
    }

    #[test]
    fn a_gap_touching_the_start_of_the_day_drops_its_opening_time() {
        let window = whole_day();
        let usages = vec![reservation(vec![gpu(0)], jst(3, 13), jst(3, 17))];
        let availabilities = availabilities(&[gpu(0)], &usages, &window);

        let text = rendered(&build(
            &availabilities,
            &window,
            date(3),
            date(2),
            &[],
            TOKYO,
        ));

        assert!(
            text.contains("〜13:00, 17:00〜"),
            "日の端に接する時刻は書かない: {text}"
        );
    }

    #[test]
    fn a_gap_in_the_middle_carries_both_times() {
        let window = whole_day();
        let usages = vec![
            reservation(vec![gpu(0)], jst(3, 0), jst(3, 9)),
            reservation(vec![gpu(0)], jst(3, 15), jst(4, 0)),
        ];
        let availabilities = availabilities(&[gpu(0)], &usages, &window);

        let text = rendered(&build(
            &availabilities,
            &window,
            date(3),
            date(2),
            &[],
            TOKYO,
        ));

        assert!(text.contains("09:00〜15:00"), "{text}");
    }

    #[test]
    fn resources_with_no_gap_are_named_at_the_end_instead_of_listed() {
        let window = whole_day();
        let usages = vec![reservation(vec![gpu(1)], jst(3, 0), jst(4, 0))];
        let availabilities = availabilities(&[gpu(0), gpu(1)], &usages, &window);

        let text = rendered(&build(
            &availabilities,
            &window,
            date(3),
            date(2),
            &[],
            TOKYO,
        ));

        assert!(text.contains("この日は空きなし"), "{text}");
        assert_eq!(
            text.matches("✅").count(),
            1,
            "空いているものだけを並べる: {text}"
        );
    }

    #[test]
    fn a_fully_booked_day_says_so() {
        let window = whole_day();
        let usages = vec![reservation(vec![gpu(0)], jst(3, 0), jst(4, 0))];
        let availabilities = availabilities(&[gpu(0)], &usages, &window);

        let text = rendered(&build(
            &availabilities,
            &window,
            date(3),
            date(2),
            &[],
            TOKYO,
        ));

        assert!(
            text.contains("この日に空いているリソースはありません"),
            "{text}"
        );
    }

    #[test]
    fn looking_at_today_says_the_past_is_left_out() {
        // 今日を14:00から見る
        let window = TimePeriod::new(jst(2, 14), jst(3, 0)).unwrap();
        let availabilities = availabilities(&[gpu(0)], &[], &window);

        let content = build(&availabilities, &window, date(2), date(2), &[], TOKYO);

        assert!(
            content.text.as_deref().unwrap().contains("いま以降"),
            "断らないと、午後に見た「終日」が朝からの意味に読める: {:?}",
            content.text
        );
    }
}
