import os
import csv
import requests
import hashlib

# Script to extract AWK file source code from GitHub, based on the names provided in awk_files.csv

# --------------- SETTINGS -----------------
INPUT_FILE = "awk_files.csv"     # your CSV/TSV file
OUTPUT_DIR = "bigquery_files"     # where files will be saved
GITHUB_TOKEN = "..."              # optional but recommended
# ------------------------------------------

headers = {}
if GITHUB_TOKEN:
    headers["Authorization"] = f"Bearer {GITHUB_TOKEN}"

os.makedirs(OUTPUT_DIR, exist_ok=True)

def raw_github_url(repo_name: str, file_path: str) -> str:
    """
    Uses default branch (usually main/master).
    This works reliably via raw.githubusercontent.com.
    """
    return f"https://raw.githubusercontent.com/{repo_name}/HEAD/{file_path}"

def safe_filename(repo_name: str, file_path: str) -> str:
    """
    Prevent collisions by hashing repo+path.
    """
    base = os.path.basename(file_path)
    h = hashlib.sha1(f"{repo_name}/{file_path}".encode()).hexdigest()[:8]
    return f"{repo_name.replace('/', '_')}__{h}__{base}"

files_ok = 0
repos_ok = set()
errors = 0

with open(INPUT_FILE, newline="", encoding="utf-8") as f:
    reader = csv.reader(f)

    # Skip header if present
    header = next(reader)
    if header[0] != "repo_name":
        f.seek(0)
        reader = csv.reader(f, delimiter="\t")

    for repo_name, file_path, stars in reader:
        print(f"Fetching {repo_name}/{file_path}")

        url = raw_github_url(repo_name, file_path)
        resp = requests.get(url, headers=headers)

        if resp.status_code != 200:
            errors += 1
            print(f"  → FAILED ({resp.status_code})")
            continue

        filename = safe_filename(repo_name, file_path)
        out_path = os.path.join(OUTPUT_DIR, filename)

        with open(out_path, "wb") as out:
            out.write(resp.content)

        files_ok += 1
        repos_ok.add(repo_name)

        print("Saved")

print("\n===== SUMMARY =====")
print(f"Files fetched successfully : {files_ok}")
print(f"Repositories involved      : {len(repos_ok)}")
print(f"Errors                     : {errors}")
print("===================")
