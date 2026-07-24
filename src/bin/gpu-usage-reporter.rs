//! GPUサーバーの実利用状況を共有ディレクトリへJSONで書き出すレポーター
//!
//! `SharedFileResourceUsageObserver`が読み取るJSONを、cron等で定期実行して生成する。
//! `nvidia-smi`と`getconf`/`getent`（標準的なLinuxユーティリティ）以外の実行時依存はない。
//!
//! # 使い方（cron例、1分間隔）
//! ```text
//! * * * * * /usr/local/bin/gpu-usage-reporter \
//!     --server-name Thalys --output-dir /mnt/shared/lrm-gpu-status
//! ```

use chrono::{DateTime, Utc};
use clap::Parser;
use lab_resource_manager::prelude::{GpuUsageProcessEntry, GpuUsageReport};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// GPUサーバーの実利用状況を共有ディレクトリへJSONで書き出す
#[derive(Parser)]
struct Args {
    /// config/resources.tomlのサーバー名（例: "Thalys"）
    #[arg(long)]
    server_name: String,

    /// 共有ディレクトリ（LRMが読み取る場所）
    #[arg(long)]
    output_dir: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let report = GpuUsageReport {
        server: args.server_name.clone(),
        generated_at: Utc::now(),
        processes: collect_entries()?,
    };

    std::fs::create_dir_all(&args.output_dir)?;
    let output_path = args
        .output_dir
        .join(format!("{}.json", args.server_name.to_lowercase()));
    let content = serde_json::to_string_pretty(&report)?;
    write_atomic(&output_path, &content)?;

    Ok(())
}

/// (デバイス番号, OSユーザー名)ごとに最も古いプロセス起動時刻へ集約したエントリ一覧を返す
fn collect_entries() -> Result<Vec<GpuUsageProcessEntry>, Box<dyn std::error::Error>> {
    let uuid_to_index = gpu_uuid_to_index()?;
    let clk_tck = clk_tck();
    let mut grouped: HashMap<(u32, String), DateTime<Utc>> = HashMap::new();

    for (pid, uuid) in compute_processes()? {
        let Some(&device_number) = uuid_to_index.get(&uuid) else {
            continue;
        };
        let Some(owner) = process_owner(pid) else {
            continue;
        };
        let Some(started_at) = process_start_time(pid, clk_tck) else {
            continue;
        };

        grouped
            .entry((device_number, owner))
            .and_modify(|existing| {
                if started_at < *existing {
                    *existing = started_at;
                }
            })
            .or_insert(started_at);
    }

    let mut entries: Vec<GpuUsageProcessEntry> = grouped
        .into_iter()
        .map(|((device_number, os_user), started_at)| GpuUsageProcessEntry {
            device_number,
            os_user,
            started_at,
        })
        .collect();
    entries.sort_by(|a, b| {
        a.device_number
            .cmp(&b.device_number)
            .then_with(|| a.os_user.cmp(&b.os_user))
    });

    Ok(entries)
}

fn run_nvidia_smi(args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("nvidia-smi").args(args).output()?;
    if !output.status.success() {
        return Err(format!(
            "nvidia-smi failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// GPU UUID -> デバイス番号(index) の対応表を取得
fn gpu_uuid_to_index() -> Result<HashMap<String, u32>, Box<dyn std::error::Error>> {
    let output = run_nvidia_smi(&["--query-gpu=index,uuid", "--format=csv,noheader"])?;
    let mut map = HashMap::new();

    for line in output.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split(',').map(str::trim);
        let index: u32 = parts.next().ok_or("missing gpu index")?.parse()?;
        let uuid = parts.next().ok_or("missing gpu uuid")?.to_string();
        map.insert(uuid, index);
    }

    Ok(map)
}

/// 現在GPUを使用中のプロセス一覧を (pid, gpu_uuid) のペアで取得
fn compute_processes() -> Result<Vec<(u32, String)>, Box<dyn std::error::Error>> {
    let output = run_nvidia_smi(&["--query-compute-apps=pid,gpu_uuid", "--format=csv,noheader"])?;
    let mut processes = Vec::new();

    for line in output.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split(',').map(str::trim);
        let pid: u32 = parts.next().ok_or("missing pid")?.parse()?;
        let uuid = parts.next().ok_or("missing gpu uuid")?.to_string();
        processes.push((pid, uuid));
    }

    Ok(processes)
}

/// システムのクロック刻み数（1秒あたりのtick数）を取得。失敗時はLinuxの一般的な既定値100を使う
fn clk_tck() -> f64 {
    Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(100.0)
}

/// プロセスの起動時刻をUTCで取得
///
/// `/proc/<pid>/stat`の22番目のフィールド(starttime、起動からの経過tick数)と
/// `/proc/uptime`を組み合わせて計算する。`ps -o lstart=`はロケール依存でパースが
/// 不安定なため使わない。
fn process_start_time(pid: u32, clk_tck: f64) -> Option<DateTime<Utc>> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm(プロセス名)フィールドは空白や括弧を含みうるため、
    // 最後の')'以降を安全な区切りとして残りのフィールドを分割する
    let comm_end = stat.rfind(')')?;
    let fields: Vec<&str> = stat[comm_end + 2..].split_whitespace().collect();
    let starttime_ticks: f64 = fields.get(19)?.parse().ok()?; // 全体22番目のフィールド

    let uptime_content = std::fs::read_to_string("/proc/uptime").ok()?;
    let uptime_seconds: f64 = uptime_content.split_whitespace().next()?.parse().ok()?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    let boot_time = now - uptime_seconds;
    let started = boot_time + starttime_ticks / clk_tck;

    DateTime::from_timestamp(started as i64, 0)
}

/// プロセスのUIDからOSユーザー名を解決
fn process_owner(pid: u32) -> Option<String> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let uid_line = status.lines().find(|l| l.starts_with("Uid:"))?;
    let uid = uid_line.split_whitespace().nth(1)?; // real uid

    let output = Command::new("getent").args(["passwd", uid]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.split(':').next().map(str::to_string)
}

/// 同一ディレクトリ内に一時ファイルを書いてrenameし、読み手が中途半端な内容を見ないようにする
fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("output path has no file name"))?
        .to_string_lossy();
    let tmp_path = path.with_file_name(format!("{}.tmp.{}", file_name, std::process::id()));

    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)
}
