//! 表示する日の選択肢
//!
//! 空き状況メッセージの下に置く「今日」「明日」ボタンと、
//! それ以外の日を選ぶセレクトメニューの中身を組み立てます。
//!
//! 選べる範囲は先読みする長さと同じです。データのない日を選ばせないため、
//! 選択肢の側で範囲を示します。

use chrono::{Datelike, NaiveDate, Weekday};

/// 表示する日の選択肢
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayOption {
    /// 対象の日（表示タイムゾーンでの暦日）
    pub date: NaiveDate,
    /// ボタンやメニューに出す文言
    pub label: String,
}

/// ボタンとして先に出す日数（今日と明日）
const BUTTON_DAYS: usize = 2;

/// 今日から数えて指定日数分の選択肢を作る
///
/// # 引数
/// * `today` - 表示タイムゾーンでの今日
/// * `days` - 今日を含めて何日ぶん作るか
pub fn options_from(today: NaiveDate, days: usize) -> Vec<DayOption> {
    (0..days)
        .filter_map(|offset| {
            today
                .checked_add_signed(chrono::Duration::days(offset as i64))
                .map(|date| DayOption {
                    date,
                    label: label_for(date, today),
                })
        })
        .collect()
}

/// ボタンにする分だけを取り出す
pub fn buttons(options: &[DayOption]) -> &[DayOption] {
    &options[..BUTTON_DAYS.min(options.len())]
}

/// セレクトメニューに回す分だけを取り出す
pub fn menu_items(options: &[DayOption]) -> &[DayOption] {
    if options.len() <= BUTTON_DAYS {
        return &[];
    }
    &options[BUTTON_DAYS..]
}

/// その日を表す文言
///
/// 近い日は「今日」「明日」と呼び、それ以降は日付と曜日で示す。
/// 「4日後」のような言い方は読み手に数え直しをさせる。
pub fn label_for(date: NaiveDate, today: NaiveDate) -> String {
    match date.signed_duration_since(today).num_days() {
        0 => "今日".to_string(),
        1 => "明日".to_string(),
        2 => "明後日".to_string(),
        _ => format!(
            "{}/{}（{}）",
            date.month(),
            date.day(),
            weekday_in_japanese(date.weekday())
        ),
    }
}

/// 見出しに使う日付（曜日つき）
pub fn heading_for(date: NaiveDate, today: NaiveDate) -> String {
    let day = format!(
        "{}月{}日（{}）",
        date.month(),
        date.day(),
        weekday_in_japanese(date.weekday())
    );

    match date.signed_duration_since(today).num_days() {
        0 => format!("{} 今日", day),
        1 => format!("{} 明日", day),
        _ => day,
    }
}

/// 曜日の日本語表記
fn weekday_in_japanese(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "月",
        Weekday::Tue => "火",
        Weekday::Wed => "水",
        Weekday::Thu => "木",
        Weekday::Fri => "金",
        Weekday::Sat => "土",
        Weekday::Sun => "日",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026年8月2日は日曜日
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 2).unwrap()
    }

    #[test]
    fn the_options_start_from_today() {
        let options = options_from(today(), 7);

        assert_eq!(options.len(), 7);
        assert_eq!(options[0].date, today());
        assert_eq!(options[0].label, "今日");
        assert_eq!(options[1].label, "明日");
    }

    #[test]
    fn days_further_out_carry_their_date_and_weekday() {
        let options = options_from(today(), 7);

        assert_eq!(
            options[3].label, "8/5（水）",
            "「3日後」では読み手が数え直すことになる"
        );
    }

    #[test]
    fn today_and_tomorrow_become_buttons_and_the_rest_go_to_the_menu() {
        let options = options_from(today(), 7);

        let buttons = buttons(&options);
        let menu = menu_items(&options);

        assert_eq!(buttons.len(), 2, "よく使う2日は1手で押せるようにする");
        assert_eq!(menu.len(), 5);
        assert_eq!(menu[0].label, "明後日");
    }

    #[test]
    fn a_short_lookahead_leaves_the_menu_empty() {
        let options = options_from(today(), 2);

        assert_eq!(buttons(&options).len(), 2);
        assert!(
            menu_items(&options).is_empty(),
            "選ぶものがないメニューは出さない"
        );
    }

    #[test]
    fn the_heading_names_the_day_and_how_near_it_is() {
        assert_eq!(heading_for(today(), today()), "8月2日（日） 今日");
        assert_eq!(
            heading_for(NaiveDate::from_ymd_opt(2026, 8, 5).unwrap(), today()),
            "8月5日（水）",
            "遠い日は日付だけで足りる"
        );
    }
}
