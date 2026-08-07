//! タイムラインの描画
//!
//! 位置は表示範囲に対する割合で渡ってくるので、`left`と`width`をパーセントで置く。
//! 時間は連続量なのでグリッドの列に丸めず、絶対配置で表す。
//!
//! 条件によってクラスを足すときは`class!`を使う。`class="..."`を二度書くと
//! 後の指定が前を打ち消してしまい、レイアウトの土台ごと失われる。

use crate::interface::web::timeline::{Tick, Timeline, TimelineBlock, TimelineRow};
use topcoat::{
    Result,
    view::{class, component, view},
};

/// 表示期間の選択肢
const RANGES: [(i64, &str); 4] = [(1, "1日"), (3, "3日"), (7, "1週間"), (30, "1か月")];

/// 1レーンの高さ（rem）
const LANE_HEIGHT: f64 = 1.75;

/// レーンの外側に取る余白（rem）。行の上下に1つずつ入る
const LANE_MARGIN: f64 = 0.25;

/// 予約ブロックの色
///
/// 所有者ごとに色相を割り当てる。誰がどれだけ押さえているかが、名前を読まなくても
/// 並びから掴める。Tailwindはクラス名をビルド時に走査するため動的な色を作れないので、
/// ここは`style`属性で直接指定する。
fn owner_color(owner: &str) -> String {
    let hue = owner.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    }) % 360;

    format!("background-color: hsl({hue} 55% 32%); border-color: hsl({hue} 60% 48%);")
}

/// 予約を読み込めなかったときの表示
#[component]
pub(crate) async fn failure(message: &str) -> Result {
    view! {
        <div class="min-h-screen bg-slate-950 px-6 py-16 text-slate-100">
            <div class="mx-auto max-w-2xl rounded-lg border border-rose-900 bg-rose-950/40 p-6">
                <h1 class="text-lg font-semibold">"予約を読み込めませんでした"</h1>
                <p class="mt-2 text-sm text-slate-300">(message)</p>
            </div>
        </div>
    }
}

#[component]
pub(crate) async fn timeline_page(timeline: &Timeline, days: i64) -> Result {
    view! {
        <div class="min-h-screen bg-slate-950 text-slate-100">
            header(timeline: timeline, days: days)
            <main class="px-6 pb-12">
                if timeline.rows.is_empty() {
                    <p class="py-16 text-center text-slate-400">
                        "予約できるリソースが設定されていません。"
                    </p>
                } else {
                    grid(timeline: timeline)
                }
            </main>
        </div>
    }
}

#[component]
async fn header(timeline: &Timeline, days: i64) -> Result {
    view! {
        <header class="px-6 pt-6 pb-4">
            <div class="flex flex-wrap items-baseline justify-between gap-4">
                <div>
                    <h1 class="text-xl font-semibold">"予約タイムライン"</h1>
                    <p class="mt-1 text-sm text-slate-400">
                        (timeline.start.format("%Y/%-m/%-d %H:%M").to_string())
                        " 〜 "
                        (timeline.end.format("%Y/%-m/%-d %H:%M").to_string())
                    </p>
                </div>
                <nav class="flex gap-1">
                    for (value, label) in RANGES {
                        <a
                            href=(format!("/?days={}", value))
                            class=(class!(
                                "rounded px-3 py-1.5 text-sm transition",
                                "bg-slate-100 text-slate-900 font-medium" if value == days,
                                "bg-slate-800 text-slate-300 hover:bg-slate-700" if value != days,
                            ))
                            if value == days {
                                aria-current="page"
                            }
                        >
                            (label)
                        </a>
                    }
                </nav>
            </div>
        </header>
    }
}

#[component]
async fn grid(timeline: &Timeline) -> Result {
    view! {
        <div class="overflow-x-auto rounded-lg border border-slate-800">
            <div class="min-w-[64rem]">
                axis(timeline: timeline)
                <div class="relative">
                    now_marker(timeline: timeline)
                    for (index, row) in timeline.rows.iter().enumerate() {
                        timeline_row(row: row, ticks: &timeline.ticks, striped: index % 2 == 1)
                    }
                </div>
            </div>
        </div>
    }
}

