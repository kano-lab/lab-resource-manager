//! TailwindのCSSを生成する。
//!
//! 生成物は`$OUT_DIR/tailwind.css`に置かれ、Web画面のレイアウトが`include_str!`で
//! 読み込んでバイナリに埋め込む。Topcoat標準のアセットバンドル（実行時にディスクから
//! 読む方式）を使わないため、配布物は単一バイナリのままで済む。

fn main() {
    #[cfg(feature = "web")]
    topcoat::tailwind::BuildConfig::new()
        .render()
        .expect("TailwindのCSS生成に失敗しました");
}
