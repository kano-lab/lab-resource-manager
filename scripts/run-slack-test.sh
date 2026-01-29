#!/bin/bash
# Slack通知テスト実行スクリプト
# Google Calendar なしでSlackボットの動作をテストする
#
# 使用方法: ./scripts/run-slack-test.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_DIR"

# 環境変数ファイルの存在確認
if [ ! -f "secrets/.env" ]; then
    echo "❌ secrets/.env が見つかりません"
    echo "   .env.example をコピーして設定してください:"
    echo "   cp .env.example secrets/.env"
    exit 1
fi

# 環境変数を読み込み
set -a
source secrets/.env
set +a

# Slackトークンの確認
if [[ "$SLACK_BOT_TOKEN" == "xoxb-YOUR-BOT-TOKEN" ]]; then
    echo "❌ SLACK_BOT_TOKEN がデフォルト値のままです"
    echo "   secrets/.env で実際のトークンを設定してください"
    exit 1
fi

if [[ "$SLACK_APP_TOKEN" == "xapp-YOUR-APP-TOKEN" ]]; then
    echo "❌ SLACK_APP_TOKEN がデフォルト値のままです"
    echo "   secrets/.env で実際のトークンを設定してください"
    exit 1
fi

echo "🧪 Slack通知テストモードで起動します"
echo ""
echo "📋 設定:"
echo "   RESOURCE_CONFIG: ${RESOURCE_CONFIG:-./config/resources.toml}"
echo "   IDENTITY_LINKS_FILE: ${IDENTITY_LINKS_FILE:-./data/identity_links.json}"
echo ""

# デバッグログを有効にする場合は RUST_LOG を設定
export RUST_LOG="${RUST_LOG:-info}"

cargo run --bin slack-test
