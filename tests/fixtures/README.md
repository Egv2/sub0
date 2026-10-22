# Test fixtures

Phase 1+ will commit here:

- one short (≤ 30 s) audio/video sample with known speech (CC-licensed),
- golden SRT files per profile (`insta`),
- hostile LLM response fixtures (wiremock).

Binary fixtures stay tiny; the >1 h VAD verification file is generated or
fetched by an ignored-by-default test, never committed.
