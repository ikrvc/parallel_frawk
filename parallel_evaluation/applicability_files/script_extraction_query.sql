-- Query to extract AWK files from repos with watch_count > 10 from public github BigQuery dataset (Results are now stored in awk_files.csv)
SELECT
    f.repo_name,
    f.path AS file_path,
    r.watch_count AS stars
FROM
    `bigquery-public-data.github_repos.files` AS f
        JOIN
    `bigquery-public-data.github_repos.sample_repos` AS r
    ON
        f.repo_name = r.repo_name
WHERE
    ENDS_WITH(f.path, '.awk')
  AND r.watch_count >= 10
ORDER BY
    r.watch_count DESC,
    f.repo_name,
    f.path;
