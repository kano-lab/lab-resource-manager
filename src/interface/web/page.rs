//! タイムライン画面のルート
//!
//! 画面は1枚だけなのでレイアウトを分けず、ここでHTML全体を組み立てる。
//! TailwindのCSSはビルドスクリプトが生成したものを埋め込む（`build.rs`を参照）。

use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::infrastructure::config::ResourceConfig;
use crate::interface::web::query::ReservationQuery;
use crate::interface::web::timeline;
use crate::interface::web::view::{failure, timeline_page};
use chrono::{DateTime, Duration, TimeZone, Utc};
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

/// 表示範囲は表示タイムゾーンでの当日0時から`days`日間
///
/// 日の変わり目に合わせておくと目盛りが揃い、進行中の予約も視野に入る。
fn display_window(now: DateTime<Utc>, days: i64, tz: Tz) -> TimePeriod {
    let local_now = now.with_timezone(&tz);
    let midnight = local_now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| tz.from_local_datetime(&naive).earliest())
        .unwrap_or(local_now);

    let start = midnight.with_timezone(&Utc);
    // 日数は1以上に丸めてあるので、開始と終了が同じ時刻になることはない
    TimePeriod::new(start, start + Duration::days(days)).expect("表示範囲の開始は終了より前になる")
}
