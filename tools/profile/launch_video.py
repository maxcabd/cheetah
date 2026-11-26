"""How long an app took to show its first card, from screen recordings
(`simctl io recordVideo`) of launches: from the first frame that differs from
the home screen to the first that matches the end, in the card's part of the
screen. Nothing runs in the app or the test runner meanwhile.

    python3 tools/profile/launch_video.py launch-*.mp4
"""
import re
import statistics
import subprocess
import sys

W, H = 60, 130  # Frames shrunk to this, in gray.
CARD = (int(H * 0.52), int(H * 0.78))  # Rows where the result card sits.


def frames(path):
    run = subprocess.run(
        ["ffmpeg", "-v", "info", "-i", path, "-vf", f"scale={W}:{H},format=gray,showinfo",
         "-vsync", "0", "-f", "rawvideo", "-"],
        capture_output=True, check=True,
    )
    times = [float(t) for t in re.findall(rb"pts_time:([0-9.]+)", run.stderr)]
    size = W * H
    data = run.stdout
    return [(times[i], data[i * size:(i + 1) * size]) for i in range(min(len(times), len(data) // size))]


def differs(a, b, rows=(0, H)):
    lo, hi = rows[0] * W, rows[1] * W
    return sum(abs(x - y) for x, y in zip(a[lo:hi], b[lo:hi])) / (hi - lo)


def launch_time(path):
    shown = frames(path)
    home, end = shown[0][1], shown[-1][1]
    start = next(t for t, f in shown if differs(f, home) > 3)
    card = next(t for t, f in shown if t >= start and differs(f, end, CARD) < 4)
    return card - start


def main() -> int:
    times = []
    for path in sys.argv[1:]:
        try:
            times.append(launch_time(path))
            print(f"{path}: {times[-1]:.3f} s")
        except (StopIteration, subprocess.CalledProcessError, IndexError) as error:
            print(f"{path}: no launch found ({error})")
    if times:
        print(f"median {statistics.median(times):.3f} s of {len(times)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
