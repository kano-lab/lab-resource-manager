//! 層の依存の向きを確かめるテスト
//!
//! ドメインとユースケースは別クレート（lab-resource-manager-core）にあり、
//! アプリケーションの層を参照できないことはコンパイラが保証する。
//! アプリケーションの中では、外から呼ばれる入口（interface）が、こちらから呼ぶ出口
//! （infrastructure）を使う。逆向きの参照は同じクレートの中ではコンパイラが止めないため、
//! ここで確かめる。

use std::fs;
use std::path::{Path, PathBuf};

/// ディレクトリ以下のRustソースをすべて集める
fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}を読めません: {e}", dir.display()))
        .map(|entry| entry.expect("ディレクトリの項目を読めません").path())
        .flat_map(|path| {
            if path.is_dir() {
                rust_sources(&path)
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

#[test]
fn infrastructure_does_not_depend_on_interface() {
    let infrastructure = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/infrastructure");

    let offending: Vec<String> = rust_sources(&infrastructure)
        .into_iter()
        .filter(|path| {
            fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("{}を読めません: {e}", path.display()))
                .contains("crate::interface")
        })
        .map(|path| path.display().to_string())
        .collect();

    assert!(
        offending.is_empty(),
        "infrastructureがinterfaceを参照している。両者が共有する値はinfrastructure側に置く: {offending:?}"
    );
}
