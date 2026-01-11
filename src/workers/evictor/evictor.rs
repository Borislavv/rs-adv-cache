//! Eviction worker functionality.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::governor::{Config, Service, Transport};
use crate::workers::EvictionBackend;

use super::counters;

use counters::Counters;

/// Scalable worker group for cache eviction.
pub struct Evictor {
    shutdown_ctx: CancellationToken,
    cfg: Arc<RwLock<Arc<dyn Config>>>,
    workers_ctx: Arc<RwLock<CancellationToken>>,
    workers_active: Arc<AtomicI64>,
    workers_kill_tx: broadcast::Sender<()>,
    workers_tasks_tx: broadcast::Sender<()>,

    inited: Arc<AtomicBool>,
    name: String,
    counters: Arc<Counters>,
    backend: Arc<dyn EvictionBackend>,
    transport: OnceLock<Arc<dyn Transport>>,
}

impl Evictor {
    /// Creates a new evictor group.
    pub fn new(
        ctx: CancellationToken,
        name: String,
        cfg: Arc<dyn Config>,
        backend: Arc<dyn EvictionBackend>,
    ) -> Result<Arc<Self>> {
        let workers_ctx = CancellationToken::new();

        // Use broadcast channels for both kill signals and tasks (one-to-many)
        let (workers_kill_tx, _) = broadcast::channel(1);
        let (workers_tasks_tx, _) = broadcast::channel(num_cpus::get() * 4);

        let evictor = Arc::new(Self {
            shutdown_ctx: ctx,
            cfg: Arc::new(RwLock::new(cfg)),
            workers_ctx: Arc::new(RwLock::new(workers_ctx)),
            workers_active: Arc::new(AtomicI64::new(0)),
            workers_kill_tx,
            workers_tasks_tx,
            inited: Arc::new(AtomicBool::new(false)),
            name,
            counters: Arc::new(Counters::new()),
            backend,
            transport: OnceLock::new(),
        });

        Ok(evictor)
    }

    /// Gets the configuration.
    async fn config(&self) -> Arc<dyn Config> {
        self.cfg.read().await.clone()
    }

    /// Main loop that handles transport signals.
    async fn loop_handler(self: Arc<Self>, transport: Arc<dyn Transport>) {
        if !self
            .inited
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return;
        }

        let _ = self.transport.get_or_init(|| transport.clone());

        // Start soft eviction logger
        let logger_self = self.clone();
        tokio::task::spawn(async move {
            logger_self
                .soft_eviction_logger(Duration::from_secs(5))
                .await;
        });

