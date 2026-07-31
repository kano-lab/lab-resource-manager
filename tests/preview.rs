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

/// 積む予約の指定（所有者, 開始, 終了, リソース, 備考）
///
/// 開始と終了は現在時刻からの時間数で表す。
type Sample = (&'static str, i64, i64, Vec<Resource>, Option<&'static str>);

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

fn room(name: &str) -> RoomConfig {
    RoomConfig {
        name: name.to_string(),
        calendar_id: format!("{}@example.com", name),
        notifications: vec![],
    }
}

#[tokio::test]
#[ignore = "画面を目で確かめるためのHTMLを書き出す"]
async fn write_preview_html() {
    let config = ResourceConfig {
        servers: vec![server("Thalys", "A100", 4), server("Freccia", "RTX6000", 2)],
        rooms: vec![room("会議室"), room("輪読室")],
    };

    let repository = Arc::new(MockUsageRepository::new());
    let now = Utc::now();

    let gpu = |server: &str, device: u32, model: &str| {
        Resource::Gpu(Gpu::new(server.to_string(), device, model.to_string()))
    };

    let reservations: Vec<Sample> = vec![
        // 進行中で、表示範囲の前から続いている
        (
            "kawaguchi",
            -30,
            18,
            vec![gpu("Thalys", 0, "A100"), gpu("Thalys", 1, "A100")],
            Some("事前学習"),
        ),
        ("sato", 2, 26, vec![gpu("Thalys", 2, "A100")], Some("蒸留")),
        ("suzuki", 30, 54, vec![gpu("Thalys", 3, "A100")], None),
        // 同じGPUで時間が重なる（レーンが分かれる）
        (
            "tanaka",
            6,
            40,
            vec![gpu("Freccia", 0, "RTX6000")],
            Some("評価"),
        ),
        (
            "takahashi",
            20,
            60,
            vec![gpu("Freccia", 0, "RTX6000")],
            Some("再現実験"),
        ),
        (
            "ito",
            50,
            120,
            vec![gpu("Freccia", 1, "RTX6000")],
            Some("長時間ジョブ"),
        ),
        (
            "watanabe",
            4,
            6,
            vec![Resource::Room {
                name: "会議室".to_string(),
            }],
            Some("ゼミ"),
        ),
        (
            "yamamoto",
            28,
            30,
            vec![Resource::Room {
                name: "会議室".to_string(),
            }],
            Some("研究会"),
        ),
        (
            "nakamura",
            52,
            55,
            vec![Resource::Room {
                name: "輪読室".to_string(),
            }],
            None,
        ),
        // 設定に無いサーバー
        (
            "obsolete",
            10,
            20,
            vec![gpu("Retired", 0, "K80")],
            Some("撤去済みサーバーの予約"),
        ),
    ];

    for (owner, from_hours, to_hours, resources, notes) in reservations {
        let usage = ResourceUsage::new(
            EmailAddress::new(format!("{}@example.com", owner)).unwrap(),
            TimePeriod::new(
                now + Duration::hours(from_hours),
                now + Duration::hours(to_hours),
            )
            .unwrap(),
            resources,
            notes.map(|n| n.to_string()),
        )
        .unwrap();

        repository.save(&usage).await.unwrap();
    }

    let query: Arc<dyn ReservationQuery> = Arc::new(UseCaseReservationQuery::new(Arc::new(
        ListAllFutureResourceUsagesUseCase::new(repository),
    )));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        web::serve_on(listener, query, Arc::new(config), chrono_tz::Asia::Tokyo)
            .await
            .unwrap();
    });

    let out = std::env::var("PREVIEW_OUT").expect("PREVIEW_OUTに出力先を指定してください");

    for days in [3, 7] {
        let body = reqwest::get(format!("http://{}/?days={}", addr, days))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        std::fs::write(format!("{}/timeline-{}days.html", out, days), body).unwrap();
    }
}
