{ split($6, a, "/"); d = sprintf("%d%02d%02d", a[3], a[1], a[2]); if (d >= "20140301" && d <= "20150331") n++ } END { print n }
