use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};

/// 全デバイス指定を表すデバイス指定記法
pub const SPEC_ALL: &str = "all";

/// デバイス指定記法からResourceオブジェクトを生成するファクトリ
pub struct ResourceFactory;

impl ResourceFactory {
    /// デバイス指定記法から Resource のリストを生成
    ///
    /// # 記法
    /// - 全て: "all" → サーバーの全デバイス
    /// - 単一: "0" → \[0\]
    /// - 範囲: "0-2" → \[0, 1, 2\]
    /// - 複数: "0,2,5" → \[0, 2, 5\]
    /// - 混在: "0-1,6-7" → \[0, 1, 6, 7\]
    ///
    /// # Arguments
    /// * `spec` - デバイス指定文字列
    /// * `server_name` - サーバー名
    /// * `all_device_ids` - サーバーの全デバイスIDリスト（"all"指定時に使用）
    /// * `device_lookup` - デバイスIDからモデル名を取得するクロージャ
    ///
    /// # Returns
    /// Resourceのリスト
    ///
    /// # Errors
    /// - デバイス指定の形式が不正な場合
    /// - 指定されたデバイスが存在しない場合
    pub fn create_gpus_from_spec(
        spec: &str,
        server_name: &str,
        all_device_ids: &[u32],
        device_lookup: impl Fn(u32) -> Option<String>,
    ) -> Result<Vec<Resource>, ResourceFactoryError> {
        let device_numbers = if spec == SPEC_ALL {
            all_device_ids.to_vec()
        } else {
            Self::parse_device_numbers(spec)?
        };

        let mut resources = Vec::new();
        for device_num in device_numbers {
            let model =
                device_lookup(device_num).ok_or_else(|| ResourceFactoryError::DeviceNotFound {
                    server: server_name.to_string(),
                    device_id: device_num,
                })?;

            let gpu = Gpu::new(server_name.to_string(), device_num, model);
            resources.push(Resource::Gpu(gpu));
        }

        Ok(resources)
    }

    /// デバイス番号のパース（内部ヘルパー）
    ///
    /// "0-2,5,7-9" のような記法をパースして、個別のデバイス番号のリストに展開します。
    ///
    /// # Arguments
    /// * `spec` - デバイス指定文字列
    ///
    /// # Returns
    /// デバイス番号のリスト
    ///
    /// # Errors
    /// - 形式が不正な場合
    /// - 数値として解釈できない場合
    /// - 範囲指定が不正な場合
    fn parse_device_numbers(spec: &str) -> Result<Vec<u32>, ResourceFactoryError> {
        let mut numbers = Vec::new();

        for part in spec.split(',') {
            let part = part.trim();

            if part.is_empty() {
                continue;
            }

            if part.contains('-') {
                let range: Vec<&str> = part.split('-').collect();
                if range.len() != 2 {
                    return Err(ResourceFactoryError::InvalidFormat(part.to_string()));
                }

                let start: u32 = range[0]
                    .parse()
                    .map_err(|_| ResourceFactoryError::InvalidNumber(range[0].to_string()))?;

                let end: u32 = range[1]
                    .parse()
                    .map_err(|_| ResourceFactoryError::InvalidNumber(range[1].to_string()))?;

                if start > end {
                    return Err(ResourceFactoryError::InvalidRange { start, end });
                }

                for n in start..=end {
                    numbers.push(n);
                }
            } else {
                let num: u32 = part
                    .parse()
                    .map_err(|_| ResourceFactoryError::InvalidNumber(part.to_string()))?;
                numbers.push(num);
            }
        }

        if numbers.is_empty() {
            return Err(ResourceFactoryError::EmptySpecification);
        }

        Ok(numbers)
    }
}

/// リソースファクトリのエラー型
#[derive(Debug)]
pub enum ResourceFactoryError {
    /// デバイス指定が空
    EmptySpecification,
    /// 無効な数値
    InvalidNumber(String),
    /// 無効なフォーマット
    InvalidFormat(String),
    /// 無効な範囲指定
    InvalidRange {
        /// 範囲の開始
        start: u32,
        /// 範囲の終了
        end: u32,
    },
    /// デバイスが見つからない
    DeviceNotFound {
        /// サーバー名
        server: String,
        /// デバイスID
        device_id: u32,
    },
}

