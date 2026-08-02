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
//!
//! 稼働率は`--sample-seconds`のあいだ毎秒サンプリングし、その最大値を報告する。
//! `nvidia-smi`が返す稼働率はごく短い期間の瞬間値であり、一度読むだけでは
//! 途切れがちな計算を取りこぼすため、実行のたびに窓を取って見張る。
//! この窓のぶんだけ実行時間が延びるので、`--sample-seconds`は実行間隔より短くする。

use chrono::{DateTime, Utc};
use clap::Parser;
use lab_resource_manager::prelude::{GpuUsageDeviceEntry, GpuUsageProcessEntry, GpuUsageReport};
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

    /// 稼働率を見張る秒数（この長さだけ毎秒サンプリングし、最大値を報告する）
    ///
    /// 実行間隔より短くする。長くするほど計算の取りこぼしは減る。
    #[arg(long, default_value_t = 30)]
    sample_seconds: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 稼働率の窓を先に取り、プロセスの一覧はその直後の姿を書く
    let devices = sample_device_activity(args.sample_seconds);
    let report = GpuUsageReport {
        server: args.server_name.clone(),
        generated_at: Utc::now(),
        processes: collect_entries()?,
        devices,
    };

    std::fs::create_dir_all(&args.output_dir)?;
    let output_path = args
        .output_dir
        .join(format!("{}.json", args.server_name.to_lowercase()));
    let content = serde_json::to_string_pretty(&report)?;
    write_atomic(&output_path, &content)?;

    Ok(())
}

/// 1つの(デバイス, 利用者)にまとめられた利用の姿
struct GroupedUsage {
    started_at: DateTime<Utc>,
    used_memory_mib: Option<u64>,
}

/// (デバイス番号, OSユーザー名)ごとに、最も古い起動時刻とメモリの合計へ集約する
fn collect_entries() -> Result<Vec<GpuUsageProcessEntry>, Box<dyn std::error::Error>> {
    let uuid_to_index = gpu_uuid_to_index()?;
    let clk_tck = clk_tck();
    let mut grouped: HashMap<(u32, String), GroupedUsage> = HashMap::new();

    for process in compute_processes()? {
        let Some(&device_number) = uuid_to_index.get(&process.gpu_uuid) else {
            continue;
        };
        let Some(owner) = process_owner(process.pid) else {
            continue;
        };
        let Some(started_at) = process_start_time(process.pid, clk_tck) else {
            continue;
        };

        grouped
            .entry((device_number, owner))
            .and_modify(|existing| {
                existing.started_at = existing.started_at.min(started_at);
                existing.used_memory_mib =
                    add_memory(existing.used_memory_mib, process.used_memory_mib);
            })
            .or_insert(GroupedUsage {
                started_at,
                used_memory_mib: process.used_memory_mib,
            });
    }

    let mut entries: Vec<GpuUsageProcessEntry> = grouped
        .into_iter()
        .map(|((device_number, os_user), usage)| GpuUsageProcessEntry {
            device_number,
            os_user,
            started_at: usage.started_at,
            used_memory_mib: usage.used_memory_mib,
        })
        .collect();
    entries.sort_by(|a, b| {
        a.device_number
            .cmp(&b.device_number)
            .then_with(|| a.os_user.cmp(&b.os_user))
    });

    Ok(entries)
}

/// 同じ利用者の別プロセスが確保している分を足し合わせる
///
/// ひとつでも読み出せないプロセスがあれば、合計そのものを問えないものとして扱う。
/// 読めた分だけを足した数を「確保している量」として伝えると、実際より少なく見える。
fn add_memory(accumulated: Option<u64>, addition: Option<u64>) -> Option<u64> {
    Some(accumulated? + addition?)
}

