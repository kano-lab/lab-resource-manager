//! Slackアプリの設定（App Manifest）の例を書き出す
//!
//! スラッシュコマンドと必要スコープはコードが決めるため、設定画面に貼る内容も
//! コードから起こす。手で書き写すと、コマンドを足したときに片方だけ古くなる。
//!
//! 出力は`deploy/slack-app-manifest.example.{yaml,json}`と同じ内容であり、
//! 一致することをCIで確かめている。

use clap::{Parser, ValueEnum};
use lab_resource_manager::interface::slack::app_manifest::{AppManifest, ManifestLanguage};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Language {
    /// 公開物として配る既定
    En,
    /// 日本語で運用する導入先向け
    Ja,
}

impl From<Language> for ManifestLanguage {
    fn from(language: Language) -> Self {
        match language {
            Language::En => ManifestLanguage::English,
            Language::Ja => ManifestLanguage::Japanese,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    /// Slackの設定画面が既定で見せる形式
    Yaml,
    /// 機械で扱いやすい形式
    Json,
}

#[derive(Parser)]
#[command(
    about = "SlackアプリのApp Manifestの例を標準出力に書き出す",
    long_about = "出力をSlackの設定画面（Create an app from manifest / App Manifest）に貼ると、\n\
                  このボットが必要とするスラッシュコマンドとスコープが設定される。\n\
                  名前や説明は例なので、導入先に合わせて変えてよい。"
)]
struct Args {
    /// 出力形式
    #[arg(long, value_enum, default_value_t = Format::Yaml)]
    format: Format,

    /// 説明文の言語（コマンドとスコープは言語によらず同じ）
    #[arg(long, value_enum, default_value_t = Language::En)]
    lang: Language,
}

fn main() {
    let args = Args::parse();
    let manifest = AppManifest::example(args.lang.into());

    let rendered = match args.format {
        Format::Yaml => manifest.to_yaml(),
        Format::Json => manifest.to_json(),
    };

    print!("{}", rendered);
}
