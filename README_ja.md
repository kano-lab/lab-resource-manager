# lab-resource-manager

研究室向けリソース予約管理システム。

## 概要

- **リソース予約管理**: GPUサーバーや部屋といった資源利用のスケジュールを管理
- **変更通知**: 予約の作成・変更・取り消し時に通知（定期予約は予約枠ごとではなく1件にまとめて通知）
- **Identity Linking**: 異なるシステム間でユーザーIDを紐付けて通知を強化

### デフォルト実装

| コンポーネント | 実装 |
| --- | --- |
| リソースリポジトリ | Google Calendar |
| 通知 | Slack |
| ユーザーインターフェース | Slack Bot |
| アクセス制御 | Google Calendar ACL |

## クイックスタート

- **ユーザー向け**: [ユーザーガイド](docs/USER_GUIDE_ja.md)
- **管理者向け**: [管理者ガイド](docs/ADMIN_GUIDE_ja.md)

## アーキテクチャ

Clean Architecture（DDD + ヘキサゴナルアーキテクチャ）で設計:

```text
src/
├── domain/                  # ドメイン層（ビジネスロジック）
│   ├── aggregates/          # 集約（Reservation, IdentityLink）
│   ├── common/              # Shared Kernel（EmailAddress等）
│   ├── ports/               # ポート（Repository, Notifierトレイト）
│   ├── read_models/         # 読み取りモデル（ReservedSlotView: 期間で引く予約枠の写し）
│   └── errors.rs            # ドメインエラー
├── application/             # アプリケーション層（ユースケース）
│   └── usecases/            # 通知、アクセス許可ユースケース
├── infrastructure/          # インフラ層（実装）
│   ├── repositories/        # リポジトリ実装
│   ├── notifier/            # 通知実装
│   ├── resource_collection_access/  # アクセス制御実装
│   └── config/              # 設定管理
├── interface/               # インターフェース層
│   └── slack/               # Slackボットインターフェース
└── bin/
    └── lab-resource-manager.rs  # エントリポイント
```

## 開発

### 必要なもの

- Rust 1.90+

### ビルド & テスト

```bash
cargo build
cargo test
cargo fmt
cargo clippy
```

### Google カレンダーとの契約テスト

Google カレンダーのアダプタが頼っている API の振る舞いは、契約テストで確かめます。
`cargo test` では偽のゲートウェイを相手に流れ、本物のテスト用カレンダーには毎週・アダプタを変える
プルリクエスト・必要なときに流れます。
本物で流すには、ほかに何も入れていないカレンダーと、そのカレンダーの「予定の変更」権限を持つ
サービスアカウントを用意します。`.env.contract.example` を `.env.contract` にコピーし、カレンダーIDと
鍵ファイルのパスを入れて（鍵は `secrets/` の下に置けば git に入りません）、次で流します。

```bash
cargo test --all-features contract_tests -- --ignored
```

環境変数 `LRM_CONTRACT_CALENDAR_ID` と `LRM_CONTRACT_SERVICE_ACCOUNT_KEY` が設定されていれば、
ファイルよりそちらが優先されます。

GitHub Actions では `Calendar Contract` ワークフローが、毎週月曜・このリポジトリのブランチからの
アダプタを変えるプルリクエスト・手動での実行のときに流れます。リポジトリのシークレット
`LRM_CONTRACT_CALENDAR_ID` と `LRM_CONTRACT_SERVICE_ACCOUNT_KEY`（鍵ファイルの中身）を使い、
同時に1本だけ流れます。

## ロードマップ

- [x] リソースベースの通知ルーティング
- [x] Identity Linking
- [x] アクセス管理インターフェース
- [x] Slackコマンドでの予約作成
- [ ] 自然言語でのリソース管理（LLMエージェント）

## ライセンス

以下のいずれかのライセンスで利用可能:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) または <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) または <http://opensource.org/licenses/MIT>)

### コントリビューション

特に明示しない限り、投稿された貢献はApache-2.0ライセンスで定義される通り、
上記のデュアルライセンスの下で提供されます。