/// 稼働率を`sample_seconds`のあいだ毎秒読み、デバイスごとに集約する
///
/// 一度も読み出せなかった場合は空を返す。稼働状況の載っていないレポートは
/// 「計算していたかを問えない」ものとして扱われるため、ここで手を止めるより、
/// 分かる範囲を書き出したほうがよい。
///
/// `nvidia-smi`自身のループ（`--loop`）は打ち切る手立てが揃った環境ばかりではないため、
/// 単発の読み出しをこちらで繰り返す。
fn sample_device_activity(sample_seconds: u32) -> Vec<GpuUsageDeviceEntry> {
    let mut readings = String::new();

    for taken in 0..sample_seconds.max(1) {
        if taken > 0 {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        match query_activity() {
            Ok(output) => {
                readings.push_str(&output);
                readings.push('\n');
            }
            Err(e) => {
                eprintln!("reading gpu activity failed ({e}); reporting with what was read so far");
                break;
            }
        }
    }

    parse_activity_samples(&readings)
}

/// 稼働率をいま一度読み出す
fn query_activity() -> Result<String, Box<dyn std::error::Error>> {
    run_nvidia_smi(&[
        "--query-gpu=index,utilization.gpu",
        "--format=csv,noheader,nounits",
    ])
}

/// サンプルの並びを、デバイスごとの最大稼働率へ集約する
///
/// 稼働率を読み出せないGPU（`[N/A]`を返すもの）の行は落とす。0%として扱うと、
/// 計算していないことの証拠として読まれてしまう。
fn parse_activity_samples(output: &str) -> Vec<GpuUsageDeviceEntry> {
    let mut peaks: HashMap<u32, u32> = HashMap::new();

    for line in output.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split(',').map(str::trim);
        let Some(Ok(device_number)) = parts.next().map(str::parse::<u32>) else {
            continue;
        };
        let Some(Ok(utilization)) = parts.next().map(str::parse::<u32>) else {
            continue;
        };

        peaks
            .entry(device_number)
            .and_modify(|peak| *peak = (*peak).max(utilization))
            .or_insert(utilization);
    }

    let mut entries: Vec<GpuUsageDeviceEntry> = peaks
        .into_iter()
        .map(
            |(device_number, peak_utilization_percent)| GpuUsageDeviceEntry {
                device_number,
                peak_utilization_percent,
            },
        )
        .collect();
    entries.sort_by_key(|entry| entry.device_number);

    entries
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

/// GPUを使用中の1プロセス
struct ComputeProcess {
    pid: u32,
    gpu_uuid: String,
    /// このプロセスが確保しているメモリ量（MiB、読み出せなければ`None`）
    used_memory_mib: Option<u64>,
}

/// 現在GPUを使用中のプロセス一覧を取得
fn compute_processes() -> Result<Vec<ComputeProcess>, Box<dyn std::error::Error>> {
    let output = run_nvidia_smi(&[
        "--query-compute-apps=pid,gpu_uuid,used_gpu_memory",
        "--format=csv,noheader,nounits",
    ])?;
    let mut processes = Vec::new();

    for line in output.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split(',').map(str::trim);
        let pid: u32 = parts.next().ok_or("missing pid")?.parse()?;
        let gpu_uuid = parts.next().ok_or("missing gpu uuid")?.to_string();
        // メモリを読み出せないプロセスがあっても、誰がどのGPUに乗っているかは伝える
        let used_memory_mib = parts.next().and_then(|raw| raw.parse::<u64>().ok());

        processes.push(ComputeProcess {
            pid,
            gpu_uuid,
            used_memory_mib,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_highest_reading_in_the_window_is_what_gets_reported() {
        // 1秒ごとに2台分、3回サンプリングした出力
        let output = "\
0, 0
1, 0
0, 74
1, 0
0, 0
1, 0";

        let entries = parse_activity_samples(output);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].device_number, 0);
        assert_eq!(
            entries[0].peak_utilization_percent, 74,
            "谷で読んだ0%を根拠にすると、途切れがちな計算を止まっていると読んでしまう"
        );
        assert_eq!(entries[1].peak_utilization_percent, 0);
    }

    #[test]
    fn a_device_that_does_not_report_its_utilization_is_left_out() {
        let output = "\
0, [N/A]
1, 30";

        let entries = parse_activity_samples(output);

        assert_eq!(
            entries.len(),
            1,
            "読み出せなかった稼働率を0%として扱ってはいけない"
        );
        assert_eq!(entries[0].device_number, 1);
    }

    #[test]
    fn an_empty_reading_yields_nothing() {
        assert!(parse_activity_samples("").is_empty());
    }

    #[test]
    fn memory_across_a_users_processes_is_summed() {
        assert_eq!(add_memory(Some(1_000), Some(2_000)), Some(3_000));
    }

    #[test]
    fn memory_that_cannot_be_read_makes_the_whole_sum_unanswerable() {
        assert_eq!(
            add_memory(Some(1_000), None),
            None,
            "読めた分だけ足した数を確保量として伝えると、実際より少なく見える"
        );
        assert_eq!(add_memory(None, Some(2_000)), None);
    }
}
