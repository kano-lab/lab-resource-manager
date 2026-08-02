//! タイムライン画面の静的なデモを書き出す
//!
//! 架空の予約を積んだ画面を起動し、期間ごとのHTMLを取得して保存する。生成物は
//! JavaScriptも外部リソースも含まない自己完結したHTMLなので、GitHub Pagesのような
//! 静的ホスティングにそのまま置ける。
//!
//! 開発中に画面を目で確かめたいときにも使える。
//!
//! ```bash
//! cargo run --example demo_page --features web -- dist
//! ```

use chrono::{Duration, Utc};
use lab_resource_manager::application::usecases::list_all_future_resource_usages::ListAllFutureResourceUsagesUseCase;
use lab_resource_manager::domain::aggregates::resource_usage::entity::ResourceUsage;
use lab_resource_manager::domain::aggregates::resource_usage::value_objects::{
    Gpu, Resource, TimePeriod,
};
use lab_resource_manager::domain::common::EmailAddress;
use lab_resource_manager::domain::ports::repositories::ResourceUsageRepository;
use lab_resource_manager::infrastructure::config::{
    DeviceConfig, ResourceConfig, RoomConfig, ServerConfig,
};
use lab_resource_manager::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
use lab_resource_manager::interface::web::{
    self,
    query::{ReservationQuery, UseCaseReservationQuery},
};
use std::path::Path;
use std::sync::Arc;

/// 書き出す表示期間。画面の期間切り替えが持つ選択肢と揃える
const DAYS: [i64; 4] = [1, 3, 7, 30];

/// 入口として複製する期間
const LANDING_DAYS: i64 = 7;

/// 積む予約の指定（所有者, 開始, 終了, リソース, 備考）
///
/// 開始と終了は現在時刻からの時間数で表す。所有者は実在の人物と読み違えられないよう
/// 英字の定番名を使う。
type Sample = (&'static str, i64, i64, Vec<Resource>, Option<&'static str>);

fn gpu(server: &str, device: u32, model: &str) -> Resource {
    Resource::Gpu(Gpu::new(server.to_string(), device, model.to_string()))
}

fn room(name: &str) -> Resource {
    Resource::Room {
        name: name.to_string(),
    }
}

fn server(name: &str, model: &str, count: u32) -> ServerConfig {
    ServerConfig {
        name: name.to_string(),
        calendar_id: format!("{}@example.com", name.to_lowercase()),
        devices: (0..count)
            .map(|id| DeviceConfig {
                id,
                model: model.to_string(),
            })
            .collect(),
        notifications: vec![],
    }
}

fn demo_config() -> ResourceConfig {
    ResourceConfig {
        servers: vec![
            server("Aurora", "A100", 4),
            server("Borealis", "RTX6000", 2),
        ],
        rooms: vec![
            RoomConfig {
                name: "会議室".to_string(),
                calendar_id: "meeting@example.com".to_string(),
                notifications: vec![],
            },
            RoomConfig {
                name: "輪読室".to_string(),
                calendar_id: "seminar@example.com".to_string(),
                notifications: vec![],
            },
        ],
    }
}

/// 画面の見どころが一通り出るように予約を選んである
fn demo_reservations() -> Vec<Sample> {
    vec![
        // 表示範囲の前から続く、進行中の予約
        (
            "alice",
            -30,
            18,
            vec![gpu("Aurora", 0, "A100"), gpu("Aurora", 1, "A100")],
            Some("事前学習"),
        ),
        ("bob", 2, 26, vec![gpu("Aurora", 2, "A100")], Some("蒸留")),
        ("carol", 30, 54, vec![gpu("Aurora", 3, "A100")], None),
        // 同じGPUで時間が重なる二件。上下に段が分かれる
        (
            "dave",
            6,
            40,
            vec![gpu("Borealis", 0, "RTX6000")],
            Some("評価"),
        ),
        (
            "erin",
            20,
            60,
            vec![gpu("Borealis", 0, "RTX6000")],
            Some("再現実験"),
        ),
        (
            "frank",
            50,
            120,
            vec![gpu("Borealis", 1, "RTX6000")],
            Some("長時間ジョブ"),
        ),
        ("grace", 4, 6, vec![room("会議室")], Some("ゼミ")),
        ("heidi", 28, 30, vec![room("会議室")], Some("研究会")),
        ("ivan", 52, 55, vec![room("輪読室")], None),
        // 設定から外れたサーバーの予約。末尾にまとめて出る
        (
            "judy",
            10,
            20,
            vec![gpu("Retired", 0, "K80")],
            Some("撤去済みサーバーの予約"),
        ),
    ]
}

