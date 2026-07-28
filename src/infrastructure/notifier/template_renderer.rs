//! テンプレートレンダラー
//!
//! 通知メッセージのテンプレートとプレースホルダー置換を処理します。

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::infrastructure::config::{FormatConfig, TemplateConfig};
use crate::infrastructure::notifier::formatter::{format_resources_styled, format_time_styled};

/// プレースホルダー定義
pub mod placeholders {
    /// ユーザー表示名/メンション
    pub const USER: &str = "{user}";
    /// リソース情報
    pub const RESOURCE: &str = "{resource}";
    /// 時刻情報
    pub const TIME: &str = "{time}";
    /// 備考
    pub const NOTES: &str = "{notes}";
    /// リソースラベル（💻 予約GPU等）
    pub const RESOURCE_LABEL: &str = "{resource_label}";
}

/// デフォルトテンプレート（現在のハードコード値と同等）
pub mod defaults {
    /// 予約作成時のデフォルトテンプレート
    pub const CREATED: &str =
        "🔔 新規予約\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 予約更新時のデフォルトテンプレート
    pub const UPDATED: &str =
        "🔄 予約更新\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 予約削除時のデフォルトテンプレート
    pub const DELETED: &str =
        "🗑️ 予約削除\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 予約競合時のデフォルトテンプレート
    pub const CONFLICT: &str = "⚠️ 予約が重複しています\n👤 予約者\n{user}\n\n📅 競合する期間\n{time}\n\n{resource_label}\n{resource}{notes}";
}

/// テンプレートレンダラー
pub struct TemplateRenderer<'a> {
    templates: &'a TemplateConfig,
    format: &'a FormatConfig,
    timezone: Option<&'a str>,
}

impl<'a> TemplateRenderer<'a> {
    /// 新しいレンダラーを作成
    pub fn new(
        templates: &'a TemplateConfig,
        format: &'a FormatConfig,
        timezone: Option<&'a str>,
    ) -> Self {
        Self {
            templates,
            format,
            timezone,
        }
    }

    /// 予約作成メッセージをレンダリング
    pub fn render_created(&self, usage: &ResourceUsage, user_display: &str) -> String {
        let template = self
            .templates
            .created
            .as_deref()
            .unwrap_or(defaults::CREATED);
        self.render(
            template,
            usage.resources(),
            usage.time_period(),
            usage.notes().map(String::as_str),
            user_display,
        )
    }

    /// 予約更新メッセージをレンダリング
    pub fn render_updated(&self, usage: &ResourceUsage, user_display: &str) -> String {
        let template = self
            .templates
            .updated
            .as_deref()
            .unwrap_or(defaults::UPDATED);
        self.render(
            template,
            usage.resources(),
            usage.time_period(),
            usage.notes().map(String::as_str),
            user_display,
        )
    }

    /// 予約削除メッセージをレンダリング
    pub fn render_deleted(&self, usage: &ResourceUsage, user_display: &str) -> String {
        let template = self
            .templates
            .deleted
            .as_deref()
            .unwrap_or(defaults::DELETED);
        self.render(
            template,
            usage.resources(),
            usage.time_period(),
            usage.notes().map(String::as_str),
            user_display,
        )
    }

    /// 予約競合メッセージをレンダリング
    ///
    /// `existing_usage`は今回の予約要求と競合した既存の使用予定。
    /// `conflicting_resource`は競合したリソース単体（要求リソースのうち衝突した1件）。
    pub fn render_conflict(
        &self,
        conflicting_resources: &[Resource],
        existing_usage: &ResourceUsage,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .conflict
            .as_deref()
            .unwrap_or(defaults::CONFLICT);
        self.render(
            template,
            conflicting_resources,
            existing_usage.time_period(),
            existing_usage.notes().map(String::as_str),
            user_display,
        )
    }

