$1 == "Europe" { eu[$2]++ } END { for (country in eu) n++; print n }
