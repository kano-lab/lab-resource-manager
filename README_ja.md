# lab-resource-manager

研究室のGPU・部屋などの資源の使用状況を管理し、変更を通知するシステム。

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
    └── lab-resource-manager.rs  # リソースアクセス管理用Slackボット
```

## Setup

セットアップの詳細については、[管理者ガイド](docs/ADMIN_GUIDE_ja.md)を参照してください。

## インストール

### バイナリリリースから（推奨）

[GitHub Releases](https://github.com/kano-lab/lab-resource-manager/releases) から最新版をダウンロード:

```bash
# ダウンロードして展開
curl -LO https://github.com/kano-lab/lab-resource-manager/releases/latest/download/lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz
tar -xzf lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# インストール（root権限が必要）
sudo bash deploy/install.sh
```

インストールされるもの:

- バイナリ: `/usr/local/bin/lab-resource-manager`
- 設定ディレクトリ: `/etc/lab-resource-manager/`
- データディレクトリ: `/var/lib/lab-resource-manager/`
- systemdサービスファイル

### ソースから

```bash
cargo build --release
```

### Dockerからの移行

Docker ベースのデプロイからアップグレードする場合は、[マイグレーションガイド](docs/MIGRATION_ja.md)を参照してください。

## Usage

### Watcher起動

```bash
# デフォルト（リポジトリ実装 + 設定済み通知先）
cargo run --bin watcher

# Mockリポジトリ使用（テスト用）
cargo run --bin watcher --repository mock

# ポーリング間隔指定（デフォルト60秒）
cargo run --bin watcher --interval 30
```

### オプション

- `--repository <google_calendar|mock>`: リポジトリ実装の選択
- `--interval <秒>`: ポーリング間隔

通知実装は `config/resources.toml` でリソースごとに設定します。

### Slackボット起動

Slackボットを使うと、ユーザーがメールアドレスを登録して全てのリソースコレクションへのアクセスを取得できます:

```bash
# ボットの起動
cargo run --bin lab-resource-manager
```

**Slackコマンド:**

- `/register-calendar <your-email@example.com>` - 自分のメールアドレスを登録し、Slackアカウントと連携
- `/link-user <@slack_user> <email@example.com>` - 他のユーザーのメールアドレスをSlackアカウントと連携

### ライブラリとして使用

`Cargo.toml`に追加:

```toml
[dependencies]
lab-resource-manager = "0.1"
```

使用例（Google Calendar実装を使用）:

```rust
use lab_resource_manager::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 設定の読み込み
    let config = load_config("config/resources.toml")?;

    // リポジトリ実装の作成（ここではGoogle Calendar実装を使用）
    let repository = GoogleCalendarUsageRepository::new(
        "secrets/service-account.json",
        config.clone(),
        PathBuf::from("data/google_calendar_mappings.json"),
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

## デバイス指定記法

リソース使用状況のタイトルで、以下の形式でデバイスを指定できます:

- 単一: `0` → デバイス0
- 範囲: `0-2` → デバイス0, 1, 2
- 複数: `0,2,5` → デバイス0, 2, 5
- 混在: `0-1,6-7` → デバイス0, 1, 6, 7

ドメイン層の`ResourceFactory`がこれらの指定のパースを処理します。

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
