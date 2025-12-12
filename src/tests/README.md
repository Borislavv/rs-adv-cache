# E2E Tests: Advanced Cache / Proxy

## How to run
```bash
cargo test
```

The harness boots **upstream** and **cache** in `init()` once, then reuses them across tests. Upstream exposes deterministic JSON (stable field order) and diagnostic headers.

## What’s covered
- **Key isolation**: response body strictly depends on `(path, whitelisted query, normalized Accept-Encoding)`; no leakage across combinations.
- **Whitelists**: non-whitelisted query params and request headers don’t affect body/keys.
- **Canonicalization**: bracketed keys and percent-encoding variants (`+/%20`, `%2f/%2F`, raw UTF-8 vs `%XX`, encoded keys) behave equivalently.
- **Negative changes**: changing exactly one whitelisted key changes the body.
- **Order-insensitivity**: parameter order does not affect body.
- **Headers (cache mode)**: hop-by-hop headers are stripped; baseline whitelisted headers like `Content-Type` are present.
- **Headers (proxy mode)**: hop-by-hop headers are stripped; do **not** require `X-*` to pass (implementation-specific).
- **Double-encoding**: `%252F` is **not** equivalent to `%2F` (single decode behaviour) — prevents double-decode pitfalls.

## Notes
- Cache vs proxy mode is toggled either via config (`cache.enabled: true/false`) or control endpoints (`/advcache/bypass/on`, `/advcache/bypass/off`). Tests use the control endpoints for isolated checks and then restore the original mode.