# lab-resource-manager

研究室のGPU・部屋などの資源の使用状況を管理し、変更を通知するシステム。

## ドキュメント

- **[管理者ガイド](docs/ADMIN_GUIDE_ja.md)** - 研究室管理者向けのセットアップと導入手順
- **[ユーザーガイド](docs/USER_GUIDE_ja.md)** - システムの使い方（エンドユーザー向け）

## Features

- **リソース使用状況の管理**: GPUサーバーと部屋の使用予定を管理（デフォルト実装: Google Calendar）
- **Slackボットによるアクセス制御**: Slackコマンドでユーザーがメールアドレスを登録し、リソースへのアクセスを取得
  - response_urlによるDMとプライベートチャンネルのサポート
  - 通知での自動ユーザーメンション
- **Identity Linking（ID連携）**: メールアドレスとチャットユーザーIDをマッピングして通知を強化
- **マルチ通知先対応**: リソースごとに異なる通知先を設定可能（デフォルト実装: Slack、Mock）
- **柔軟なデバイス指定**: `0-2,5,7-9` 形式での複数デバイス指定に対応
- **クリーンアーキテクチャ**: DDD + ヘキサゴナルアーキテクチャ with Shared Kernelパターンで設計
- **拡張可能な設計**: リポジトリ、通知先、アクセス制御サービスをポートとして抽象化

## Architecture

このプロジェクトはClean Architectureの原則に従っています:

```text
src/
├── domain/                  # ドメイン層（ビジネスロジック）
│   ├── aggregates/          # 集約（ResourceUsage, IdentityLink）
│   ├── common/              # Shared Kernel（EmailAddress等）
│   ├── ports/               # ポート（Repository, Notifier, ResourceCollectionAccessトレイト）
│   └── errors.rs            # ドメインエラー
├── application/             # アプリケーション層（ユースケース）
│   └── usecases/            # 通知、アクセス許可ユースケース
├── infrastructure/          # インフラ層（外部システム統合）
│   ├── repositories/        # リポジトリ実装（Google Calendar、JSONファイル等）
│   │   ├── resource_usage/  # ResourceUsageリポジトリ実装
│   │   └── identity_link/   # IdentityLinkリポジトリ実装
│   ├── notifier/            # 通知実装（Slack、Mock等）
│   ├── resource_collection_access/ # アクセス制御サービス実装（Google Calendar等）
│   └── config/              # 設定管理
├── interface/               # インターフェース層（アダプター）
│   └── slack/               # Slackボット（Socket Mode + コマンドハンドラー）
└── bin/                     # エントリポイント
    ├── watcher.rs           # リソース使用状況監視
    └── slackbot.rs          # リソースアクセス管理用Slackボット
```

## クイックスタート

セットアップと導入の詳細については、[管理者ガイド](docs/ADMIN_GUIDE_ja.md)を参照してください。

システムの使用方法については、[ユーザーガイド](docs/USER_GUIDE_ja.md)を参照してください。

### ライブラリとして使用

`Cargo.toml`に追加:

```toml
[dependencies]
lab-resource-manager = "0.1"
```

使用例（Google Calendar実装を使用）:

```rust
use lab_resource_manager::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 設定の読み込み
    let config = load_config("config/resources.toml")?;

    // リポジトリ実装の作成（ここではGoogle Calendar実装を使用）
    let repository = GoogleCalendarUsageRepository::new(
        "secrets/service-account.json",
        config.clone(),
    ).await?;

    // 通知ルーターの作成（設定された全ての通知実装を自動処理）
    let notifier = NotificationRouter::new(config);

    // ユースケースの作成と実行
    let usecase = NotifyResourceUsageChangesUseCase::new(repository, notifier).await?;
    usecase.poll_once().await?;

    Ok(())
}
```

詳細な使用方法は [examples/](examples/) を参照してください。

## Development

### テスト実行

```bash
# 全テスト
cargo test

# 特定のモジュール
cargo test resource_factory
```

### ビルド

```bash
# 開発ビルド
cargo build

# リリースビルド
cargo build --release
```

### コード検査

```bash
cargo check
cargo clippy
cargo fmt
```

## Project Status

### 実装済み ✅

- [x] リソースベースの通知ルーティング
- [x] Identity Linking（チャットユーザーとの紐づけ）
- [x] Slackボットによるリソースコレクションへのアクセス制御

### Roadmap

- [ ] Slackコマンドでのリソース使用予約作成
- [ ] 自然言語でのリソース管理（LLMエージェント）

## ライセンス

このプロジェクトは以下のいずれかのライセンスで利用できます:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) または <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) または <http://opensource.org/licenses/MIT>)

どちらか好きな方を選択してください。

### コントリビューション

特に明示しない限り、あなたが投稿する貢献は Apache-2.0 ライセンスで定義される通り、
上記のデュアルライセンスの下で提供されるものとし、追加の条件は付与されません。
