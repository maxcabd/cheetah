"""Cheetah against its UIKit twin, from an xcodebuild test log.

    python3 tools/profile/ios_compare.py /tmp/xcodebuild-perf.log

Prints a Markdown table. `noise` is the larger relative standard deviation: a
gap smaller than it says nothing.
"""
import re
import sys

LINE = re.compile(
    r"test(Cheetah|Native)(\w+)\]' measured \[([^,\]]+), ([^\]]+)\] average: ([\d.]+), "
    r"relative standard deviation: ([\d.]+)%"
)


def main() -> int:
    found = {}
    for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
        match = LINE.search(line)
        if match:
            side, test, metric, unit, average, noise = match.groups()
            found.setdefault((test, metric, unit), {})[side] = (float(average), float(noise))
    if not found:
        print("No measurements in the log.")
        return 1
    print("| Test | Metric | Cheetah | UIKit | Gap | Noise |")
    print("|---|---|---|---|---|---|")
    for (test, metric, unit), sides in sorted(found.items()):
        cheetah, native = sides.get("Cheetah"), sides.get("Native")
        cell = lambda side: f"{side[0]:.3f} {unit}" if side else "-"
        gap = f"{(cheetah[0] / native[0] - 1) * 100:+.0f}%" if cheetah and native and native[0] else "-"
        noise = max(side[1] for side in sides.values())
        print(f"| {test} | {metric} | {cell(cheetah)} | {cell(native)} | {gap} | {noise:.0f}% |")
    return 0


if __name__ == "__main__":
    sys.exit(main())
