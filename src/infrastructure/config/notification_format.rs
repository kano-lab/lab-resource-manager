//! 通知フォーマット設定
//!
//! 通知メッセージのテンプレートとフォーマットスタイルを定義します。

use serde::Deserialize;

/// メッセージテンプレート設定
///
/// 各イベントタイプ（作成・更新・削除）のメッセージテンプレートを定義。
/// プレースホルダー: `{user}`, `{resource}`, `{time}`, `{notes}`, `{resource_label}`
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash, Default)]
pub struct TemplateConfig {
    /// 予約作成時のテンプレート
    #[serde(default)]
    pub created: Option<String>,

    /// 予約更新時のテンプレート
    #[serde(default)]
    pub updated: Option<String>,

    /// 予約削除時のテンプレート
    #[serde(default)]
    pub deleted: Option<String>,
}

/// リソース表示スタイル
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourceStyle {
    /// フル表示: "Thalys / A100 80GB PCIe / GPU:0"
    #[default]
    Full,
    /// コンパクト表示: "Thalys 0,1,2"
    Compact,
    /// サーバー名のみ: "Thalys"
    ServerOnly,
}

/// 時刻表示スタイル
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimeStyle {
    /// フル表示: "2024-01-15 19:00 - 2024-01-15 21:00 (Asia/Tokyo)"
    #[default]
    Full,
    /// スマート表示: 同日なら終了日省略 "1/15 19:00 - 21:00"
    Smart,
    /// 相対表示: "今日 19:00 - 21:00", "明日 10:00 - 12:00"
    Relative,
}

/// 日付フォーマット
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    /// ISO形式: "2024-01-15"
    #[default]
    Ymd,
    /// 月/日: "1/15"
    Md,
    /// 日本語: "1月15日"
    MdJapanese,
}

/// フォーマット設定
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash, Default)]
pub struct FormatConfig {
    /// リソース表示スタイル
    #[serde(default)]
    pub resource_style: ResourceStyle,

    /// 時刻表示スタイル
    #[serde(default)]
    pub time_style: TimeStyle,

    /// 日付フォーマット
    #[serde(default)]
    pub date_format: DateFormat,
}

/// 通知カスタマイズ設定（統合）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct NotificationCustomization {
    /// メッセージテンプレート
    pub templates: TemplateConfig,

    /// フォーマット設定
    pub format: FormatConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_style_default() {
        let style: ResourceStyle = Default::default();
        assert_eq!(style, ResourceStyle::Full);
    }

    #[test]
    fn test_time_style_default() {
        let style: TimeStyle = Default::default();
        assert_eq!(style, TimeStyle::Full);
    }

    #[test]
    fn test_date_format_default() {
        let format: DateFormat = Default::default();
        assert_eq!(format, DateFormat::Ymd);
    }

    #[test]
    fn test_template_config_default() {
        let config: TemplateConfig = Default::default();
        assert!(config.created.is_none());
        assert!(config.updated.is_none());
        assert!(config.deleted.is_none());
    }

    #[test]
    fn test_deserialize_resource_style() {
        #[derive(Deserialize)]
        struct Test {
            style: ResourceStyle,
        }

        let toml = r#"style = "compact""#;
        let test: Test = toml::from_str(toml).unwrap();
        assert_eq!(test.style, ResourceStyle::Compact);

        let toml = r#"style = "server_only""#;
        let test: Test = toml::from_str(toml).unwrap();
        assert_eq!(test.style, ResourceStyle::ServerOnly);
    }

    #[test]
    fn test_deserialize_time_style() {
        #[derive(Deserialize)]
        struct Test {
            style: TimeStyle,
        }

        let toml = r#"style = "smart""#;
        let test: Test = toml::from_str(toml).unwrap();
        assert_eq!(test.style, TimeStyle::Smart);

        let toml = r#"style = "relative""#;
        let test: Test = toml::from_str(toml).unwrap();
        assert_eq!(test.style, TimeStyle::Relative);
    }

    #[test]
    fn test_deserialize_date_format() {
        #[derive(Deserialize)]
        struct Test {
            format: DateFormat,
        }

        let toml = r#"format = "md""#;
        let test: Test = toml::from_str(toml).unwrap();
        assert_eq!(test.format, DateFormat::Md);

        let toml = r#"format = "md_japanese""#;
        let test: Test = toml::from_str(toml).unwrap();
        assert_eq!(test.format, DateFormat::MdJapanese);
    }
}
