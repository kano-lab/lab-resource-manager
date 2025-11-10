//! Resource parsing utilities

/// デバイステキストからデバイスIDを抽出
///
/// # Arguments
/// * `device_text` - デバイステキスト (例: "Device 0 (RTX 3090)")
///
/// # Returns
/// 抽出されたデバイスID
///
/// # Errors
/// - フォーマットが不正な場合
/// - デバイスIDが数値でない場合
///
/// # Example
/// ```
/// use lab_resource_manager::interface::slack::parsers::resource::parse_device_id;
///
/// let id = parse_device_id("Device 0 (RTX 3090)").unwrap();
/// assert_eq!(id, 0);
/// ```
pub fn parse_device_id(device_text: &str) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    // "Device "の後の数値を抽出
    let text = device_text.trim();
    if !text.starts_with("Device ") {
        return Err(format!("不正なデバイスフォーマット: {}", device_text).into());
    }

    let after_prefix = &text[7..]; // "Device " の後
    let id_str = after_prefix
        .split_whitespace()
        .next()
        .ok_or_else(|| format!("デバイスIDが見つかりません: {}", device_text))?;

    id_str
        .parse::<u32>()
        .map_err(|e| format!("デバイスIDのパースに失敗: {} ({})", id_str, e).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_id_valid() {
        let id = parse_device_id("Device 0 (RTX 3090)").unwrap();
        assert_eq!(id, 0);

        let id = parse_device_id("Device 123 (A100)").unwrap();
        assert_eq!(id, 123);
    }

    #[test]
    fn test_parse_device_id_invalid_format() {
        let result = parse_device_id("GPU 0");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_device_id_invalid_number() {
        let result = parse_device_id("Device abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_device_id_missing_id() {
        let result = parse_device_id("Device ");
        assert!(result.is_err());
    }
}
