# Subtitle standards — value provenance (§8, §17.5)

Every numeric limit shipped in `profiles/*.toml` gets a row here: value,
source, verification status. Nothing in stage code is a magic number.

| Profile | Key | Value | Status | Source |
|---|---|---|---|---|
| netflix | max_chars_per_line | 42 | [Likely] | Netflix TTSG Latin-script default — re-verify at <https://partner.help.netflix.com/en/node/249> |
| netflix | max_lines | 2 | [Likely] | Netflix TTSG |
| netflix | max_cps_adult | 17 | [Likely] | Netflix TTSG |
| netflix | max_cps_children | 13 | [Verify] | Netflix TTSG |
| netflix | min_duration_frames | 20 (≈5/6 s @24fps) | [Likely] | Netflix TTSG |
| netflix | max_duration_ms | 7000 | [Likely] | Netflix TTSG |
| netflix | min_gap_frames | 2 | [Likely] | Netflix TTSG |
| netflix | pause_split_ms | 500 | [Verify] | internal default, tune on data |
| bbc | — | — | [Verify] | BBC Subtitle Guidelines, Phase 4 |
| ebu | — | — | [Verify] | EBU-TT-D docs, Phase 4 |

Per-language overrides (CJK, Arabic/RTL, Thai …) must **not** inherit Latin
defaults; Netflix publishes per-language guides — add rows per language in
Phase 4.