/// 時間軸の目盛り
#[component]
async fn axis(timeline: &Timeline) -> Result {
    view! {
        <div class="flex border-b border-slate-800 bg-slate-900">
            <div class="w-48 shrink-0 border-r border-slate-800 px-3 py-2 text-xs font-medium text-slate-400">
                "リソース"
            </div>
            <div class="relative h-9 grow">
                for tick in &timeline.ticks {
                    <div
                        class=(class!(
                            "absolute top-0 h-full -translate-x-1/2 whitespace-nowrap px-1 pt-2 text-xs",
                            "font-medium text-slate-200" if tick.is_day_boundary,
                            "text-slate-500" if !tick.is_day_boundary,
                        ))
                        style=(format!("left: {:.4}%", tick.ratio * 100.0))
                    >
                        (&tick.label)
                    </div>
                }
            </div>
        </div>
    }
}

/// 現在時刻の縦線
///
/// 行の背景より手前、予約ブロックより奥に置く。
#[component]
async fn now_marker(timeline: &Timeline) -> Result {
    view! {
        if let Some(ratio) = timeline.now_ratio {
            <div class="pointer-events-none absolute inset-y-0 left-48 right-0 z-10">
                <div
                    class="absolute inset-y-0 w-px bg-rose-500"
                    style=(format!("left: {:.4}%", ratio * 100.0))
                ></div>
            </div>
        }
    }
}

#[component]
async fn timeline_row(row: &TimelineRow, ticks: &[Tick], striped: bool) -> Result {
    let lane_count = row.lanes.len().max(1);
    let height = format!(
        "min-height: {}rem",
        lane_count as f64 * LANE_HEIGHT + LANE_MARGIN * 2.0
    );

    view! {
        <div class=(class!(
            "flex border-b border-slate-800/60 last:border-b-0",
            "bg-slate-900/40" if striped,
        ))>
            <div class="w-48 shrink-0 border-r border-slate-800 px-3 py-2">
                <div class="text-sm text-slate-200">(&row.label)</div>
                <div class="text-xs text-slate-500">(&row.group)</div>
            </div>
            <div class="relative grow" style=(height)>
                gridlines(ticks: ticks)
                for (lane_index, lane) in row.lanes.iter().enumerate() {
                    for block in lane {
                        reservation(block: block, lane: lane_index)
                    }
                }
            </div>
        </div>
    }
}

/// 目盛りに合わせた縦の罫線
#[component]
async fn gridlines(ticks: &[Tick]) -> Result {
    view! {
        <div class="pointer-events-none absolute inset-0">
            for tick in ticks {
                <div
                    class=(class!(
                        "absolute inset-y-0 w-px",
                        "bg-slate-700" if tick.is_day_boundary,
                        "bg-slate-800/70" if !tick.is_day_boundary,
                    ))
                    style=(format!("left: {:.4}%", tick.ratio * 100.0))
                ></div>
            }
        </div>
    }
}

#[component]
async fn reservation(block: &TimelineBlock, lane: usize) -> Result {
    let style = format!(
        "left: {:.4}%; width: {:.4}%; top: {}rem; {}",
        block.start_ratio * 100.0,
        block.width_ratio() * 100.0,
        lane as f64 * LANE_HEIGHT + LANE_MARGIN,
        owner_color(&block.owner)
    );

    let tooltip = format!(
        "{} 〜 {}\n{}{}",
        block.start.format("%-m/%-d %H:%M"),
        block.end.format("%-m/%-d %H:%M"),
        block.owner,
        block
            .notes
            .as_ref()
            .map(|notes| format!("\n{}", notes))
            .unwrap_or_default()
    );

    view! {
        <div
            class=(class!(
                "absolute z-20 flex h-6 items-center overflow-hidden rounded border px-1.5 text-xs text-slate-50",
                // 表示範囲の外へ続いている側は、角と枠線を落として切れていることを示す
                "rounded-l-none border-l-0" if block.clipped_start,
                "rounded-r-none border-r-0" if block.clipped_end,
            ))
            style=(style)
            title=(tooltip)
        >
            <span class="truncate font-medium">(&block.owner)</span>
            if let Some(notes) = &block.notes {
                <span class="ml-1.5 truncate text-slate-300">(notes)</span>
            }
        </div>
    }
}
