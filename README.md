# Advanced Cache (advCache)
[![Rust Version](https://img.shields.io/static/v1?label=Rust&message=1.82%2B&logo=rust&color=000000)](https://www.rust-lang.org/tools/install) [![Coverage](https://img.shields.io/codecov/c/github/Borislavv/adv-cache?label=coverage)](https://codecov.io/gh/Borislavv/adv-cache) [![License](https://img.shields.io/badge/License-Apache--2.0-green.svg)](./LICENSE)

High‑performance **in‑memory HTTP cache & reverse proxy** for latency‑sensitive workloads. Implemented in Rust on top of `tokio` + `axum`, with sharded storage, TinyLFU admission, background refresh, upstream controls, and minimal‑overhead tracing/metrics (Prometheus + OpenTelemetry).

---

## Why advCache?
- **Throughput**: 160–170k RPS locally; ~250k RPS sustained on 24‑core bare‑metal with a 50GB cache.
- **Memory safety**: 1.5–3GB overhead at 50GB (no traces); ~7GB at 100% OTEL sampling.
- **Hot path discipline**: zero allocations, sharded counters, per‑shard LRU, TinyLFU admission.
- **Control plane**: runtime API for toggles (admission, eviction, refresh, compression, tracing).
- **Traces & Metrics**: Prometheus/VictoriaMetrics metrics + OpenTelemetry tracing.
- **Kubernetes‑friendly**: health probes, config via ConfigMap, Docker image.

---

## Quick start (production‑capable starter config)
**Edit the CHANGEME fields and run.** This is a complete config based on `advcache.cfg.yaml`, trimmed for a fast start but **fully runnable**.

```yaml
cache:
  env: prod
  enabled: true
  logs:
    level: debug
  runtime:
    num_cpus: 0
  api:
    name: adv_cache
    port: '8020             # <-- CHANGEME: API port to listen on'
  upstream:
    backend:
      id: example-upstream-backend
      enabled: true
      policy: deny
      host: service-example:8080
      scheme: http
      rate: 15000
      concurrency: 4096
      timeout: 10s
      max_timeout: 1m
      use_max_timeout_header: ''
      healthcheck: /healthcheck
      addr: http://127.0.0.1:8081  # <-- CHANGEME: your upstream origin URL
      health_path: /health
  compression:
    enabled: true
    level: 1
  data:
    dump:
      enabled: true
      dump_dir: public/dump
      dump_name: cache.dump
      crc32_control_sum: true
      max_versions: 3
      gzip: false
    mock:
      enabled: false
      length: 1000000
  storage:
    mode: listing
    size: 53687091200
  admission:
    enabled: true
    capacity: 2000000
    sample_multiplier: 4
    shards: 256
    min_table_len_per_shard: 65536
    door_bits_per_counter: 12
  eviction:
    enabled: true
    replicas: 32
    soft_limit: 0.8
    hard_limit: 0.99
    check_interval: 100ms
  lifetime:
    enabled: true
    ttl: 2h
    on_ttl: refresh
    beta: 0.35
    rate: 1000
    replicas: 32
    coefficient: 0.25
  traces:
    enabled: true
    service_name: adv_cache
    service_version: dev
    exporter: http
    endpoint: 127.0.0.1:4318     # <-- CHANGEME: your OTEL Collector (http/4318 or grpc/4317)
    insecure: true
    sampling_mode: ratio
    sampling_rate: 0.1
    export_batch_size: 512
    export_batch_timeout: 3s
    export_max_queue: 1024
  metrics:
    enabled: true
  k8s:
    probe:
      timeout: 5s
  rules:
    /api/v1/user:                 # OnTTL: Will inherit global refresh unless overridden here.
      cache_key:
        query:                    # Include query params by prefix into the cache key (order-insensitive).
          - user[id]
          - domain
          - language
          - picked
          - timezone
        headers:                  # Include these request headers into the cache key (exact match).
          - Accept-Encoding
      cache_value:
        headers:                  # Response headers to store/forward with cached value.
          - Vary
          - Server
          - Content-Type
          - Content-Length
          - Content-Encoding
          - Cache-Control
          - X-Error-Reason

    /api/v1/client:
      cache_key:
        query:
          - user[id]
          - domain
          - language
          - picked
          - timezone
        headers:
          - Accept-Encoding
      cache_value:
        headers:
          - Vary
          - Server
          - Content-Type
          - Content-Length
          - Content-Encoding
          - Cache-Control
          - X-Error-Reason
```

**What to change first:**
- `cache.api.port` — the port **advCache** listens on.
- `cache.upstream.backend.addr` — point to your origin.
- `cache.compression.enabled` — enable if latency budget allows (runtime‑toggle also available).
- `cache.traces.*` — set `enabled: true` and `endpoint` of your OTEL Collector; adjust sampling.
- `cache.admission.enabled` — **true** to protect hot set; details of TinyLFU/Doorkeeper in the main config comments.
- `cache.upstream.policy` — **both `deny` and `await` are production‑ready**; choose behavior:
    - `deny` → fail‑fast under pressure (good for synthetic load / when back‑pressure is handled elsewhere).
    - `await` → apply back‑pressure (preferred default in many prod setups).

> Full field descriptions and advanced knobs are documented inline in the canonical `advcache.cfg.yaml`.

---

## Endpoints (runtime control plane)
- Main: `GET /{any:*}`
- Health: `GET /k8s/probe`
- Metrics: `GET /metrics` (Prometheus/VictoriaMetrics)
- Bypass: `/advcache/bypass`, `/on`, `/off`
- Compression: `/advcache/http/compression`, `/on`, `/off`
- Config dump: `/advcache/config`
- Entry by key: `/advcache/entry?key=<uint64>`
- Clear (two‑step): `/advcache/clear` → then `/advcache/clear?token=<...>`
- Invalidate: `/advcache/invalidate` (supports `_remove` for delete entries and `_path` + any additional queries)
- Upstream policy: `/advcache/upstream/policy`, `/await`, `/deny`
- Evictor: `/advcache/eviction`, `/on`, `/off`, `/scale?to=<n>`
- Lifetime manager: `/advcache/lifetime-manager`, `/on`, `/off`, `/rate?to=<n>`, `/scale?to=<n>`, `/policy`, `/policy/remove`, `/policy/refresh`
- Admission: `/advcache/admission`, `/on`, `/off`
- Tracing: `/advcache/traces`, `/on`, `/off`

---

## Traces: minimal overhead
- Spans: `ingress` (server), `upstream` (client on miss/proxy), `refresh` (background).
- When disabled: **fast no‑op** provider (atomic toggle only).
- When enabled: stdout exporter → sync; OTLP (`grpc`/`http`) → **batch** exporter.

**Enable quickly:** set in YAML and/or toggle at runtime:
```bash
GET /advcache/traces/on    # enable tracing now
GET /advcache/traces/off   # disable tracing
```

---

## Build & Run
```bash
# Native
cargo build --release
./target/release/advcache -cfg ./cfg/advcache.cfg.yaml

# Docker (see Dockerfile)
docker build -t advcache .
docker run --rm -p 8020:8020 \
  -v "$PWD/public/dump:/app/public/dump" \
  advcache -cfg /app/cfg/advcache.cfg.yaml
```

---

## Notes on performance
Remember that this depends largely on the specifics of your load.
- Local (4–6 CPU, 1–16KB docs, 20–25GB store): **175k RPS** steady.
- Bare‑metal (24 CPU, 50GB store, prod traffic): **~250k RPS** sustained.
- Memory overhead at 50GB: **1.5–3GB** (no traces) • **~7GB** (100% sampling).

---

## Testing
```bash
cargo test
```

---

## Benching
```bash
cargo bench
```


---

## License & Maintainer
Apache‑2.0 — see [LICENSE](./LICENSE).  
Maintainer: **Borislav Glazunov** — <glazunov2142@gmail.com> · Telegram `@gl_c137`
