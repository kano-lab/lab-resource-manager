//! Web画面をHTTPで叩く結合テスト
//!
//! ここで確かめたいのはHTMLの見た目ではなく、Topcoatの`.discover()`がライブラリ側に
//! 置いた`#[page]`をリンク時に拾えているかどうか。ページが登録されていなければ
//! 404が返るので、200と本文の内容が両方揃って初めて配線が通ったと言える。

#![cfg(feature = "web")]

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
use std::sync::Arc;

fn config() -> ResourceConfig {
    ResourceConfig {
        servers: vec![ServerConfig {
            name: "Thalys".to_string(),
            calendar_id: "thalys@example.com".to_string(),
            devices: vec![DeviceConfig {
                id: 0,
                model: "A100".to_string(),
            }],
            notifications: vec![],
        }],
        rooms: vec![RoomConfig {
            name: "会議室".to_string(),
            calendar_id: "room@example.com".to_string(),
            notifications: vec![],
        }],
    }
}

/// 予約を1件積んだWeb画面を起動し、その基準URLを返す
async fn start_server() -> String {
    let repository = Arc::new(MockUsageRepository::new());

    let now = Utc::now();
    let usage = ResourceUsage::new(
        EmailAddress::new("kawaguchi@example.com".to_string()).unwrap(),
        TimePeriod::new(now + Duration::hours(1), now + Duration::hours(5)).unwrap(),
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))],
        Some("実験メモ".to_string()),
    )
    .unwrap();
    repository.save(&usage).await.unwrap();

    let reservations: Arc<dyn ReservationQuery> = Arc::new(UseCaseReservationQuery::new(Arc::new(
        ListAllFutureResourceUsagesUseCase::new(repository),
    )));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        web::serve_on(
            listener,
            reservations,
            Arc::new(config()),
            chrono_tz::Asia::Tokyo,
        )
        .await
        .unwrap();
    });

    format!("http://{}", addr)
}

async fn get(url: &str) -> (reqwest::StatusCode, String) {
    let response = reqwest::get(url).await.expect("リクエストに失敗しました");
    let status = response.status();
    let body = response.text().await.unwrap();

    (status, body)
}

#[tokio::test]
async fn the_timeline_page_is_served_with_the_reservations_on_it() {
    let base = start_server().await;
    let (status, body) = get(&base).await;

    assert_eq!(status, reqwest::StatusCode::OK);

    // 設定したリソースが行として並ぶ
    assert!(body.contains("GPU#0 (A100)"), "GPUの行がない");
    assert!(body.contains("会議室"), "部屋の行がない");

    // 予約が所有者のローカルパートで描かれる
    assert!(body.contains("kawaguchi"), "予約が描かれていない");
    assert!(body.contains("実験メモ"), "備考が描かれていない");
}

#[tokio::test]
async fn the_address_of_the_owner_is_not_exposed_in_full() {
    let base = start_server().await;
    let (_, body) = get(&base).await;

    // 認証を求めない画面なので、メールアドレス全体は出さない
    assert!(
        !body.contains("kawaguchi@example.com"),
        "メールアドレス全体が露出している"
    );
}

#[tokio::test]
async fn the_generated_css_is_embedded_in_the_response() {
    let base = start_server().await;
    let (_, body) = get(&base).await;

    // Tailwindのビルド成果物がバイナリから直接埋まっている（別ファイルを取りに行かない）
    assert!(body.contains("<style>"), "スタイルが埋め込まれていない");
    assert!(
        !body.contains("stylesheet"),
        "外部スタイルシートを参照している"
    );
}

#[tokio::test]
async fn the_displayed_range_follows_the_days_parameter() {
    let base = start_server().await;

    let (status, one_day) = get(&format!("{}/?days=1", base)).await;
    assert_eq!(status, reqwest::StatusCode::OK);

    let (_, one_month) = get(&format!("{}/?days=30", base)).await;

    // 表示範囲の終わりが期間に応じて動く
    let range_of = |body: &str| {
        let marker = "text-sm text-slate-400\">";
        let from = body.find(marker).expect("表示範囲の見出しがない") + marker.len();
        body[from..from + 60].to_string()
    };

    assert_ne!(
        range_of(&one_day),
        range_of(&one_month),
        "表示期間を変えても表示範囲が変わっていない"
    );
}

#[tokio::test]
async fn conditional_classes_do_not_wipe_out_the_base_ones() {
    // 条件付きのクラスを`class="..."`の二度書きで表すと後の指定が前を打ち消し、
    // 絶対配置の土台ごと失われてレイアウトが崩れる。`class!`で組み立てている限り
    // 基本のクラスは残る。
    let base = start_server().await;
    let (_, body) = get(&base).await;

    assert!(
        body.contains("absolute inset-y-0 w-px bg-"),
        "目盛りの縦線が絶対配置のクラスを失っている"
    );
    assert!(
        body.contains("rounded px-3 py-1.5 text-sm transition"),
        "期間切り替えのリンクが基本のクラスを失っている"
    );
}

#[tokio::test]
async fn an_out_of_range_days_parameter_does_not_break_the_page() {
    let base = start_server().await;

    // 上限を超える値や負の値でも、丸めて描画する
    for query in ["?days=100000", "?days=-5", "?days=0"] {
        let (status, body) = get(&format!("{}/{}", base, query)).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{}で失敗", query);
        assert!(body.contains("GPU#0 (A100)"), "{}で行が消えた", query);
    }
}
