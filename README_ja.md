# lab-resource-manager

研究室向けリソース予約管理システム。

## 概要

- **リソース予約管理**: GPUサーバーや部屋といった資源利用のスケジュールを管理
- **変更通知**: 予約の作成・更新・削除時に通知
- **Identity Linking**: 異なるシステム間でユーザーIDを紐付けて通知を強化

### デフォルト実装

| コンポーネント | 実装 |
|---------------|------|
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
│   ├── aggregates/          # 集約（ResourceUsage, IdentityLink）
│   ├── common/              # Shared Kernel（EmailAddress等）
│   ├── ports/               # ポート（Repository, Notifierトレイト）
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