impl std::fmt::Display for ResourceFactoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceFactoryError::EmptySpecification => write!(f, "デバイス指定が空です"),
            ResourceFactoryError::InvalidNumber(s) => write!(f, "無効な数値: {}", s),
            ResourceFactoryError::InvalidFormat(s) => write!(f, "無効なフォーマット: {}", s),
            ResourceFactoryError::InvalidRange { start, end } => {
                write!(f, "無効な範囲: {}-{}", start, end)
            }
            ResourceFactoryError::DeviceNotFound { server, device_id } => {
                write!(f, "デバイス{}が{}に存在しません", device_id, server)
            }
        }
    }
}

impl std::error::Error for ResourceFactoryError {}
impl crate::domain::errors::DomainError for ResourceFactoryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_device() {
        let result = ResourceFactory::parse_device_numbers("0").unwrap();
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_parse_device_range() {
        let result = ResourceFactory::parse_device_numbers("0-2").unwrap();
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_parse_multiple_devices() {
        let result = ResourceFactory::parse_device_numbers("0,2,5").unwrap();
        assert_eq!(result, vec![0, 2, 5]);
    }

    #[test]
    fn test_parse_mixed_specification() {
        let result = ResourceFactory::parse_device_numbers("0-1,6-7").unwrap();
        assert_eq!(result, vec![0, 1, 6, 7]);
    }

    #[test]
    fn test_parse_complex_specification() {
        let result = ResourceFactory::parse_device_numbers("0-2,5,7-9").unwrap();
        assert_eq!(result, vec![0, 1, 2, 5, 7, 8, 9]);
    }

    #[test]
    fn test_parse_empty_specification() {
        let result = ResourceFactory::parse_device_numbers("");
        assert!(matches!(
            result,
            Err(ResourceFactoryError::EmptySpecification)
        ));
    }

    #[test]
    fn test_parse_invalid_number() {
        let result = ResourceFactory::parse_device_numbers("0,abc,2");
        assert!(matches!(
            result,
            Err(ResourceFactoryError::InvalidNumber(_))
        ));
    }

    #[test]
    fn test_parse_invalid_range() {
        let result = ResourceFactory::parse_device_numbers("5-2");
        assert!(matches!(
            result,
            Err(ResourceFactoryError::InvalidRange { start: 5, end: 2 })
        ));
    }

    #[test]
    fn test_create_gpus_from_spec() {
        let resources =
            ResourceFactory::create_gpus_from_spec("0-1", "Thalys", &[0, 1], |device_id| {
                match device_id {
                    0 => Some("A100".to_string()),
                    1 => Some("A100".to_string()),
                    _ => None,
                }
            })
            .unwrap();

        assert_eq!(resources.len(), 2);
    }

    #[test]
    fn test_create_gpus_from_spec_all() {
        let resources =
            ResourceFactory::create_gpus_from_spec(SPEC_ALL, "Thalys", &[0, 1, 2], |device_id| {
                match device_id {
                    0 | 1 => Some("A100".to_string()),
                    2 => Some("RTX6000".to_string()),
                    _ => None,
                }
            })
            .unwrap();

        assert_eq!(resources.len(), 3);
        assert_eq!(
            resources,
            vec![
                Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
                Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
                Resource::Gpu(Gpu::new("Thalys".to_string(), 2, "RTX6000".to_string())),
            ]
        );
    }

    #[test]
    fn test_create_gpus_device_not_found() {
        let result =
            ResourceFactory::create_gpus_from_spec("0-2", "Thalys", &[0, 1], |device_id| {
                match device_id {
                    0 | 1 => Some("A100".to_string()),
                    _ => None,
                }
            });

        assert!(matches!(
            result,
            Err(ResourceFactoryError::DeviceNotFound { .. })
        ));
    }
}
