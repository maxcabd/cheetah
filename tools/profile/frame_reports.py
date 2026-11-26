"""Cheetah's frame reports from a simulator's log (CHEETAH_FRAME_LOG=1), as
a table: where each kind of slow frame's time goes on the main thread.

    xcrun simctl spawn booted log show --last 30m --style compact \\
        --predicate 'subsystem == "dev.cheetah.host" AND category == "report"' > frames.log
    python3 tools/profile/frame_reports.py frames.log
"""
import json
import statistics
import sys

PARTS = ["create", "props", "children", "layout", "remove", "insert", "glass", "modules", "calls", "map"]


def main() -> int:
    frames = []
    for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
        if "frame {" in line:
            try:
                frames.append(json.loads(line[line.index("frame {") + 6:]))
            except ValueError:
                pass
    # Frames that did something worth looking at, grouped by size.
    main = lambda f: f.get("apply_us", 0) + f.get("uikit_layout_us", 0) + f.get("commit_us", 0)  # noqa: E731
    big = [f for f in frames if main(f) >= 20_000]
    if not big:
        print(f"{len(frames)} frames, none over 20 ms.")
        return 0
    print(f"{len(frames)} frames; {len(big)} over 20 ms. Medians of those, in ms:\n")
    print("| ops | frames | apply | UIKit layout | commit | " + " | ".join(PARTS) + " |")
    print("|---|---|---|---|---|" + "---|" * len(PARTS))
    groups = {}
    for f in big:
        groups.setdefault(f.get("ops", 0), []).append(f)
    for ops, group in sorted(groups.items(), key=lambda g: -len(g[1]))[:8]:
        med = lambda key: statistics.median(f.get(key, 0) for f in group) / 1000  # noqa: E731
        cells = [f"{med(p + '_us'):.1f}" for p in PARTS]
        print(f"| {ops} | {len(group)} | {med('apply_us'):.1f} | {med('uikit_layout_us'):.1f} | {med('commit_us'):.1f} | " + " | ".join(cells) + " |")
    named = {}
    for f in big:
        for name, us in (f.get("named") or {}).items():
            named.setdefault(name, []).append(us)
    if named:
        print("\n| native work | frames | median ms | max ms |\n|---|---|---|---|")
        for name, values in sorted(named.items(), key=lambda n: -max(n[1]))[:12]:
            print(f"| {name} | {len(values)} | {statistics.median(values) / 1000:.1f} | {max(values) / 1000:.1f} |")
    return 0


if __name__ == "__main__":
    sys.exit(main())
