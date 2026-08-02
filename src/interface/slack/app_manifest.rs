//! Slackアプリの設定（App Manifest）
//!
//! # 何をコードが決め、何を導入先が決めるか
//!
//! スラッシュコマンドの一覧と必要なスコープは、このボットが動くための条件である。
//! コマンドがSlackに登録されていなければボットには何も届かず、スコープが足りなければ
//! 送信も返信もできない。だからこの2つはコードが決め、ここを唯一の出どころとする。
//!
//! 名前・説明・色といった見え方は導入先が決めてよい。`deploy/`に置いてある生成物は
//! そのまま使える例であり、コピーして変えることを想定している。

use serde::Serialize;

/// このボットが受け取るスラッシュコマンド
///
/// Slackに登録されていないコマンドは、ボットが動いていても呼び出されない。
/// 追加したときは`gateway`のルーティングと揃える必要がある（テストで縛っている）。
pub struct SlashCommandSpec {
    /// Slackに登録するコマンド名
    pub command: &'static str,
    /// コマンド入力時にSlackが表示する説明
    pub description: &'static str,
    /// 引数の書き方（引数を取らないコマンドでは`None`）
    pub usage_hint: Option<&'static str>,
}

/// このボットが受け取るスラッシュコマンドの一覧
pub const SLASH_COMMANDS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        command: "/free",
        description: "いま空いているリソースを一覧する",
        usage_hint: None,
    },
    SlashCommandSpec {
        command: "/reserve",
        description: "リソースを予約する",
        usage_hint: None,
    },
    SlashCommandSpec {
        command: "/register-calendar",
        description: "自分のメールアドレスをリソースカレンダーに登録する",
        usage_hint: Some("<your-email@example.com>"),
    },
    SlashCommandSpec {
        command: "/link-user",
        description: "他の人物の識別情報を紐付ける（管理者用）",
        usage_hint: None,
    },
    SlashCommandSpec {
        command: "/mcp-token",
        description: "MCPアクセストークンを発行する",
        usage_hint: None,
    },
    SlashCommandSpec {
        command: "/status",
        description: "実利用の監視の稼働状況を表示する（管理者用）",
        usage_hint: None,
    },
];

/// このボットが必要とするBotトークンのスコープ
///
/// - `chat:write`: 予約通知の投稿、操作結果の返信、送信済みメッセージの書き換え
/// - `commands`: スラッシュコマンドを受け取る
/// - `im:write`: 事後予約の提案などを本人へ届けるDMチャンネルを開く
///
/// Block Kitのユーザー選択（`/link-user`）はSlackが解決するため、`users:read`は要らない。
pub const BOT_SCOPES: &[&str] = &["chat:write", "commands", "im:write"];

/// 例として配る見え方の既定値
const DEFAULT_APP_NAME: &str = "LabResourceManager";
const DEFAULT_APP_DESCRIPTION: &str = "研究室リソースの管理・通知ボット";
const DEFAULT_BACKGROUND_COLOR: &str = "#1d7c00";
const DEFAULT_BOT_DISPLAY_NAME: &str = "Lab Resource Manager";

/// Slackアプリの設定
#[derive(Debug, Serialize)]
pub struct AppManifest {
    display_information: DisplayInformation,
    features: Features,
    oauth_config: OauthConfig,
    settings: Settings,
}

#[derive(Debug, Serialize)]
struct DisplayInformation {
    name: String,
    description: String,
    background_color: String,
}

#[derive(Debug, Serialize)]
struct Features {
    bot_user: BotUser,
    slash_commands: Vec<SlashCommand>,
}

#[derive(Debug, Serialize)]
struct BotUser {
    display_name: String,
    always_online: bool,
}

#[derive(Debug, Serialize)]
struct SlashCommand {
    command: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage_hint: Option<String>,
    should_escape: bool,
}

#[derive(Debug, Serialize)]
struct OauthConfig {
    scopes: Scopes,
    pkce_enabled: bool,
}

#[derive(Debug, Serialize)]
struct Scopes {
    bot: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Settings {
    interactivity: Interactivity,
    org_deploy_enabled: bool,
    socket_mode_enabled: bool,
    token_rotation_enabled: bool,
}

#[derive(Debug, Serialize)]
struct Interactivity {
    is_enabled: bool,
}

impl AppManifest {
    /// このボットが要求する設定から、そのまま使える例を組み立てる
    pub fn example() -> Self {
        Self {
            display_information: DisplayInformation {
                name: DEFAULT_APP_NAME.to_string(),
                description: DEFAULT_APP_DESCRIPTION.to_string(),
                background_color: DEFAULT_BACKGROUND_COLOR.to_string(),
            },
            features: Features {
                bot_user: BotUser {
                    display_name: DEFAULT_BOT_DISPLAY_NAME.to_string(),
                    always_online: true,
                },
                slash_commands: SLASH_COMMANDS
                    .iter()
                    .map(|spec| SlashCommand {
                        command: spec.command.to_string(),
                        description: spec.description.to_string(),
                        usage_hint: spec.usage_hint.map(str::to_string),
                        // 引数はボット側で解釈するため、Slackにエスケープさせない
                        should_escape: false,
                    })
                    .collect(),
            },
            oauth_config: OauthConfig {
                scopes: Scopes {
                    bot: BOT_SCOPES.iter().map(|scope| scope.to_string()).collect(),
                },
                pkce_enabled: false,
            },
            settings: Settings {
                interactivity: Interactivity { is_enabled: true },
                org_deploy_enabled: false,
                // Socket Modeで動くため、SlackからのRequest URLは不要
                socket_mode_enabled: true,
                token_rotation_enabled: false,
            },
        }
    }

