use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};

/// 全デバイス指定を表すデバイス指定記法
pub const SPEC_ALL: &str = "all";

/// デバイス指定記法と Resource オブジェクトを相互変換するファクトリ
///
/// サーバーのデバイスカタログ `&[(u32, String)]` を共通の入力として、
/// パース（spec → Resources）とフォーマット（Resources → spec）の対称な操作を提供する。
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
    /// * `server_devices` - サーバーのデバイスカタログ（ID, モデル名）
    pub fn create_gpus_from_spec(
        spec: &str,
        server_name: &str,
        server_devices: &[(u32, String)],
    ) -> Result<Vec<Resource>, ResourceFactoryError> {
        let device_numbers = if spec == SPEC_ALL {
            server_devices.iter().map(|(id, _)| *id).collect()
        } else {
            Self::parse_device_numbers(spec)?
        };

        let mut resources = Vec::new();
        for device_num in device_numbers {
            let model = server_devices
                .iter()
                .find(|(id, _)| *id == device_num)
                .map(|(_, model)| model.clone())
                .ok_or_else(|| ResourceFactoryError::DeviceNotFound {
                    server: server_name.to_string(),
                    device_id: device_num,
                })?;

            resources.push(Resource::Gpu(Gpu::new(
                server_name.to_string(),
                device_num,
                model,
            )));
        }

        Ok(resources)
    }

    /// Resource のリストからデバイス指定記法を生成
    ///
    /// サーバーの全デバイスが含まれている場合は "all" を返す。
    /// それ以外はデバイス番号のカンマ区切り（例: "0,1,5,7"）を返す。
    ///
    /// # Arguments
    /// * `resources` - GPUリソースのリスト
    /// * `server_devices` - サーバーのデバイスカタログ（ID, モデル名）
    pub fn format_gpu_spec(
        resources: &[Resource],
        server_devices: &[(u32, String)],
    ) -> Option<String> {
        let mut device_numbers: Vec<u32> = resources
            .iter()
            .filter_map(|r| match r {
                Resource::Gpu(gpu) => Some(gpu.device_number()),
                _ => None,
            })
            .collect();

        if device_numbers.is_empty() {
            return None;
        }

        if device_numbers.len() == server_devices.len()
            && server_devices
                .iter()
                .all(|(id, _)| device_numbers.contains(id))
        {
            return Some(SPEC_ALL.to_string());
        }

        device_numbers.sort_unstable();
        Some(
            device_numbers
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    }

    /// デバイス番号のパース（内部ヘルパー）
    ///
    /// "0-2,5,7-9" のような記法をパースして、個別のデバイス番号のリストに展開する。
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

    fn test_devices() -> Vec<(u32, String)> {
        vec![
            (0, "A100".to_string()),
            (1, "A100".to_string()),
            (2, "RTX6000".to_string()),
        ]
    }

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
        let devices = test_devices();
        let resources = ResourceFactory::create_gpus_from_spec("0-1", "Thalys", &devices).unwrap();

        assert_eq!(resources.len(), 2);
    }

    #[test]
    fn test_create_gpus_from_spec_all() {
        let devices = test_devices();
        let resources =
            ResourceFactory::create_gpus_from_spec(SPEC_ALL, "Thalys", &devices).unwrap();

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
        let devices = vec![(0, "A100".to_string()), (1, "A100".to_string())];
        let result = ResourceFactory::create_gpus_from_spec("0-2", "Thalys", &devices);

        assert!(matches!(
            result,
            Err(ResourceFactoryError::DeviceNotFound { .. })
        ));
    }

    #[test]
    fn test_format_gpu_spec_all_devices() {
        let devices = test_devices();
        let resources =
            ResourceFactory::create_gpus_from_spec(SPEC_ALL, "Thalys", &devices).unwrap();

        assert_eq!(
            ResourceFactory::format_gpu_spec(&resources, &devices),
            Some("all".to_string())
        );
    }

    #[test]
    fn test_format_gpu_spec_partial_devices() {
        let devices = test_devices();
        let resources = ResourceFactory::create_gpus_from_spec("0,2", "Thalys", &devices).unwrap();

        assert_eq!(
            ResourceFactory::format_gpu_spec(&resources, &devices),
            Some("0,2".to_string())
        );
    }

    #[test]
    fn test_format_gpu_spec_empty() {
        let devices = test_devices();
        assert_eq!(ResourceFactory::format_gpu_spec(&[], &devices), None);
    }
}
