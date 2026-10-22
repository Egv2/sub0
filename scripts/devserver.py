"""Dev-only static file server WITH HTTP Range support (206).

Python's built-in `http.server` rejects Range requests, which makes browsers
fail to seek in <video> elements (the media pipeline resets to 0:00 on every
seek). This server answers Range correctly so `docs/spike-data/preview.html`
and `data/*/preview.html` can be previewed over HTTP during development.

Not part of the subtitler runtime (which invokes only ffmpeg/ffprobe/yt-dlp);
this is a development aid.

Usage:  python3 scripts/devserver.py [port]     # serves the repo root
"""
import os
import re
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer


class RangeHandler(SimpleHTTPRequestHandler):
    def send_head(self):
        path = self.translate_path(self.path)
        if os.path.isdir(path):
            return super().send_head()
        try:
            f = open(path, "rb")
        except OSError:
            self.send_error(404, "Not Found")
            return None
        try:
            size = os.fstat(f.fileno()).st_size
            m = re.match(r"bytes=(\d*)-(\d*)$", self.headers.get("Range", "") or "")
            if not m:
                self.send_response(200)
                self.send_header("Content-Length", str(size))
                self.send_header("Accept-Ranges", "bytes")
                self.send_header("Content-Type", self.guess_type(path))
                self.end_headers()
                return f
            start = int(m.group(1)) if m.group(1) else 0
            end = min(int(m.group(2)) if m.group(2) else size - 1, size - 1)
            if start > end or start >= size:
                self.send_error(416, "Requested Range Not Satisfiable")
                f.close()
                return None
            self.send_response(206)
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
            self.send_header("Content-Length", str(end - start + 1))
            self.send_header("Accept-Ranges", "bytes")
            self.send_header("Content-Type", self.guess_type(path))
            self.end_headers()
            f.seek(start)
            left = end - start + 1

            class _RangeFile:
                def read(self, n=-1):
                    nonlocal left, f
                    if left <= 0:
                        return b""
                    n = left if n < 0 else min(n, left)
                    data = f.read(n)
                    left -= len(data)
                    return data

                def close(self):
                    f.close()

            return _RangeFile()
        except Exception:
            f.close()
            raise

    def log_message(self, fmt, *args):
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8760
    print(f"serving repo root on http://127.0.0.1:{port} (Range-capable)")
    ThreadingHTTPServer(("127.0.0.1", port), RangeHandler).serve_forever()
