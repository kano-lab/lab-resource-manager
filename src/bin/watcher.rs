use clap::Parser;
use lab_resource_manager::*;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "watcher")]
#[command(about = "Resource usage watcher service", long_about = None)]
struct Args {
    #[arg(
        long,
        default_value = "google_calendar",
        help = "Repository implementation: google_calendar or mock"
    )]
    repository: String,

    #[arg(long, default_value = "60", help = "Polling interval in seconds")]
    interval: u64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    // .envファイルを読み込み、そのパスからプロジェクトルートを特定
    let dotenv_path = dotenv::dotenv().ok();
    let project_root = dotenv_path
        .as_ref()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().expect("❌ カレントディレクトリの取得に失敗"));

    println!("🚀 カレンダー監視サービスを起動します");
    println!("📋 Repository: {}", args.repository);
    println!("📋 Interval: {}秒", args.interval);

    let config_path = std::env::var("CONFIG_PATH").expect("❌ CONFIG_PATH must be set");
    let absolute_config_path = project_root.join(&config_path);
    let config = load_config(absolute_config_path.to_str().expect("❌ パスの変換に失敗"))
        .expect("❌ 設定ファイルの読み込みに失敗");

    match args.repository.as_str() {
        "google_calendar" => {
            let service_account_key_path = std::env::var("GOOGLE_SERVICE_ACCOUNT_KEY")
                .expect("❌ GOOGLE_SERVICE_ACCOUNT_KEY must be set");
            let absolute_key_path = project_root.join(&service_account_key_path);

            let repository = GoogleCalendarUsageRepository::new(
                absolute_key_path.to_str().expect("❌ パスの変換に失敗"),
                config.clone(),
            )
            .await
            .expect("❌ Google Calendar接続に失敗");

            let notifier = NotificationRouter::new(config);
            run_watcher(repository, notifier, args.interval).await;
        }
        "mock" => {
            let repository = MockUsageRepository::new();
            let notifier = NotificationRouter::new(config);
            run_watcher(repository, notifier, args.interval).await;
        }
        _ => {
            eprintln!("❌ Invalid repository: {}", args.repository);
            eprintln!("Valid values:");
            eprintln!("  --repository: google_calendar, mock");
            std::process::exit(1);
        }
    }
}

async fn run_watcher<R, N>(repository: R, notifier: N, interval_secs: u64)
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    let usecase = NotifyResourceUsageChangesUseCase::new(repository, notifier);
    let interval = Duration::from_secs(interval_secs);

    println!("🔍 カレンダー監視を開始します（間隔: {:?}）", interval);

    loop {
        match usecase.poll_once().await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("❌ ポーリングエラー: {}", e);
            }
        }

        tokio::time::sleep(interval).await;
    }
}
