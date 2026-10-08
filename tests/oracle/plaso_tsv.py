"""plaso's events (psort -o json_line) as sorted TSV lines, one per event.

    python3 -I tests/oracle/plaso_tsv.py office.jsonl office_written.jsonl > tests/oracle/plaso.tsv

Columns: file name, data type, timestamp description, timestamp
(microseconds), then every other value plaso gives as name=value, sorted by
name. Booleans are written true/false; bytes (plaso writes them as a Python
literal) as "<n> bytes".
"""

import ast
import json
import os
import sys

# Event bookkeeping, not values read from the file.
SKIPPED = {
    "__container_type__",
    "__type__",
    "_event_values_hash",
    "data_type",
    "date_time",
    "display_name",
    "filename",
    "hostname",
    "inode",
    "message",
    "parser",
    "pathspec",
    "sha256_hash",
    "tag",
    "timestamp",
    "timestamp_desc",
    "username",
}


def text(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str) and value.startswith("b'"):
        return f"{len(ast.literal_eval(value))} bytes"
    return str(value)


def line(event):
    values = sorted(
        f"{name}={text(value)}" for name, value in event.items() if name not in SKIPPED
    )
    fields = [
        os.path.basename(event["pathspec"]["location"]),
        event["data_type"],
        event["timestamp_desc"],
        str(event["timestamp"]),
    ]
    return "\t".join(fields + values)


def main():
    lines = []
    for path in sys.argv[1:]:
        with open(path, encoding="utf-8") as handle:
            lines.extend(line(json.loads(row)) for row in handle if row.strip())
    for row in sorted(lines):
        print(row)


if __name__ == "__main__":
    main()