        loop {
            tokio::select! {
                _ = self.shutdown_ctx.cancelled() => {
                    return;
                }
                _ = transport.on_start() => {
                    if let Err(err) = self.reload(None, "starting").await {
                        tracing::error!(name = %self.name, error = %err, "start up failed");
                    }
                }
                _ = transport.on_on() => {
                    if let Err(err) = self.on().await {
                        tracing::error!(name = %self.name, error = %err, "turning on failed");
                    }
                }
                _ = transport.on_off() => {
                    if let Err(err) = self.off().await {
                        tracing::error!(name = %self.name, error = %err, "turning off failed");
                    }
                }
                replicas = transport.on_scale_to() => {
                    if let Err(err) = self.scale(replicas).await {
                        tracing::error!(name = %self.name, error = %err, "scaling failed");
                    }
                }
                cfg = transport.on_reload() => {
                    if let Err(err) = self.reload(Some(cfg), "reloading").await {
                        tracing::error!(name = %self.name, error = %err, "reloading failed");
                    }
                }
                _ = transport.on_stop() => {
                    return;
                }
            }
        }
    }

    /// Turns on the evictor.
    async fn on(&self) -> Result<()> {
        let was_cfg = self.config().await;
        if !was_cfg.is_enabled() {
            let new_cfg = was_cfg.set_enabled(true);
            if let Err(e) = self.reload(Some(new_cfg), "reloading").await {
                return Err(e);
            }
            info!(name = %self.name, where = "on/off", "enabled");
        } else {
            warn!(name = %self.name, where = "on/off", "already enabled, nothing to change");
        }
        Ok(())
    }

    /// Turns off the evictor.
    async fn off(&self) -> Result<()> {
        let was_cfg = self.config().await;
        if was_cfg.is_enabled() {
            let new_cfg = was_cfg.set_enabled(false);
            if let Err(e) = self.reload(Some(new_cfg), "reloading").await {
                return Err(e);
            }
            info!(name = %self.name, where = "on/off", "disabled");
        } else {
            warn!(name = %self.name, where = "on/off", "already disabled, nothing to change");
        }
        Ok(())
    }

    /// Reloads the evictor with new configuration.
    async fn reload(&self, cfg: Option<Arc<dyn Config>>, action: &str) -> Result<()> {
        info!(name = %self.name, where = "reloading", action = action, "reloading...");

        let active = self.workers_active.load(Ordering::Relaxed);
        if active > 0 {
            info!(name = %self.name, where = "reloading", all = active, "downscaling all replicas");
        }
        self.scale_to(0).await;

        // Cancel and recreate workers context
        {
            let mut ctx_guard = self.workers_ctx.write().await;
            ctx_guard.cancel();
            *ctx_guard = CancellationToken::new();
        }

        if let Some(new_cfg) = cfg {
            *self.cfg.write().await = new_cfg;
            info!(name = %self.name, where = "reloading", "new config was applied");
        }

        let need_replicas = self.config().await.get_replicas();
        if need_replicas > 0 {
            info!(name = %self.name, where = "reloading", need_replicas, "upscaling to replicas");
            self.scale_to(need_replicas).await;
            info!(name = %self.name, where = "scaling", "scaled");
        }
        // Start tasks provider AFTER workers are created, so they can subscribe to broadcast channel
        // Start tasks provider AFTER workers are created, so they can subscribe to broadcast channel
        // loses messages sent before subscription
        self.run_eviction_tasks_provider().await;

        Ok(())
    }

    /// Scales the evictor to a specific number of replicas.
    async fn scale(&self, scale: usize) -> Result<()> {
        let was_cfg = self.config().await;
        if was_cfg.get_replicas() != scale {
            let new_cfg = was_cfg.set_replicas(scale);
            if let Err(e) = self.reload(Some(new_cfg), "reloading").await {
                return Err(e);
            }
            info!(name = %self.name, where = "scaling", "scaled");
        } else {
            warn!(name = %self.name, where = "scaling", "already scaled, nothing to change");
        }
        Ok(())
    }

    /// Scales to a specific number of replicas.
    async fn scale_to(&self, n: usize) {
        let to = n as i64;
        if to == 0 {
            {
                let ctx = self.workers_ctx.write().await;
                ctx.cancel();
            }
            // Wait for all workers to finish
            while self.workers_active.load(Ordering::Relaxed) > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            return;
        }

        let actual = self.workers_active.load(Ordering::Relaxed);

        if to > actual {
            let ctx_clone = {
                let mut guard = self.workers_ctx.write().await;
                if guard.is_cancelled() {
                    *guard = CancellationToken::new();
                }
                guard.clone()
            };
            let diff = to - actual;
            let cfg = self.config().await;
            for _ in 0..diff {
                self.up_one(ctx_clone.clone(), cfg.clone()).await;
            }
        } else if to < actual {
            let diff = actual - to;
            for _ in 0..diff {
                self.down_one().await;
            }
        }
    }

    /// Downs one worker.
    async fn down_one(&self) {
        info!(name = %self.name, "attempt to kill someone");
        if self.workers_kill_tx.send(()).is_ok() {
            info!(name = %self.name, "kill signal was sent");
        }
    }

    /// Ups one worker.
    async fn up_one(&self, ctx: CancellationToken, cfg: Arc<dyn Config>) {
        let self_clone = self;
        let name = self_clone.name.clone();
        info!(name = %name, "attempt to up single instance");
        let backend = self.backend.clone();
        let counters = self.counters.clone();
        let workers_active = self.workers_active.clone();
        let cfg_clone = cfg.clone();

        // Create receivers for this worker
        // Use broadcast receivers for both kill signals and tasks (subscribes to shared broadcast channels)
        let mut workers_kill_rx = self.workers_kill_tx.subscribe();
        let mut workers_tasks_rx = self.workers_tasks_tx.subscribe();

        workers_active.fetch_add(1, Ordering::Relaxed);

        let ctx_clone = ctx.clone();
        let worker_name = self_clone.name.clone();
        tokio::task::spawn(async move {
            let tick_freq = cfg_clone.get_freq().get_tick_freq();
            info!(name = %worker_name, tick_freq = ?tick_freq, "worker upped");

            let _guard = {
                struct Guard {
                    workers_active: Arc<AtomicI64>,
                    name: String,
                }
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.workers_active.fetch_sub(1, Ordering::Relaxed);
                        info!(name = %self.name, "worker is gone");
                    }
                }
                Guard {
                    workers_active: workers_active.clone(),
                    name: worker_name.clone(),
                }
            };

            loop {
                tokio::select! {
                    _ = ctx_clone.cancelled() => {
                        return;
                    }
                    _ = workers_kill_rx.recv() => {
                        return;
                    }
                    msg = workers_tasks_rx.recv() => {
                        match msg {
                            Ok(_) => {
                                const SPINS_BACKOFF: i64 = 8196;
                                let (freed_bytes, items) = backend.soft_evict_until_within_limit(SPINS_BACKOFF);
                                if items > 0 || freed_bytes > 0 {
                                    counters.evicted_items.fetch_add(items, Ordering::Relaxed);
                                    counters.evicted_bytes.fetch_add(freed_bytes, Ordering::Relaxed);
                                }
                            }
                            Err(_e) => {
                                // Broadcast channel closed or lagged, ignore
                            }
                        }
                    }
                }
            }
        });
    }

    /// Runs the eviction tasks provider.
    async fn run_eviction_tasks_provider(&self) {
        let backend = self.backend.clone();
        let workers_tasks_tx = self.workers_tasks_tx.clone();
        let shutdown_token = self.shutdown_ctx.clone();
        let workers_ctx = self.workers_ctx.clone();
        let cfg = self.config().await;
        let cfg_arc = Arc::new(cfg);

        tokio::task::spawn(async move {
            let tick_freq = cfg_arc.get_freq().get_tick_freq();
            let mut interval = tokio::time::interval(tick_freq);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Skip the first immediate tick
            // Skip the first immediate tick to match interval behavior
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        return;
                    }
                    _ = async {
                        let token = { workers_ctx.read().await.clone() };
                        token.cancelled().await
                    } => {
                        return;
                    }
                    _ = interval.tick() => {
                        // Config is already cloned before spawn, so we use it directly
                        if cfg_arc.is_enabled() && backend.soft_memory_limit_overcome() {
                            // Send task to all workers via broadcast channel
                            let _ = workers_tasks_tx.send(());
                        }
                    }
                }
            }
        });
    }

    /// Soft eviction logger.
    async fn soft_eviction_logger(&self, interval_duration: Duration) {
        use crate::workers::evictor::telemetry;

        let mut interval = tokio::time::interval(interval_duration);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = self.shutdown_ctx.cancelled() => {
                    return;
                }
                _ = interval.tick() => {
                    // Log eviction statistics and update metrics
                    // Counters are reset inside log_stats() via swap(0)
                    telemetry::log_stats(&self.name, &self.counters);
                }
            }
        }
    }
}

impl Service for Arc<Evictor> {
    fn name(&self) -> &str {
        &self.name
    }

    fn cfg(&self) -> Arc<dyn Config> {
        self.cfg.blocking_read().clone()
    }

    fn replicas(&self) -> usize {
        self.workers_active.load(Ordering::Relaxed) as usize
    }

    fn serve(&self, t: Arc<dyn Transport>) {
        // Ensure transport is set before any signals are sent.
        let _ = self.transport.get_or_init(|| t.clone());

        let evictor_clone = Arc::clone(self);
        tokio::task::spawn(async move {
            evictor_clone.loop_handler(t).await;
        });
    }

    fn transport(&self) -> Arc<dyn Transport> {
        self.transport
            .get()
            .expect("transport not initialized")
            .clone()
    }
}