    /// JSON形式に整形する
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("マニフェストは常にJSONにできる")
    }

    /// YAML形式に整形する
    ///
    /// 構造が決まっているため、YAMLライブラリを増やさず組み立てる。
    /// Slackの設定画面が既定で見せる形式であり、人が読み書きしやすい。
    pub fn to_yaml(&self) -> String {
        let mut out = String::new();

        out.push_str("display_information:\n");
        out.push_str(&format!(
            "  name: {}\n",
            quoted(&self.display_information.name)
        ));
        out.push_str(&format!(
            "  description: {}\n",
            quoted(&self.display_information.description)
        ));
        out.push_str(&format!(
            "  background_color: {}\n",
            quoted(&self.display_information.background_color)
        ));

        out.push_str("features:\n");
        out.push_str("  bot_user:\n");
        out.push_str(&format!(
            "    display_name: {}\n",
            quoted(&self.features.bot_user.display_name)
        ));
        out.push_str(&format!(
            "    always_online: {}\n",
            self.features.bot_user.always_online
        ));
        out.push_str("  slash_commands:\n");
        for command in &self.features.slash_commands {
            out.push_str(&format!("    - command: {}\n", quoted(&command.command)));
            out.push_str(&format!(
                "      description: {}\n",
                quoted(&command.description)
            ));
            if let Some(usage_hint) = &command.usage_hint {
                out.push_str(&format!("      usage_hint: {}\n", quoted(usage_hint)));
            }
            out.push_str(&format!("      should_escape: {}\n", command.should_escape));
        }

        out.push_str("oauth_config:\n");
        out.push_str("  scopes:\n");
        out.push_str("    bot:\n");
        for scope in &self.oauth_config.scopes.bot {
            out.push_str(&format!("      - {}\n", quoted(scope)));
        }
        out.push_str(&format!(
            "  pkce_enabled: {}\n",
            self.oauth_config.pkce_enabled
        ));

        out.push_str("settings:\n");
        out.push_str("  interactivity:\n");
        out.push_str(&format!(
            "    is_enabled: {}\n",
            self.settings.interactivity.is_enabled
        ));
        out.push_str(&format!(
            "  org_deploy_enabled: {}\n",
            self.settings.org_deploy_enabled
        ));
        out.push_str(&format!(
            "  socket_mode_enabled: {}\n",
            self.settings.socket_mode_enabled
        ));
        out.push_str(&format!(
            "  token_rotation_enabled: {}\n",
            self.settings.token_rotation_enabled
        ));

        out
    }
}

/// YAMLの文字列として安全な形にする
///
/// 日本語や`/`・`#`を含む値をそのまま置くと、YAMLの解釈が値によって変わる。
/// 常に引用符で囲み、値の中身に関係なく同じ読まれ方をさせる。
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// スラッシュコマンドのルーティング（このファイルからの相対パス）
    const GATEWAY_SOURCE: &str = include_str!("gateway.rs");

    #[test]
    fn every_declared_command_is_routed() {
        for spec in SLASH_COMMANDS {
            assert!(
                GATEWAY_SOURCE.contains(&format!("\"{}\"", spec.command)),
                "{} を宣言しているが、gatewayが受け取っていない",
                spec.command
            );
        }
    }

    #[test]
    fn every_routed_command_is_declared() {
        // ルーティングは `"/xxx" =>` の形で書かれている
        let routed: Vec<&str> = GATEWAY_SOURCE
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix("\"/")?;
                let (command, _) = rest.split_once('"')?;
                Some(command)
            })
            .collect();

        assert!(!routed.is_empty(), "ルーティングを読み取れていない");

        for command in routed {
            assert!(
                SLASH_COMMANDS
                    .iter()
                    .any(|spec| spec.command == format!("/{}", command)),
                "/{} を受け取っているが、マニフェストに宣言がない（Slackに登録されず届かない）",
                command
            );
        }
    }

    #[test]
    fn the_scopes_are_stated_once_each_and_in_order() {
        let mut sorted = BOT_SCOPES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        assert_eq!(
            BOT_SCOPES,
            &sorted[..],
            "スコープは重複なく並べ、差分を読みやすくする"
        );
    }

    #[test]
    fn both_formats_carry_every_command_and_scope() {
        let manifest = AppManifest::example();
        let yaml = manifest.to_yaml();
        let json = manifest.to_json();

        for spec in SLASH_COMMANDS {
            assert!(yaml.contains(spec.command), "YAMLに{}がない", spec.command);
            assert!(json.contains(spec.command), "JSONに{}がない", spec.command);
        }
        for scope in BOT_SCOPES {
            assert!(yaml.contains(scope), "YAMLに{}がない", scope);
            assert!(json.contains(scope), "JSONに{}がない", scope);
        }
    }

    #[test]
    fn a_command_without_arguments_has_no_usage_hint() {
        let manifest = AppManifest::example();
        let yaml = manifest.to_yaml();

        let reserve_block = yaml
            .split("- command: \"/reserve\"")
            .nth(1)
            .expect("/reserve が出ていない");
        let reserve_block = reserve_block
            .split("- command:")
            .next()
            .expect("次のコマンドまでを取れない");

        assert!(
            !reserve_block.contains("usage_hint"),
            "引数を取らないコマンドに書き方の例を出すと、あるものと読まれる: {reserve_block}"
        );
    }

    #[test]
    fn values_are_quoted_so_yaml_reads_them_the_same_way() {
        assert_eq!(quoted("#1d7c00"), "\"#1d7c00\"");
        assert_eq!(quoted("say \"hi\""), "\"say \\\"hi\\\"\"");
    }
}