    /// テンプレートをレンダリング（シングルパス方式）
    ///
    /// チェーン式の`replace`だと置換後の値にプレースホルダーが含まれる場合に
    /// 誤置換が発生する可能性があるため、シングルパスで処理する。
    fn render(
        &self,
        template: &str,
        resources: &[Resource],
        time_period: &TimePeriod,
        notes: Option<&str>,
        user_display: &str,
    ) -> String {
        let resources_formatted = format_resources_styled(resources, self.format.resource_style);

        let time_formatted = format_time_styled(
            time_period,
            self.timezone,
            self.format.time_style,
            self.format.date_format,
        );

        let notes_formatted = notes
            .filter(|n| !n.is_empty())
            .map(|n| format!("\n\n📝 備考\n{}", n))
            .unwrap_or_default();

        let resource_label = Self::get_resource_label(resources);

        // シングルパスでテンプレートを走査し、プレースホルダーのみ置換する
        // プレースホルダーはすべてASCIIなので .len() で文字数を取得可能
        let mut result = String::with_capacity(template.len() + resources_formatted.len() * 2);
        let mut chars = template.char_indices().peekable();

        while let Some((i, ch)) = chars.next() {
            let rest = &template[i..];

            // 長いプレースホルダーから順にチェック（{resource_label}と{resource}の順序に注意）
            if rest.starts_with(placeholders::RESOURCE_LABEL) {
                result.push_str(resource_label);
                // プレースホルダー分の文字をスキップ（先頭1文字は既に読み込み済み）
                for _ in 1..placeholders::RESOURCE_LABEL.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::RESOURCE) {
                result.push_str(&resources_formatted);
                for _ in 1..placeholders::RESOURCE.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::USER) {
                result.push_str(user_display);
                for _ in 1..placeholders::USER.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::TIME) {
                result.push_str(&time_formatted);
                for _ in 1..placeholders::TIME.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::NOTES) {
                result.push_str(&notes_formatted);
                for _ in 1..placeholders::NOTES.len() {
                    chars.next();
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// リソースタイプに応じたラベルを取得
    fn get_resource_label(resources: &[Resource]) -> &'static str {
        if resources.is_empty() {
            return "📦 予約リソース";
        }

        let has_gpu = resources.iter().any(|r| matches!(r, Resource::Gpu(_)));
        let has_room = resources.iter().any(|r| matches!(r, Resource::Room { .. }));

        match (has_gpu, has_room) {
            (true, false) => "💻 予約GPU",
            (false, true) => "🏢 予約部屋",
            _ => "📦 予約リソース",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, TimePeriod};
    use crate::domain::common::EmailAddress;
    use crate::infrastructure::config::{DateFormat, ResourceStyle, TimeStyle};
    use chrono::{TimeZone, Utc};

    fn create_test_usage() -> ResourceUsage {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = TimePeriod::new(start, end).unwrap();

        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
        ];

        ResourceUsage::new(
            EmailAddress::new("test@example.com".to_string()).unwrap(),
            period,
            resources,
            Some("テスト用予約".to_string()),
        )
        .unwrap()
    }

    #[test]
    fn test_render_with_default_template() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let usage = create_test_usage();

        let result = renderer.render_created(&usage, "<@U12345>");

        assert!(result.contains("🔔 新規予約"));
        assert!(result.contains("<@U12345>"));
        assert!(result.contains("Thalys"));
        assert!(result.contains("📝 備考"));
        assert!(result.contains("テスト用予約"));
    }

    #[test]
    fn test_render_with_custom_template() {
        let templates = TemplateConfig {
            created: Some("{user}が{resource}を{time}使います".to_string()),
            updated: None,
            deleted: None,
            conflict: None,
        };
        let format = FormatConfig {
            resource_style: ResourceStyle::Compact,
            time_style: TimeStyle::Smart,
            date_format: DateFormat::Md,
        };

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let usage = create_test_usage();

        let result = renderer.render_created(&usage, "田中太郎");

        assert!(result.contains("田中太郎が"));
        assert!(result.contains("Thalys 0,1")); // compact format
        assert!(result.contains("1/15")); // smart format with md date
        assert!(result.contains("使います"));
        // notes not in template, so not included
        assert!(!result.contains("備考"));
    }

    #[test]
    fn test_render_with_custom_format_only() {
        let templates = TemplateConfig::default();
        let format = FormatConfig {
            resource_style: ResourceStyle::ServerOnly,
            time_style: TimeStyle::Relative,
            date_format: DateFormat::MdJapanese,
        };

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let usage = create_test_usage();

        let result = renderer.render_updated(&usage, "user@example.com");

        assert!(result.contains("🔄 予約更新"));
        assert!(result.contains("Thalys")); // server only
        assert!(!result.contains("GPU:0")); // no device number in server_only
    }

    #[test]
    fn test_get_resource_label_gpu() {
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];
        assert_eq!(
            TemplateRenderer::get_resource_label(&resources),
            "💻 予約GPU"
        );
    }

    #[test]
    fn test_get_resource_label_room() {
        let resources = vec![Resource::Room {
            name: "会議室A".to_string(),
        }];
        assert_eq!(
            TemplateRenderer::get_resource_label(&resources),
            "🏢 予約部屋"
        );
    }

    #[test]
    fn test_get_resource_label_mixed() {
        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Room {
                name: "会議室A".to_string(),
            },
        ];
        assert_eq!(
            TemplateRenderer::get_resource_label(&resources),
            "📦 予約リソース"
        );
    }

    #[test]
    fn test_render_conflict_with_default_template() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let existing_usage = create_test_usage();
        let conflicting_resource =
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()));

        let result = renderer.render_conflict(
            std::slice::from_ref(&conflicting_resource),
            &existing_usage,
            "<@U67890>",
        );

        assert!(result.contains("⚠️ 予約が重複しています"));
        assert!(result.contains("<@U67890>"));
        assert!(result.contains("Thalys"));
        // 競合リソースは1件のみなので、既存予約の2台目(GPU:1)は含まれない
        assert!(!result.contains("GPU:1"));
    }

    #[test]
    fn test_render_conflict_with_custom_template() {
        let templates = TemplateConfig {
            created: None,
            updated: None,
            deleted: None,
            conflict: Some("{user}が既に{resource}を{time}で予約済みです".to_string()),
        };
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let existing_usage = create_test_usage();
        let conflicting_resource =
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string()));

        let result = renderer.render_conflict(
            std::slice::from_ref(&conflicting_resource),
            &existing_usage,
            "田中太郎",
        );

        assert!(result.contains("田中太郎が既に"));
        assert!(result.contains("GPU:1"));
        assert!(result.contains("予約済みです"));
    }

    #[test]
    fn test_render_without_notes() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));

        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];

        let usage = ResourceUsage::new(
            EmailAddress::new("test@example.com".to_string()).unwrap(),
            period,
            resources,
            None, // no notes
        )
        .unwrap();

        let result = renderer.render_created(&usage, "user");

        assert!(!result.contains("📝 備考"));
    }
}
