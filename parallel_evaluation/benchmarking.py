#!/usr/bin/env python3

import subprocess
import statistics
import os
import csv
from pathlib import Path

# Script to benchmark AWK performance for different AWK versions on the same script (firstly run warmup runs for each AWK version and then multiple benchmarking runs)
#
# ==========================
# Configuration
# ==========================

scripts_dir = "benchmarking_scripts"          # folder containing .awk files
input_file = os.path.expanduser("~/sales_records_large.txt")
output_file = "benchmarking_results/benchmark_results_large.csv"

warmup = 1
runs = 5

base_commands = {
    "awk":     "awk -f {script} {input}",
    "goawk":   "./goawk -f {script} {input}",
    "nawk":    "./onetrueawk/a.out -f {script} {input}",
    "mawk":    "mawk -f {script} {input}",
    "frawk":   "./frawk/target/debug/frawk -f {script} {input}",
    "frawk -p 5":"./frawk/target/debug/frawk -p a -j 5 -f {script} {input}",
    "frawk -p 10":"./frawk/target/debug/frawk -p a -j 10 -f {script} {input}"
}

# ==========================
# Helpers
# ==========================

def parse_elapsed(time_str):
    parts = time_str.strip().split(":")
    if len(parts) == 2:
        m, s = parts
        return int(m) * 60 + float(s)
    elif len(parts) == 3:
        h, m, s = parts
        return int(h) * 3600 + int(m) * 60 + float(s)
    return float(time_str)


def run_command(cmd):
    full_cmd = f"/usr/bin/time -v {cmd}"
    result = subprocess.run(
        full_cmd,
        shell=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True
    )

    data = {}

    for line in result.stderr.splitlines():
        if "User time" in line:
            data["user"] = float(line.split(":")[1])
        elif "System time" in line:
            data["sys"] = float(line.split(":")[1])
        elif "Percent of CPU" in line:
            data["cpu"] = float(line.split(":")[1].replace("%", ""))
        elif "Elapsed (wall clock)" in line:
            data["real"] = parse_elapsed(line.split(": ",1)[1])
        elif "Maximum resident set size" in line:
            data["rss"] = float(line.split(":")[1]) / 1024
        elif "Major (requiring I/O) page faults" in line:
            data["majflt"] = int(line.split(":")[1])
        elif "Minor (reclaiming a frame) page faults" in line:
            data["minflt"] = int(line.split(":")[1])
        elif "Voluntary context switches" in line:
            data["vcsw"] = int(line.split(":")[1])
        elif "Involuntary context switches" in line:
            data["ivcsw"] = int(line.split(":")[1])

    return data


def compute_stats(values):
    return {
        "mean": statistics.mean(values),
        "sd": statistics.stdev(values) if len(values) > 1 else 0
    }

# ==========================
# Benchmark
# ==========================

all_results = []

awk_files = sorted(Path(scripts_dir).glob("*.awk"))

for script_path in awk_files:
    script_name = script_path.name
    print(f"\n=== Benchmarking script: {script_name} ===")

    for cmd_name, cmd_template in base_commands.items():

        cmd = cmd_template.format(script=script_path, input=input_file)

        metrics = {
            "real": [],
            "user": [],
            "sys": [],
            "cpu": [],
            "rss": [],
            "majflt": [],
            "minflt": [],
            "vcsw": [],
            "ivcsw": [],
        }

        # Warmup
        for _ in range(warmup):
            run_command(cmd)

        # Measured runs
        for i in range(runs):
            data = run_command(cmd)
            for k in metrics:
                metrics[k].append(data.get(k, 0))

            print(f"{script_name} | {cmd_name} | run {i} | real={data['real']:.4f}s rss={data['rss']:.2f}MB")

        stats = {k: compute_stats(v) for k, v in metrics.items()}

        row = {
            "script": script_name,
            "cmd": cmd_name
        }

        for metric in metrics:
            row[f"{metric}_mean"] = round(stats[metric]["mean"], 6)
            row[f"{metric}_std"]  = round(stats[metric]["sd"], 6)

        all_results.append(row)

# ==========================
# CSV Output
# ==========================

fieldnames = ["script", "cmd"]

for metric in [
    "real", "user", "sys", "cpu",
    "rss", "majflt", "minflt",
    "vcsw", "ivcsw"
]:
    fieldnames.append(f"{metric}_mean")
    fieldnames.append(f"{metric}_std")

with open(output_file, "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=fieldnames)
    writer.writeheader()
    writer.writerows(all_results)

print("\nCSV written to benchmark_results2.csv")