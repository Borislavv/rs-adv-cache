# advCache Configuration Guide

This document provides a **complete, practical, and field‑tested** reference to configure **AdvCache**.  
It includes the **canonical configuration** (verbatim), **what to change first**, and a **deep dive** into every section with runtime effects and tuning advice.

> If you're just starting, begin with the *What to change first* checklist, then come back to the details.

---

## What to change first (quick checklist)

- **Listening port** → `cache.api.port` (default `8020`).
- **Origin (upstream)** → `cache.upstream.backend.addr`.
- **Tracing (OTEL)** → `cache.traces.enabled` = `true`, set `endpoint` to your collector (`4317` gRPC or `4318` HTTP).
- **Compression** → `cache.compression.enabled` = `true` (enable when latency budget allows).
- **Admission control (TinyLFU)** → `cache.admission.enabled` = `true` to protect the hot set.
- **Eviction thresholds** → keep `soft_limit` and `hard_limit` sensible for your memory budget.
- **Upstream policy** → `await` (back‑pressure) **or** `deny` (fail‑fast) — choose based on desired behavior.

Runtime control plane for quick toggles exists for: **bypass**, **compression**, **admission**, **lifetime/eviction scaling**, **traces**, **upstream policy**. See [Runtime endpoints](#runtime-endpoints-mapping).

---

## Canonical configuration (verbatim)

Below is the canonical `advcache.cfg.yaml` shipped with the repository. Use it as the source of truth.

```yaml
cache:
  env: "prod"                    # Runtime environment label (e.g., dev/stage/prod). Used for logs/metrics tagging.
  enabled: true                  # Master switch: enables the cache service.

  logs:
    level: "debug"               # Log level: debug|info|warn|error. Prefer "info" in prod, "debug" for short bursts.

  runtime:
    num_cpus: 0                  # Rust/tokio: 0 = auto (num_cpus). If set >0, used to cap worker threads.

  api:
    name: "adv_cache"            # Human-readable service name exposed in API/metrics.
    port: "8020"                 # HTTP port for the admin/API endpoints.

  upstream:
    backend:
      id: "example-upstream-backend"
      enabled: true
      policy: "deny"
      host: "service-example:8080"
      scheme: "http"
      rate: 15000                 # Per-backend RPS cap (token bucket or equivalent).
      concurrency: 4096           # Max simultaneous requests.
      timeout: "10s"              # Base timeout for upstream requests.
      max_timeout: "1m"           # Hard cap if “slow path” header allows extending timeouts.
      use_max_timeout_header: ""  # If non-empty, presence of this header lifts timeout to max_timeout.
      healthcheck: "/healthcheck" # Liveness probe path; 2xx = healthy.

  # Compression
  # - Supported levels:
  #   CompressNoCompression      = 0
  #   CompressBestSpeed          = 1
  #   CompressBestCompression    = 9
  #   CompressDefaultCompression = 6
  #   CompressHuffmanOnly        = -2
  compression:
    enabled: false
    level: 1

  data:
    dump:
      enabled: true               # Enable periodic dump to disk for warm restarts / backup.
      dump_dir: "public/dump"     # Directory to store dump files.
      dump_name: "cache.dump"     # Base filename (rotations will append indices/timestamps).
      crc32_control_sum: true     # Validate dump integrity via CRC32 on load.
      max_versions: 3             # Keep up to N rotated versions; older are deleted.
      gzip: false                 # Compress dumps with gzip (smaller disk, more CPU).
    mock:
      enabled: false              # If true, prefill cache with mock data (for local testing).
      length: 1000000             # Number of mock entries to generate.

  storage:
    mode: listing                 # Implementation of LRU algo through per-shard lists or Redis style sampling (values=sampling/listing).
    size: 53687091200             # Max memory budget for storage (bytes). Here: 50 GiB.

  admission:
    enabled: false
    capacity: 2000000               # global TinyLFU window size (how many recent events are tracked)
    sample_multiplier: 4            # multiplies the window for aging (larger = slower aging)
    shards: 256                     # number of independent shards (reduces contention)
    min_table_len_per_shard: 65536  # minimum counter slots per shard (larger = fewer collisions)
    door_bits_per_counter: 12       # bits per counter (2^bits-1 max count; larger = less saturation)

  eviction:
    enabled: true                 # Run background evictor to keep memory under configured thresholds.
    replicas: 32                  # Number of evictor workers (>=1). Increase for large heaps / high churn.
    soft_limit: 0.80              # At storage.size × soft_limit start gentle eviction + tighten admission.
    hard_limit: 0.99              # At storage.size × hard_limit trigger minimal hot-path eviction; also set debug memory limit.
    check_interval: "100ms"       # Defines how often the main evictor loop will check the memory limit.

  lifetime:
    enabled: true                 # Enable background refresh/remove for eligible entries.
    ttl: "2h"                     # Default TTL for successful (200) responses unless overridden by rules.
    on_ttl: refresh               # Values: 'remove' or 'refresh'. Defines behavior on TTL overcome.
    beta: 0.35                    # Jitter factor (0..1) to spread refreshes/removes and prevent thundering herd.
    rate: 1000                    # Global refresh/remove QPS cap to upstreams (safety valve).
    replicas: 32                  # Number of workers (>=1).
    coefficient: 0.25             # Start refresh/remove attempts at TTL × coefficient (e.g., 0.5 = at 50% of TTL).

  traces:
    enabled: true
    service_name: "adv_cache"
    service_version: "dev"
    exporter: "http"                                       # "stdout" | "grpc" (OTLP/4317) | "http" (OTLP/4318)
    endpoint: "localhost:4318"                             # ignored when exporter=stdout
    insecure: true                                         # should use TLS?
    sampling_mode: "always"                                # off | always | ratio
    sampling_rate: 1.0                                     # used when mode=ratio
    export_batch_size: 512
    export_batch_timeout: "3s"
    export_max_queue: 1024

  metrics:
    enabled: true                 # Expose Prometheus-style metrics (and/or internal stats if logs.stats=true).

  k8s:
    probe:
      timeout: "5s"               # Liveness/readiness probe timeout for the service endpoints.

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

    /api/v1/customer:
      enabled: true
      ttl: "4h"
      beta: 0.4
      coefficient: 0.3
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

    /api/v1/buyer:
      enabled: true
      ttl: "4h"
      beta: 0.4
      coefficient: 0.3
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

> Notes:  
> • Comments in the YAML are authoritative and come from the codebase.  
> • You can maintain an override file (e.g., `./cfg/advcache.cfg.yaml`). The app will try to read the local file first if present.

---

## Section‑by‑section reference

### 1) `cache.env`, `cache.enabled`, `cache.logs`, `cache.runtime`, `cache.api`

- **`cache.env`** (`"dev"|"stage"|"prod"|...`): environment label used for logs/metrics tagging.
- **`cache.enabled`** (`bool`): master switch for the cache service. When `false`, requests are effectively proxied (pair with the runtime **bypass** toggles).
- **`cache.logs.level`** (`"debug"|"info"|"warn"|"error"`): prefer `"info"` in production. Use `"debug"` only during short diagnostic windows.
- **`cache.runtime.num_cpus`** (`int`, `0 = auto`): CPU limit. Set explicit `N` to cap CPU usage if needed.
- **`cache.api`**:
    - `name` → human‑readable service name exposed in API/metrics.
    - `port` → HTTP port for runtime control plane and main route.

**Impact:** determines process resource limits, logging verbosity, and the address advCache listens on.  
**Related endpoints:** `/k8s/probe`, `/metrics`, all `/advcache/...` toggles use the same `api.port`.

---

### 2) `cache.upstream` (origin & policy)

Use either **`backend`** (single origin). Common fields:

- `addr` (`scheme://host:port`) → **must point to your origin**.
- `healthcheck` (`path`) → 2xx response = healthy.
- `concurrency` (`int`) → max simultaneous requests per process.
- `timeout`, `max_timeout` (`duration`) → base and extended timeouts.
- `use_max_timeout_header` (string header name) → if the header is present upstream, allow requests to extend to `max_timeout`.
- Optional rate‑limit knobs (RPS caps) where provided.

**Policy**:
- `policy: "await"` → back‑pressure under load (queue/await), preventing error storms.
- `policy: "deny"` → fail‑fast when saturated; good for synthetic load or when back‑pressure is handled elsewhere.  
  **Both** are production‑ready — pick for your behavior and SLOs.

**Impact:** affects how misses/proxy paths talk to origin (timeouts, concurrency), and how the system behaves under pressure.  
**Runtime endpoints:** `/advcache/upstream/policy`, `/advcache/upstream/policy/await`, `/advcache/upstream/policy/deny`.

---

### 3) `cache.data` (dump & mock)

- **`dump`**: periodic on‑disk snapshots for warm restarts/backup.
    - `enabled`, `dump_dir`, `dump_name`, `crc32_control_sum`, `max_versions`, `gzip`.
- **`mock`**: optional prefill for local testing.
    - `enabled`, `length` → generates N synthetic entries.

**Impact:** enables faster restarts and local UX. Dumps increase disk I/O; keep versions sensible.

---

### 4) `cache.storage`

- `mode` → implementation strategy for LRU/eviction (e.g., **`listing`** vs **`sampling`**).
- `size` (`bytes`) → **hard** memory budget for the storage layer (e.g., `50 GiB`).

**Impact:** `size` is the global memory ceiling used together with eviction thresholds; see next section.

---

### 5) `cache.admission` (TinyLFU + Doorkeeper)

- `enabled` (`bool`) → **enable to protect the hot set** against one‑hit wonders.
- `capacity` → global logical window of events tracked. Larger = longer memory of popularity.
- `sample_multiplier` → multiplies the window for aging cadence. Larger = slower aging.
- `shards` → independent CMS shards to reduce contention.
- `min_table_len_per_shard` → min counter slots per shard (bigger table = fewer collisions, more RAM).
- `door_bits_per_counter` → bit width per counter (affects max frequency and saturation).

**Impact:** stronger admission reduces churn.  
**Runtime endpoints:** `/advcache/admission`, `/advcache/admission/on`, `/advcache/admission/off`.

---

### 6) `cache.eviction` (background memory controller)

- `enabled` (`bool`) → run the evictor loop. **Keep enabled in production.**
- `replicas` (`int`) → number of workers (increase for large heaps/high churn).
- `soft_limit` (`0..1`) → when `mem_used >= size × soft_limit` start gentle eviction & tighten admission.
- `hard_limit` (`0..1`) → when `mem_used >= size × hard_limit` trigger aggressive eviction to avoid OOM.
- `check_interval` (`duration`) → how often the evictor loop re-evaluates memory limits.

**Impact:** keeps RSS stable and prevents out‑of‑memory conditions.  
**Runtime endpoints:** `/advcache/eviction`, `/on`, `/off`, `/scale?to=<n>`.

---

### 7) `cache.lifetime` (TTL, β‑staggered refresh/remove)

- `enabled` (`bool`) → turn on background refresh/remove.
- `ttl` (`duration`) → default TTL for success responses (overridable per rule).
- `on_ttl` (`"refresh"|"remove"`) → behavior when TTL elapses.
- `beta` (`0..1`) → randomness factor to spread work and avoid thundering herd.
- `rate` (`int`) → global QPS cap for refresh/remove to upstream (safety valve).
- `replicas` (`int`) → number of workers.
- `coefficient` (`0..1`) → when to start attempts relative to TTL (e.g., `0.5` = at 50% of TTL).

**Impact:** controls freshness and background traffic to origin.  
**Runtime endpoints:** `/advcache/lifetime-manager`, `/on`, `/off`, `/rate?to=<n>`, `/scale?to=<n>`, `/policy`, `/policy/remove`, `/policy/refresh`.

---

### 8) `cache.compression` (response middleware)

- `enabled` (`bool`) → compress responses when enabled. Runtime‑toggle available.
- `level` (`int`) → `-2..9` as per comments: `0` no compression, `1` best speed, `9` best compression, `-2` Huffman only.

**Impact:** reduces egress bytes at some CPU cost. If you **cache compressed variants**, include `Accept-Encoding` in `rules.*.cache_key.headers`.  
**Runtime endpoints:** `/advcache/http/compression`, `/on`, `/off`.

---

### 9) `cache.traces` (OpenTelemetry tracing)

- `enabled` (`bool`) → enable tracing. Hot path uses a fast no‑op provider when disabled.
- `service_*` → name/version/tenant attributes.
- `exporter` (`"stdout"|"grpc"|"http"`) → OTLP transport.
- `endpoint` (`host:port`) → collector address (ignored for `stdout`).
- `insecure` (`bool`) → allow insecure local dev.
- `sampling_mode` (`"off"|"always"|"ratio"`) + `sampling_rate` (`0..1`) → head sampling.
- `export_batch_*` → batch size/timeout/queue for exporters.

**Impact:** minimal overhead when disabled; batched exports when enabled. Spans: `ingress` (server), `upstream` (client), `refresh` (internal).  
**Runtime endpoints:** `/advcache/traces`, `/on`, `/off`.

---

### 10) `cache.metrics`

- `enabled` (`bool`) → expose Prometheus‑style metrics at `/metrics`.

---

### 11) `cache.k8s.probe`

- `timeout` (`duration`) → liveness/readiness probe timeout for `/k8s/probe`.

---

### 13) `cache.rules` (canonical keys & stored headers)

Rules are **path‑scoped** and override or complement global behavior:

- **`cache_key.query`**: list of query parameter names (or prefixes) included into the **cache key**. The list is **sorted** for determinism.
- **`cache_key.headers`**: request headers included into the key (e.g., **`Accept-Encoding`** to separate gzip/br).
- **`cache_value.headers`**: response headers to **store and replay** with the cached value (order preserved).
- Optional per‑rule overrides: `enabled`, `ttl`, `beta`, `coefficient`.

**Impact:** determines **key canonicalization** and which response metadata is preserved.  
**Gotcha:** if you store compressed variants, **whitelist `Accept-Encoding`** in `cache_key.headers`.

---

## Runtime endpoints mapping

| Feature | Endpoints |
|---|---|
| Bypass cache | `/advcache/bypass`, `/advcache/bypass/on`, `/advcache/bypass/off` |
| Compression | `/advcache/http/compression`, `/on`, `/off` |
| Admission | `/advcache/admission`, `/on`, `/off` |
| Traces (tracing) | `/advcache/traces`, `/on`, `/off` |
| Eviction | `/advcache/eviction`, `/on`, `/off`, `/scale?to=<n>` |
| Lifetime/refresh | `/advcache/lifetime-manager`, `/on`, `/off`, `/rate?to=<n>`, `/scale?to=<n>`, `/policy`, `/policy/remove`, `/policy/refresh` |
| Upstream policy | `/advcache/upstream/policy`, `/await`, `/deny` |
| Config dump | `/advcache/config` |
| Entry by key | `/advcache/entry?key=<uint64>` |
| Health & metrics | `/k8s/probe`, `/metrics` |

---

## Recipes

### A) Local development
- Set `cache.api.port: "8020"` and `cache.upstream.backend.addr: "http://127.0.0.1:8081"` (run a local origin).
- Optionally `traces.enabled: true` with `endpoint: "127.0.0.1:4318"`.
- Keep `admission.enabled: true`, `compression.enabled: false` for synthetic benchmarks.
- Enable dumps for faster restarts.

### B) Production baseline
- `upstream.policy: "await"` for back‑pressure or use more detail parameters like rate and timeout for manage `"deny"` policy.
- `admission.enabled: true` to protect the hot set.
- Set `soft_limit ≈ 0.85–0.9`, `hard_limit ≈ 0.95–0.99` and tune `replicas/scan/check_interval`.
- Metrics/traces enabled; sampling set to a small ratio (e.g., `0.01–0.1`).

### C) Clustered origin
- Use `cache.upstream.cluster.backends[]` with individual `addr` and `healthcheck`.
- Consider per‑backend rate limits where applicable.

---

## Validation checklist

- [ ] `/k8s/probe` returns 200 on the chosen `api.port`.
- [ ] `/metrics` exposes series; import Grafana dashboard JSON.
- [ ] `policy` toggle behaves as expected (`await` vs `deny`).
- [ ] Admission toggles correctly affect hit/miss churn under load.
- [ ] Eviction thresholds maintain stable RSS under peak load.
- [ ] `Accept-Encoding` is whitelisted when storing compressed variants.
- [ ] Traces reach the OTEL collector (check `service_name` and `endpoint`).

---

If something is unclear or you find a mismatch with the code, open an issue or PR — contributions welcome.
