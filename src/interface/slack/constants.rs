//! Slackインターフェース定数
//!
//! Slackインタラクションで使用されるアクションID、コールバックID、その他の定数

// モーダルコールバックID
/// メールアドレス登録モーダルのコールバックID
pub const CALLBACK_REGISTER_EMAIL: &str = "register_email";
/// ユーザーリンクモーダルのコールバックID
pub const CALLBACK_LINK_USER: &str = "link_user";

// アクションID - メールアドレス登録モーダル
/// メールアドレス入力フィールドのアクション
pub const ACTION_EMAIL_INPUT: &str = "email_input";

// アクションID - ユーザーリンクモーダル
/// ユーザー選択フィールドのアクション
pub const ACTION_USER_SELECT: &str = "user_select";
/// リンク先メールアドレス入力フィールドのアクション
pub const ACTION_LINK_EMAIL_INPUT: &str = "link_email_input";
