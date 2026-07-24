#!/usr/bin/env python3
"""GPUサーバーの実利用状況を共有ディレクトリへJSONで書き出す（cron実行用）

lab-resource-managerのSharedFileResourceUsageObserverがこのJSONを読み取り、
予約と実利用の突合に使う。

依存: nvidia-smi, Python標準ライブラリのみ（pip installは不要）

使い方（cron例、1分間隔）:
    * * * * * /usr/bin/python3 /path/to/gpu_usage_reporter.py \
        --server-name Thalys --output-dir /mnt/shared/lrm-gpu-status
"""
import argparse
import json
import os
import pwd
import subprocess
import time
from datetime import datetime, timezone


def run_nvidia_smi(args):
    result = subprocess.run(
        ["nvidia-smi", *args],
        capture_output=True,
        text=True,
        check=True,
        timeout=10,
    )
    return result.stdout.strip()


def gpu_uuid_to_index():
    output = run_nvidia_smi(["--query-gpu=index,uuid", "--format=csv,noheader"])
    mapping = {}
    for line in output.splitlines():
        if not line.strip():
            continue
        index_str, uuid = (part.strip() for part in line.split(","))
        mapping[uuid] = int(index_str)
    return mapping


def compute_processes():
    output = run_nvidia_smi(["--query-compute-apps=pid,gpu_uuid", "--format=csv,noheader"])
    processes = []
    for line in output.splitlines():
        if not line.strip():
            continue
        pid_str, uuid = (part.strip() for part in line.split(","))
        processes.append((int(pid_str), uuid))
    return processes


def process_start_time(pid):
    """プロセスの起動時刻をUTCのdatetimeで返す。取得できない場合はNoneを返す。

    /proc/<pid>/stat の22番目のフィールド(starttime、起動からの経過tick数)と
    /proc/uptime を組み合わせて計算する。ps -o lstart= はロケール依存で
    パースが不安定なため使わない。
    """
    try:
        with open(f"/proc/{pid}/stat") as f:
            stat = f.read()
        # comm(プロセス名)フィールドは空白や括弧を含みうるため、
        # 最後の')'以降を安全な区切りとして残りのフィールドを分割する
        rest = stat[stat.rfind(")") + 2 :]
        fields = rest.split()
        starttime_ticks = int(fields[19])  # 全体22番目のフィールド(starttime)

        clk_tck = os.sysconf("SC_CLK_TCK")
        with open("/proc/uptime") as f:
            uptime_seconds = float(f.read().split()[0])

        boot_time = time.time() - uptime_seconds
        started = boot_time + starttime_ticks / clk_tck
        return datetime.fromtimestamp(started, tz=timezone.utc)
    except (FileNotFoundError, ProcessLookupError, IndexError, ValueError):
        return None


def process_owner(pid):
    try:
        uid = os.stat(f"/proc/{pid}").st_uid
        return pwd.getpwuid(uid).pw_name
    except (FileNotFoundError, KeyError):
        return None


def collect_entries():
    """(device_number, os_user)ごとに最も古いstarted_atへ集約したエントリ一覧を返す"""
    uuid_to_index = gpu_uuid_to_index()
    grouped = {}

    for pid, uuid in compute_processes():
        device_number = uuid_to_index.get(uuid)
        if device_number is None:
            continue

        owner = process_owner(pid)
        started_at = process_start_time(pid)
        if owner is None or started_at is None:
            continue

        key = (device_number, owner)
        if key not in grouped or started_at < grouped[key]:
            grouped[key] = started_at

    return [
        {
            "device_number": device_number,
            "os_user": os_user,
            "started_at": started_at.isoformat(),
        }
        for (device_number, os_user), started_at in sorted(grouped.items())
    ]


def write_atomic(path, content):
    """同一ディレクトリ内に一時ファイルを書いてrenameし、読み手が中途半端な内容を見ないようにする"""
    tmp_path = f"{path}.tmp.{os.getpid()}"
    with open(tmp_path, "w") as f:
        f.write(content)
    os.replace(tmp_path, path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--server-name", required=True, help='resources.tomlのサーバー名（例: "Thalys"）'
    )
    parser.add_argument("--output-dir", required=True, help="共有ディレクトリ（LRMが読み取る場所）")
    args = parser.parse_args()

    payload = {
        "server": args.server_name,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "processes": collect_entries(),
    }

    os.makedirs(args.output_dir, exist_ok=True)
    output_path = os.path.join(args.output_dir, f"{args.server_name.lower()}.json")
    write_atomic(output_path, json.dumps(payload, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
