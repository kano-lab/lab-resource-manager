//! タイムライン画面のルート
//!
//! 画面は1枚だけなのでレイアウトを分けず、ここでHTML全体を組み立てる。
//! TailwindのCSSはビルドスクリプトが生成したものを埋め込む（`build.rs`を参照）。

use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::infrastructure::config::ResourceConfig;
use crate::interface::web::query::ReservationQuery;
use crate::interface::web::timeline;
use crate::interface::web::view::{failure, timeline_page};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use std::sync::Arc;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{page, query_params},
    view::view,
};

/// 画面の表示タイムゾーン
///
/// `app_context`はRustの型を鍵にして値を引くため、`Tz`をそのまま登録すると
/// 他の用途で登録された`Tz`と衝突する。専用の型で包んで区別する。
#[derive(Debug, Clone, Copy)]
pub struct DisplayTimezone(pub Tz);

/// 既定の表示日数
const DEFAULT_DAYS: i64 = 7;

/// 表示日数の上限
///
/// 繰り返し予約は個々の予定へ展開されるため、際限なく先まで引くと
/// カレンダーが展開できる限りの予定を取りに行ってしまう。
const MAX_DAYS: i64 = 60;

#[query_params(error = bad_request)]
struct TimelineQuery {
    days: Option<i64>,
}

fn reservations(cx: &Cx) -> &Arc<dyn ReservationQuery> {
    app_context::<Arc<dyn ReservationQuery>>(cx)
}

fn resource_config(cx: &Cx) -> &Arc<ResourceConfig> {
    app_context::<Arc<ResourceConfig>>(cx)
}

fn timezone(cx: &Cx) -> Tz {
    app_context::<DisplayTimezone>(cx).0
}

#[page("/")]
async fn index(cx: &Cx) -> Result {
    let params = query_params::<TimelineQuery>(cx)?;
    let days = params.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);

    let tz = timezone(cx);
    let now = Utc::now();
    let window = display_window(now, days, tz);

    // カレンダーへの問い合わせが失敗しても画面ごと落とさない。
    // 何も見えないより、何が起きているか分かる方がよい。
    let (timeline, error) = match reservations(cx).list_all(&window).await {
        Ok(usages) => (
            Some(timeline::build(
                resource_config(cx),
                &usages,
                &window,
                now,
                tz,
            )),
            None,
        ),
        Err(error) => (None, Some(error.to_string())),
    };

    view! {
        <!DOCTYPE html>
        <html lang="ja">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"予約タイムライン - lab-resource-manager"</title>
                <style>(include_str!(concat!(env!("OUT_DIR"), "/tailwind.css")))</style>
            </head>
            <body class="bg-slate-950">
                if let Some(timeline) = &timeline {
                    timeline_page(timeline: timeline, days: days)
                } else {
                    failure(message: error.as_deref().unwrap_or("原因不明のエラーです"))
                }
            </body>
        </html>
    }
}

/// 表示範囲は表示タイムゾーンでの当日の始まりから`days`日分
///
/// 日の変わり目に合わせておくと目盛りが揃い、進行中の予約も視野に入る。
/// 一定の時間を足すのではなく現地の日付で数える。夏時間で一日の長さが変わる日を
/// またぐと、72時間が3日ぶんにならないため。
///
/// 実在する日には必ず始まりの時刻があるため、以下のフォールバックは
/// タイムゾーンのデータが壊れているときの保険にすぎない。画面を落とさないことを優先し、
/// 多少ずれた範囲を出す方を選んでいる。
fn display_window(now: DateTime<Utc>, days: i64, tz: Tz) -> TimePeriod {
    let today = now.with_timezone(&tz).date_naive();

    let start = timeline::day_start(today, tz)
        .map(|at| at.with_timezone(&Utc))
        .unwrap_or(now);

    let end = today
        .checked_add_signed(Duration::days(days))
        .and_then(|date| timeline::day_start(date, tz))
        .map(|at| at.with_timezone(&Utc))
        .unwrap_or(start + Duration::days(days));

    // 日数は1以上に丸めてあるので、開始と終了が同じ時刻になることはない
    TimePeriod::new(start, end).expect("表示範囲の開始は終了より前になる")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// 現地時刻の表記に直す
    fn local(at: DateTime<Utc>, tz: Tz) -> String {
        at.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string()
    }

    #[test]
    fn the_window_starts_at_the_beginning_of_today() {
        let tokyo = chrono_tz::Asia::Tokyo;
        let now = tokyo
            .with_ymd_and_hms(2026, 8, 3, 14, 30, 0)
            .unwrap()
            .with_timezone(&Utc);

        let window = display_window(now, 7, tokyo);

        assert_eq!(local(window.start(), tokyo), "2026-08-03 00:00");
        assert_eq!(local(window.end(), tokyo), "2026-08-10 00:00");
    }

    #[test]
    fn the_window_spans_whole_local_days_across_a_clock_shift() {
        // 夏時間の始まる日は現地の一日が23時間しかない。一定時間を足す数え方だと
        // 「3日ぶん」が現地の3日と1時間になってしまう。
        let berlin = chrono_tz::Europe::Berlin;
        let now = berlin
            .with_ymd_and_hms(2026, 3, 28, 12, 0, 0)
            .unwrap()
            .with_timezone(&Utc);

        let window = display_window(now, 3, berlin);

        assert_eq!(local(window.start(), berlin), "2026-03-28 00:00");
        assert_eq!(local(window.end(), berlin), "2026-03-31 00:00");
        assert_eq!((window.end() - window.start()).num_hours(), 71);
    }
}