/// 架空のデータであることを画面の先頭で断る
///
/// Tailwindはビルド時にクラス名を走査するため、生成済みのCSSに無いクラスは効かない。
/// ここは後から挿し込む断り書きなので、素のスタイル指定で完結させる。
const NOTICE: &str = concat!(
    r#"<div style="background:#1e293b;border-bottom:1px solid #334155;color:#cbd5e1;"#,
    r#"font:14px/1.6 system-ui,sans-serif;padding:10px 24px;text-align:center">"#,
    "これは lab-resource-manager の予約タイムラインのデモです。",
    "表示されている予約・利用者・リソースはすべて架空のものです。",
    r#" <a href="https://github.com/kano-lab/lab-resource-manager""#,
    r#" style="color:#93c5fd">リポジトリ</a></div>"#,
);

/// 静的ホスティング向けにHTMLを整える
///
/// 画面が出すリンクはサーバー上の絶対パス（`/?days=7`）で、配信先が
/// ドメイン直下とは限らない静的ホスティングでは行き先を見失う。
/// 期間ごとのファイル名へ置き換える。
fn to_static_page(html: &str) -> String {
    let mut page = html.to_string();

    for days in DAYS {
        // 属性ごと置き換える。`/?days=3`だけを狙うと`/?days=30`の先頭にも当たり、
        // 30日表示へのリンクが`3days.html0`に化ける
        page = page.replace(
            &format!(r#"href="/?days={}""#, days),
            &format!(r#"href="{}""#, page_name(days)),
        );
    }

    let page = page.replace(
        r#"<body class="bg-slate-950">"#,
        &format!(r#"<body class="bg-slate-950">{}"#, NOTICE),
    );

    // 行き先を見失うリンクを配信してしまわないための歯止め
    assert!(
        !page.contains(r#"href="/?"#),
        "サーバー上の絶対パスを指すリンクが残っている"
    );

    page
}

fn page_name(days: i64) -> String {
    format!("{}days.html", days)
}

/// 架空の予約を積んだ画面を起動し、基準URLを返す
async fn start_demo_server() -> String {
    let repository = Arc::new(MockUsageRepository::new());
    let now = Utc::now();

    for (owner, from_hours, to_hours, resources, notes) in demo_reservations() {
        let usage = ResourceUsage::new(
            EmailAddress::new(format!("{}@example.com", owner)).expect("メールアドレスの形式"),
            TimePeriod::new(
                now + Duration::hours(from_hours),
                now + Duration::hours(to_hours),
            )
            .expect("開始は終了より前"),
            resources,
            notes.map(str::to_string),
        )
        .expect("リソースが1つ以上ある");

        repository
            .save(&usage)
            .await
            .expect("モックの保存は失敗しない");
    }

    let query: Arc<dyn ReservationQuery> = Arc::new(UseCaseReservationQuery::new(Arc::new(
        ListAllFutureResourceUsagesUseCase::new(repository),
    )));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("空きポートに束縛できる");
    let addr = listener.local_addr().expect("束縛したアドレスを取得できる");

    tokio::spawn(async move {
        web::serve_on(
            listener,
            query,
            Arc::new(demo_config()),
            chrono_tz::Asia::Tokyo,
        )
        .await
        .expect("デモサーバーが起動する");
    });

    format!("http://{}", addr)
}

#[tokio::main]
async fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dist".to_string());
    let out = Path::new(&out);

    std::fs::create_dir_all(out).expect("出力先を作成できる");

    let base = start_demo_server().await;
    let mut landing = None;

    for days in DAYS {
        let html = reqwest::get(format!("{}/?days={}", base, days))
            .await
            .expect("デモサーバーに接続できる")
            .text()
            .await
            .expect("本文を読める");

        let page = to_static_page(&html);

        if days == LANDING_DAYS {
            landing = Some(page.clone());
        }

        let path = out.join(page_name(days));
        std::fs::write(&path, page).expect("HTMLを書き出せる");
        println!("{}", path.display());
    }

    // 入口。期間切り替えのリンクは相対パスなので、複製しても行き先は変わらない
    let index = out.join("index.html");
    std::fs::write(&index, landing.expect("入口にする期間が書き出されている"))
        .expect("index.htmlを書き出せる");
    println!("{}", index.display());
}
