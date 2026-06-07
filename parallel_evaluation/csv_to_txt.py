# Script to convert initial CSV files for benchmarking into txt file
input_file = "sales_records_large.csv"
output_file = "sales_records_large.txt"

with open(input_file, "r", encoding="utf-8") as f:
    content = f.read()

content = content.replace(" ", "_")

content = content.replace(",", " ")


with open(output_file, "w", encoding="utf-8") as f:
    f.write(content)

print("Replacement complete. Saved to", output_file)