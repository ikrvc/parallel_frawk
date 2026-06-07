import os
import subprocess
import csv
from collections import Counter

# Script to check parallelizability of extracted AWK scripts

AWK_DIR = "bigquery_files"
CMD = "frawk"
OUTPUT_CSV = "strict_output_analysis.csv"

results = []
status_counter = Counter()

for filename in os.listdir(AWK_DIR):
    if not filename.endswith(".awk"):
        continue

    filepath = os.path.join(AWK_DIR, filename)
    cmd = [CMD, "-f", filepath, "--check-parallel", "-p", "a", "-j", "5"]

    try:
        proc = subprocess.run(cmd, capture_output=True, text=True)
        output = proc.stdout.strip()
        error_output = proc.stderr.strip()

        if "No main statements" in output or "No main statements" in error_output:
            status = "No main statements"
        elif "Parallelizable: true" in output:
            status = "Parallelizable: true"
        elif "Parallelizable: false" in output:
            status = "Parallelizable: false"
        else:
            status = "Error"

    except Exception:
        status = "Error"

    status_counter[status] += 1
    results.append((filename, status))
    print(status)

with open(OUTPUT_CSV, mode="w", newline="", encoding="utf-8") as f:
    writer = csv.writer(f)
    writer.writerow(["filename", "status"])  # header
    writer.writerows(results)

print(f"Results saved to {OUTPUT_CSV}")
for status, count in status_counter.items():
    print(f"{status}: {count}")