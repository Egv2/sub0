# Third-party notices

Binaries built from this repository include the components below. Rust crate
dependencies keep their own licenses (see `Cargo.lock`, or run
`cargo about` / `cargo license` for a full report).

## ONNX Runtime 1.24.2

Linked statically through the [`ort`](https://github.com/pykeio/ort) crate,
which downloads a prebuilt `libonnxruntime.a` from `cdn.pyke.io` at build
time. Offline or reproducible builds can point `ORT_LIB_LOCATION` at a local
ONNX Runtime build instead.

ONNX Runtime itself bundles further components (protobuf, abseil, re2,
flatbuffers, …); their notices are in
[ThirdPartyNotices.txt](https://github.com/microsoft/onnxruntime/blob/v1.24.2/ThirdPartyNotices.txt)
and must ship with any redistributed binary.

```text
MIT License

Copyright (c) Microsoft Corporation

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Silero VAD model

`silero_vad.onnx`, embedded via the
[`silero`](https://github.com/Findit-AI/silero) crate (MIT OR Apache-2.0).
Upstream: <https://github.com/snakers4/silero-vad>.

```text
MIT License

Copyright (c) 2020-present Silero Team

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## External tools

`ffmpeg`, `ffprobe` and `yt-dlp` are not bundled; they are run from `PATH`.
If a release ever ships them as sidecars, their licenses apply (FFmpeg is
LGPL-2.1+ or GPL depending on the build; yt-dlp is Unlicense).
