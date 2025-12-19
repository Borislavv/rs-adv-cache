
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::config::{self, ConfigTrait, LifetimeRule, Rule, RuleKey, RuleValue};
use crate::governor::Governor;
use crate::governor::Orchestrator;
use crate::model::{Entry, Response as ModelResponse};
use crate::db::{Storage, DB};
use crate::upstream::{Response, Upstream};

fn make_rule(path: &str, ttl: Option<Duration>) -> Arc<Rule> {
    Arc::new(Rule {
        path: Some(path.to_string()),
        path_bytes: Some(path.as_bytes().to_vec()),
        cache_key: RuleKey {
            query: None,
            query_bytes: None,
            headers: None,
            headers_map: None,
        },
        cache_value: RuleValue {
            headers: None,
            headers_map: None,
        },
        refresh: ttl.map(|d| LifetimeRule {
            enabled: true,
            ttl: Some(d),
            beta: Some(1.0),
            coefficient: Some(0.0),
        }),
    })
}

fn make_entry(rule: Arc<Rule>, id: usize, body_size: usize) -> Entry {
    let query = format!("id-{id}");
    let queries = vec![(b"user[id]".to_vec(), query.into_bytes())];
    let mut entry = Entry::new(rule, &queries, &[]);
    let resp = ModelResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: vec![b'a'; body_size],
    };
    entry.set_payload(&[], &[], &resp);
    entry.touch_refreshed_at();
    entry
}

fn tune_limits(cfg: &mut config::Config, storage_size: i64, soft: f64, hard: f64) {
    let storage = cfg.cache.storage.as_mut().unwrap();
    storage.size = storage_size;
    storage.soft_memory_limit = (storage_size as f64 * soft) as i64;
    storage.hard_memory_limit = (storage_size as f64 * hard) as i64;
    storage.admission_memory_limit = storage.soft_memory_limit.saturating_sub(1024);

    if let Some(eviction) = cfg.cache.eviction.as_mut() {
        eviction.soft_limit = Some(soft);
        eviction.hard_limit = Some(hard);
        eviction.check_interval = Some(Duration::from_millis(20));
        eviction.enabled = true;
        eviction.replicas = Some(4);
    }
}

struct CountingUpstream {
    refresh_calls: Arc<AtomicUsize>,
}

impl CountingUpstream {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            refresh_calls: Arc::new(AtomicUsize::new(0)),
        })
    }
}

#[async_trait::async_trait]
impl Upstream for CountingUpstream {
    async fn request(
        &self,
        _rule: &Rule,
        _queries: &[(Vec<u8>, Vec<u8>)],
        _headers: &[(Vec<u8>, Vec<u8>)],
    ) -> anyhow::Result<Response> {
        Ok(Response::new(200, vec![], b"ok".to_vec()))
    }

    async fn proxy_request(
        &self,
        _method: &str,
        _path: &str,
        _query: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> anyhow::Result<Response> {
        Ok(Response::new(200, vec![], b"ok".to_vec()))
    }

    async fn refresh(&self, _entry: &Entry) -> anyhow::Result<()> {
        self.refresh_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn is_healthy(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_evictor_respects_soft_limit() {
    let shutdown = tokio_util::sync::CancellationToken::new();
    let mut cfg = config::new_test_config();

    // Small storage and low soft limit to trigger eviction with few entries.
    tune_limits(&mut cfg, 20_000, 0.5, 0.9);
    if let Some(lifetime) = cfg.cache.lifetime.as_mut() {
        lifetime.enabled = false;
    }

    let governor = Arc::new(Orchestrator::new());
    let upstream = CountingUpstream::new();
    let db = DB::new(
        shutdown.clone(),
        cfg.clone(),
        governor.clone(),
        upstream.clone(),
    )
    .expect("storage must start");

    // Populate storage with large payloads to exceed soft limit.
    let rule = make_rule("/api/v1/user", None);
    for i in 0..32 {
        let entry = make_entry(rule.clone(), i, 2_048);
        db.set(entry);
    }

    let (mem_before, _) = db.stat();
    let soft_limit = cfg.storage().soft_memory_limit;
    assert!(
        mem_before > soft_limit,
        "mem_before={} must exceed soft_limit={}",
        mem_before,
        soft_limit
    );

    // Wait for evictor workers to scan and evict.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let (mem_after, len_after) = db.stat();

    assert!(
        mem_after <= soft_limit,
        "mem_after={} must be <= soft_limit={}",
        mem_after,
        soft_limit
    );
    assert!(
        len_after > 0,
        "evictor should keep cache non-empty after eviction"
    );

    // Shutdown workers explicitly.
    shutdown.cancel();
    drop(db);
    governor.stop();
}

#[tokio::test]
async fn test_lifetimer_refreshes_expired_entries() {
    let shutdown = tokio_util::sync::CancellationToken::new();
    let mut cfg = config::new_test_config();

    tune_limits(&mut cfg, 50_000, 0.9, 0.95);
    if let Some(eviction) = cfg.cache.eviction.as_mut() {
        eviction.enabled = false;
    }
    if let Some(lifetime) = cfg.cache.lifetime.as_mut() {
        lifetime.enabled = true;
        lifetime.ttl = Some(Duration::from_millis(200));
        lifetime.rate = Some(100);
        lifetime.replicas = Some(2);
        lifetime.on_ttl = Some(config::TTLMode::Refresh);
    }

    let governor = Arc::new(Orchestrator::new());
    let upstream = CountingUpstream::new();
    let db = DB::new(
        shutdown.clone(),
        cfg.clone(),
        governor.clone(),
        upstream.clone(),
    )
    .expect("storage must start");

    let ttl_ns = cfg.lifetime().unwrap().ttl.unwrap().as_nanos() as i64;
    let rule = make_rule("/api/v1/user", cfg.lifetime().unwrap().ttl);
    let now = crate::time::unix_nano();

    // Insert expired entries.
    for i in 0..8 {
        let entry = make_entry(rule.clone(), i, 512);
        entry.set_refreshed_at_for_tests(now - (ttl_ns * 2));
        db.set(entry);
    }

    // Wait for lifetimer to pick and refresh expired entries.
    // Poll every 500ms for up to 30 seconds.
    let timeout = Duration::from_secs(30);
    let poll_interval = Duration::from_millis(500);
    let start = std::time::Instant::now();
    
    loop {
        let refreshed = upstream.refresh_calls.load(Ordering::Relaxed);
        if refreshed >= 8 {
            // Condition met, test passes
            break;
        }
        
        if start.elapsed() >= timeout {
            panic!(
                "timeout waiting for refresh calls: expected at least 8, got {} after {:?}",
                refreshed,
                start.elapsed()
            );
        }
        
        tokio::time::sleep(poll_interval).await;
    }
    
    let refreshed = upstream.refresh_calls.load(Ordering::Relaxed);
    assert!(
        refreshed >= 8,
        "expected at least 8 refresh calls, got {}",
        refreshed
    );

    shutdown.cancel();
    drop(db);
    governor.stop();
}
