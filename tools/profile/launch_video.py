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
# The result card's name and details, inside the card: the map around it
# loads its tiles for a while after, and that isn't the card appearing.
CARD_ROWS = (int(H * 0.585), int(H * 0.70))
CARD_COLUMNS = (int(W * 0.12), int(W * 0.62))


def frames(path):
    run = subprocess.run(
        ["ffmpeg", "-v", "info", "-i", path, "-vf", f"scale={W}:{H},format=gray,showinfo",
         "-fps_mode", "passthrough", "-f", "image2pipe", "-vcodec", "rawvideo", "-pix_fmt", "gray", "-"],
        capture_output=True, check=True,
    )
    times = [float(t) for t in re.findall(rb"pts_time:([0-9.]+)", run.stderr)]
    size = W * H
    data = run.stdout
    return [(times[i], data[i * size:(i + 1) * size]) for i in range(min(len(times), len(data) // size))]


def differs(a, b, rows=(0, H), columns=(0, W)):
    total = count = 0
    for row in range(*rows):
        lo, hi = row * W + columns[0], row * W + columns[1]
        total += sum(abs(x - y) for x, y in zip(a[lo:hi], b[lo:hi]))
        count += hi - lo
    return total / count


def launch_time(path):
    shown = frames(path)
    home, end = shown[0][1], shown[-1][1]
    start = next(t for t, f in shown if differs(f, home) > 3)
    card = next(t for t, f in shown if t >= start and differs(f, end, CARD_ROWS, CARD_COLUMNS) < 6)
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
