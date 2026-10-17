"""Generate factual API lookup tables from probe-fader-curve.lua CSV output."""
import argparse
import csv
import math
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("probe_directory", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()


def read(filename, field, count):
    with (args.probe_directory / filename).open(newline="") as source:
        values = [float(row[field]) for row in csv.DictReader(source)]
    assert len(values) == count
    assert all(math.isfinite(value) for value in values)
    assert all(left <= right for left, right in zip(values, values[1:]))
    return values


forward = read("fader-forward-reference.csv", "slider", 10121)
first_nonzero = next(index for index, value in enumerate(forward) if value > 0)
minimum_db = -1000 + first_nonzero / 10
tables = [
    ("POSITION_TO_DB", read("fader-curve-reference.csv", "db", 10001)),
    ("DB_TO_POSITION", forward[first_nonzero:]),
]
with args.output.open("w") as output:
    output.write("// Generated factual measurements: Linux REAPER 7.82 / Default curve.\n")
    output.write("// Regenerate with scripts/reaper_parity/generate-fader-data.py.\n")
    output.write(f"pub(super) const MIN_FORWARD_DB: f64 = {minimum_db!r};\n")
    for name, values in tables:
        output.write(f"#[rustfmt::skip]\npub(super) static {name}: [f64; {len(values)}] = [\n")
        for index in range(0, len(values), 5):
            output.write("    " + ", ".join(repr(v) for v in values[index:index + 5]) + ",\n")
        output.write("];\n")
