# バージョンの付け方

このリポジトリは2つのクレートをリリースする。それぞれ別の約束事に従って版を付ける。

| クレート | 頼りにしている人 | 版が従うもの |
|---|---|---|
| `lab-resource-manager`（アプリケーション） | 運用者、SlackやMCPから使う人 | 設置したものが頼りにしているもの（後述） |
| `lab-resource-manager-core`（ライブラリ） | このクレートに依存するRustのコード | RustのAPI（[CargoのSemVerの規則](https://doc.rust-lang.org/cargo/reference/semver.html)） |

リリースは[release-plz](https://release-plz.dev/)（`release-plz.toml`）が準備する。
次の版と変更履歴を載せたリリースPRを開いておき、それをマージすると両クレートを公開し、タグを付け、アプリケーションのGitHubリリースを作る。

## アプリケーション

**破壊的変更**とは、バイナリを差し替えただけでは既存の設置が今までどおりに動かず、運用者が何かをしなければならない変更をいう。
`deploy/auto-update.sh`が引く線と同じで、minorとpatchは自動で当て、majorは管理者に任せる。

| 約束に含まれる（変えたら破壊的） | 約束に含まれない |
|---|---|
| 環境変数とその意味 | `lab-resource-manager`クレートのRustのAPI |
| `resources.toml`の書式 | ログのメッセージとフィールド名 |
| データファイルの書式（`identity_links.json`、MCPトークン、状態を置くディレクトリ） | Slackに出す文面 |
| `deploy/lab-resource-manager.service`とコマンドライン引数 | |
| Slackアプリの権限（増やすとアプリの再インストールが要る）、スラッシュコマンドの名前と意味 | |
| MCPツールの名前と入出力（呼ぶ側がプログラムのため） | |

破壊的変更はコミットメッセージに`!`を付け（`feat!: ...`）、運用者がすべきことを[MIGRATION_ja.md](MIGRATION_ja.md)に書く。
`!`はこの約束を壊すときだけに使う。
ライブラリの変更は次節を参照。

`feat`はminor、`fix`と`perf`はpatchのリリースになる。リファクタリング、文書、テストだけではリリースを提案せず、次のリリースに含まれる。

## ライブラリ

APIの破壊的変更は、リリースPRを準備するときに[cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)が見つける。
コミットメッセージで知らせる必要はない。
1.0になるまでは、破壊的変更でminorが上がる（0.1.x → 0.2.0）。

ライブラリのAPIだけを変える変更に`!`を付けない。
ライブラリを変えるPRは、たいていアプリケーション側の呼び出しも直す。
スカッシュマージで両方が1つのコミットになるため、`!`を付けると運用者には何も変わらないのにアプリケーションがmajorになってしまう。

ライブラリをリリースするときは、アプリケーションもpatchでリリースする。ライブラリの変更は、アプリケーションのバイナリを通してしか運用者に届かないため。

## タグ

| クレート | タグ | GitHubリリース |
|---|---|---|
| アプリケーション | `vX.Y.Z` | 作る。`deploy/auto-update.sh`が取得するバイナリを添える |
| ライブラリ | `lab-resource-manager-core-vX.Y.Z` | 作らない（crates.ioとdocs.rsにある）。アプリケーションのリリースを最新のままにするため |
