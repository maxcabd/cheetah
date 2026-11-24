"""Cheetah against the same screens in UIKit and React Native, from an
xcodebuild test log.

    python3 tools/profile/ios_compare.py /tmp/xcodebuild-perf.log

Prints a Markdown table. Gaps are to UIKit; `noise` is the largest relative
standard deviation, and a gap smaller than it says nothing.
"""
import re
import sys

LINE = re.compile(
    r"test(Cheetah|Native|React|Tuned)([A-Z]\w+)\]' measured \[([^,\]]+), ([^\]]+)\] average: ([\d.]+), "
    r"relative standard deviation: ([\d.]+)%"
)
COLUMNS = [("Cheetah", "Cheetah"), ("Native", "UIKit"), ("React", "React Native"), ("Tuned", "RN, native sheet and paging")]


def read(path):
    found = {}
    for line in open(path, encoding="utf-8", errors="replace"):
        match = LINE.search(line)
        if not match:
            continue
        side, test, metric, unit, average, noise = match.groups()
        # "CPU Time (cheetah)" names the process: the column says that. A
        # "Duration (apply)" names the signpost.
        if not metric.startswith("Duration"):
            metric = re.sub(r" \([a-z][\w ]*\)$", "", metric)
        # The simulator has no hardware counters; they read 0.
        if float(average) > 0:
            found.setdefault((test, metric, unit), {})[side] = (float(average), float(noise))
    return found


def table(found):
    sides = [(key, name) for key, name in COLUMNS if any(key in row for row in found.values())]
    lines = [
        "| Test | Metric | " + " | ".join(name for _, name in sides) + " | Noise |",
        "|---|---|" + "---|" * (len(sides) + 1),
    ]
    for (test, metric, unit), row in sorted(found.items()):
        native = row.get("Native")
        cells = []
        for key, _ in sides:
            if key not in row:
                cells.append("-")
                continue
            value = row[key][0]
            cell = f"{value * 1000:.0f} ms" if unit == "s" and value < 0.1 else f"{value:.2f} {unit}"
            if native and key != "Native":
                cell += f" ({(row[key][0] / native[0] - 1) * 100:+.0f}%)"
            cells.append(cell)
        noise = max(value[1] for value in row.values())
        lines.append(f"| {test} | {metric} | " + " | ".join(cells) + f" | {noise:.0f}% |")
    return "\n".join(lines)


def main() -> int:
    found = read(sys.argv[1])
    if not found:
        print("No measurements in the log.")
        return 1
    print(table(found))
    return 0


if __name__ == "__main__":
    sys.exit(main())
