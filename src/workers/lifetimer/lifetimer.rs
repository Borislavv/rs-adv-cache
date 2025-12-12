// Package lifetimer provides lifetime management worker group.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::rate;
use crate::config::Config as AppConfig;
use crate::model::Entry;
use crate::governor::{Config, Transport};
use crate::workers::RefreshBackend;

use super::counters::Counters;
use super::telemetry;

/// Group is a scalable worker set driven by governor.Transport.
pub struct LifetimeManager {
    shutdown_token: CancellationToken,
    cfg: Arc<RwLock<Arc<dyn Config>>>,
    g_cfg: AppConfig,
    w_mu: Arc<Mutex<()>>,
    w_ctx: Arc<RwLock<CancellationToken>>,
    w_wg: Arc<Mutex<JoinSet<()>>>,
    w_num_active: Arc<AtomicI64>,
    w_kill: Arc<Mutex<mpsc::Receiver<()>>>,
    w_kill_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    w_tasks: Arc<Mutex<mpsc::Receiver<Entry>>>,
    w_tasks_tx: Arc<Mutex<mpsc::Sender<Entry>>>,
    g_rate: Arc<governor::RateLimiter<governor::state::direct::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
    inited: Arc<AtomicBool>,
    name: String,
    backend: Arc<dyn RefreshBackend>,
    transport: Arc<Mutex<Option<Arc<dyn Transport>>>>,
    counters: Arc<Counters>,
}

impl LifetimeManager {
    /// Creates a new refresher Group.
    pub fn new(
        shutdown_token: CancellationToken,
        name: String,
        cfg: Arc<dyn Config>,
        g_cfg: crate::config::Config,
        backend: Arc<dyn RefreshBackend>,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        use governor::{Quota, RateLimiter};
        use std::num::NonZeroU32;

        let rate_limit = cfg.get_freq().get_rate_limit() as u32;
        let quota = Quota::per_second(NonZeroU32::new(rate_limit).unwrap());
        let g_rate = Arc::new(RateLimiter::direct(quota));

        let (w_kill_tx, w_kill_rx) = mpsc::channel(1);
        let (w_tasks_tx, w_tasks_rx) = mpsc::channel(cfg.get_freq().get_rate_limit());

        Ok(Arc::new(Self {
            shutdown_token,
            cfg: Arc::new(RwLock::new(cfg.clone())),
            g_cfg,
            w_mu: Arc::new(Mutex::new(())),
            w_ctx: Arc::new(RwLock::new(CancellationToken::new())),
            w_wg: Arc::new(Mutex::new(JoinSet::new())),
            w_num_active: Arc::new(AtomicI64::new(0)),
            w_kill: Arc::new(Mutex::new(w_kill_rx)),
            w_kill_tx: Arc::new(Mutex::new(Some(w_kill_tx))),
            w_tasks: Arc::new(Mutex::new(w_tasks_rx)),
            w_tasks_tx: Arc::new(Mutex::new(w_tasks_tx)),
            g_rate,
            inited: Arc::new(AtomicBool::new(false)),
            name,
            backend,
            transport: Arc::new(Mutex::new(None)),
            counters: Arc::new(Counters::new()),
        }))
    }

    /// Gets the current configuration.
    pub async fn cfg(&self) -> Arc<dyn Config> {
        self.cfg.read().await.clone()
    }

    /// Gets the group name.
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets the number of active replicas.
    #[allow(dead_code)]
    pub fn replicas(&self) -> i64 {
        self.w_num_active.load(Ordering::Relaxed)
    }

    /// Starts the worker group loop.
    pub async fn serve(&self, transport: Arc<dyn Transport>) {
        if self.inited.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            return;
        }

        *self.transport.lock().await = Some(transport.clone());
        let w_ctx = CancellationToken::new();
        *self.w_ctx.write().await = w_ctx.clone();

        // Start logger
        let shutdown_token = self.shutdown_token.clone();
        let name = self.name.clone();
        let counters = self.counters.clone();
        let cfg = self.cfg.clone();
        let g_cfg = self.g_cfg.clone();
        let w_num_active = self.w_num_active.clone();
        tokio::task::spawn(async move {
            telemetry::logger(
                shutdown_token,
                name,
                counters,
                cfg,
                g_cfg,
                w_num_active,
                std::time::Duration::from_secs(5),
            ).await;
        });

        let group = self.clone();
        let transport_clone = transport.clone();
        tokio::spawn(async move {
            group.loop_worker(transport_clone, w_ctx).await;
        });
    }

    async fn loop_worker(&self, transport: Arc<dyn Transport>, _w_ctx: CancellationToken) {
        loop {
            tokio::select! {
                _ = self.shutdown_token.cancelled() => {
                    return;
                }
                _ = transport.on_start() => {
                    if let Err(e) = self.reload(None, "starting").await {
                        tracing::error!(name = %self.name, error = %e, "start up failed");
                    }
                }
                _ = transport.on_on() => {
                    if let Err(e) = self.on().await {
                        tracing::error!(name = %self.name, error = %e, "turning on failed");
                    }
                }
                _ = transport.on_off() => {
                    if let Err(e) = self.off().await {
                        tracing::error!(name = %self.name, error = %e, "turning off failed");
                    }
                }
                replicas = transport.on_scale_to() => {
                    if let Err(e) = self.scale(replicas).await {
                        tracing::error!(name = %self.name, error = %e, "scaling failed");
                    }
                }
                cfg = transport.on_reload() => {
                    if let Err(e) = self.reload(Some(cfg), "reloading").await {
                        tracing::error!(name = %self.name, error = %e, "reloading failed");
                    }
                }
                _ = transport.on_stop() => {
                    return;
                }
            }
        }
    }

    async fn on(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let was_cfg = self.cfg().await;
        if !was_cfg.is_enabled() {
            let new_cfg = was_cfg.set_enabled(true);
            self.reload(Some(new_cfg), "reloading").await?;
            tracing::info!(name = %self.name, where = "on/off", "enabled");
        } else {
            tracing::warn!(name = %self.name, where = "on/off", "already enabled, nothing to change");
        }
        Ok(())
    }

    async fn off(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let was_cfg = self.cfg().await;
        if was_cfg.is_enabled() {
            let new_cfg = was_cfg.set_enabled(false);
            self.reload(Some(new_cfg), "reloading").await?;
            tracing::info!(name = %self.name, where = "on/off", "disabled");
        } else {
            tracing::warn!(name = %self.name, where = "on/off", "already disabled, nothing to change");
        }
        Ok(())
    }

    async fn reload(&self, cfg: Option<Arc<dyn Config>>, action: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(name = %self.name, where = "reloading", %action, "reloading...");

        let active = self.w_num_active.load(Ordering::Relaxed);
        if active > 0 {
            tracing::info!(name = %self.name, where = "reloading", active, "downscaling all replicas");
        }
        self.scale_to(0).await;

        // Reset worker context
        {
            let mut w_ctx = self.w_ctx.write().await;
            w_ctx.cancel();
            *w_ctx = CancellationToken::new();
        }

        if let Some(cfg) = cfg {
            // Store the new config
            *self.cfg.write().await = cfg;
            tracing::info!(name = %self.name, where = "reloading", "new config was applied");
        }

        self.run_exceed_ttl_entries_provider().await;
        
        let need_replicas = self.cfg().await.get_replicas();
        if need_replicas > 0 {
            tracing::info!(name = %self.name, where = "reloading", need_replicas, "upscaling to replicas");
            self.scale_to(need_replicas).await;
            tracing::info!(name = %self.name, where = "scaling", "scaled");
        }

        Ok(())
    }

    async fn scale(&self, scale: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let was_cfg = self.cfg().await;
        if was_cfg.get_replicas() != scale {
            let new_cfg = was_cfg.set_replicas(scale);
            self.reload(Some(new_cfg), "reloading").await?;
            tracing::info!(name = %self.name, where = "scaling", "scaled");
        } else {
            tracing::warn!(name = %self.name, where = "scaling", "already scaled, nothing to change");
        }
        Ok(())
    }

    async fn scale_to(&self, n: usize) {
        let to = n as i64;
        if to == 0 {
            {
                let w_ctx = self.w_ctx.read().await;
                w_ctx.cancel();
            }
            let mut wg = self.w_wg.lock().await;
            while let Some(_) = wg.join_next().await {}
            return;
        }

        let actual = self.w_num_active.load(Ordering::Relaxed);

        if to > actual {
            let diff = to - actual;
            let cfg = self.cfg().await;
            let w_ctx = self.w_ctx.read().await.clone();
            for _ in 0..diff {
                self.up_one(w_ctx.clone(), cfg.clone()).await;
            }
            return;
        }

        if to < actual {
            let diff = actual - to;
            for _ in 0..diff {
                self.down_one().await;
            }
        }
    }

    async fn down_one(&self) {
        tracing::info!(svc = "refresher", name = %self.name, "attempt to kill one worker");
        if let Some(tx) = self.w_kill_tx.lock().await.as_ref() {
            let _ = tx.send(()).await;
            tracing::info!(svc = "refresher", name = %self.name, "kill signal sent");
        }
    }

    /// Launches one worker goroutine.
    async fn up_one(&self, ctx: CancellationToken, cfg: Arc<dyn Config>) {
        tracing::info!(name = %self.name, tick_freq = ?cfg.get_freq().get_tick_freq(), "attempt to up worker");

        let w_num_active = self.w_num_active.clone();
        let w_wg = self.w_wg.clone();
        let w_kill = self.w_kill.clone();
        let w_tasks = self.w_tasks.clone();
        let g_rate = self.g_rate.clone();
        let name = self.name.clone();
        let backend = self.backend.clone();
        let counters = self.counters.clone();

        w_num_active.fetch_add(1, Ordering::Relaxed);
        let mut join_set = w_wg.lock().await;
        join_set.spawn(async move {
            let _guard = WorkerGuard::new(w_num_active.clone(), name.clone());

            tracing::info!(name = %name, tick_freq = ?cfg.get_freq().get_tick_freq(), "worker upped");

            loop {
                tokio::select! {
                    _ = ctx.cancelled() => {
                        return;
                    }
                    result = async {
                        let mut guard = w_kill.lock().await;
                        guard.recv().await
                    } => {
                        if result.is_some() {
                            return;
                        }
                    }
                    entry = async {
                        let mut guard = w_tasks.lock().await;
                        guard.recv().await
                    } => {
                        if let Some(mut entry) = entry {
                            // Global rate limiting for refresh
                            g_rate.until_ready().await;
                            match backend.on_ttl(&mut entry).await {
                                Ok(_) => {
                                    counters.success_updates.fetch_add(1, Ordering::Relaxed);
                                }
                                Err(_) => {
                                    counters.error_updates.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    async fn run_exceed_ttl_entries_provider(&self) {
        let w_wg = self.w_wg.clone();
        let shutdown_token = self.shutdown_token.clone();
        let w_ctx = self.w_ctx.clone();
        let cfg = self.cfg.clone();
        let backend = self.backend.clone();
        let w_tasks_tx = self.w_tasks_tx.clone();
        let counters = self.counters.clone();

        let mut join_set = w_wg.lock().await;
        join_set.spawn(async move {
            let rate_limit = cfg.read().await.get_freq().get_rate_limit();
            let mut limiter = rate::Limiter::new(shutdown_token.clone(), rate_limit);

            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        return; // Global cancellation
                    }
                    _ = async {
                        let guard = w_ctx.read().await;
                        guard.cancelled().await
                    } => {
                        return; // Workers reloading
                    }
                    _ = limiter.take() => {
                        let cfg_guard = cfg.read().await;
                        if cfg_guard.is_enabled() {
                            let l = backend.len();
                            let m = backend.mem();
                            if l > 0 || m > 0 {
                                counters.scans_total.fetch_add(1, Ordering::Relaxed);
                                if let Some(entry) = backend.peek_expired_ttl() {
                                    counters.scans_hit.fetch_add(1, Ordering::Relaxed);
                                    let tx = w_tasks_tx.lock().await;
                                    if tx.send(entry).await.is_err() {
                                        break;
                                    }
                                } else {
                                    counters.scans_miss.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    #[allow(dead_code)]
    async fn close(&self) {
        self.inited.store(false, Ordering::Relaxed);
        self.shutdown_token.cancel();
        {
            let w_ctx = self.w_ctx.read().await;
            w_ctx.cancel();
        }
        let mut wg = self.w_wg.lock().await;
        while let Some(_) = wg.join_next().await {}
        tracing::info!(name = %self.name, where = "closing", "closed");
    }
}

impl Clone for LifetimeManager {
    fn clone(&self) -> Self {
        Self {
            shutdown_token: self.shutdown_token.clone(),
            cfg: self.cfg.clone(),
            g_cfg: self.g_cfg.clone(),
            w_mu: self.w_mu.clone(),
            w_ctx: self.w_ctx.clone(),
            w_wg: self.w_wg.clone(),
            w_num_active: self.w_num_active.clone(),
            w_kill: self.w_kill.clone(),
            w_kill_tx: self.w_kill_tx.clone(),
            w_tasks: self.w_tasks.clone(),
            w_tasks_tx: self.w_tasks_tx.clone(),
            g_rate: self.g_rate.clone(),
            inited: self.inited.clone(),
            name: self.name.clone(),
            backend: self.backend.clone(),
            transport: self.transport.clone(),
            counters: self.counters.clone(),
        }
    }
}

/// Guard to decrement active worker count on drop.
struct WorkerGuard {
    w_num_active: Arc<AtomicI64>,
    name: String,
}

impl WorkerGuard {
    fn new(w_num_active: Arc<AtomicI64>, name: String) -> Self {
        Self { w_num_active, name }
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.w_num_active.fetch_sub(1, Ordering::Relaxed);
        tracing::info!(name = %self.name, "worker is gone");
    }
}

impl crate::governor::Service for Arc<LifetimeManager> {
    fn name(&self) -> &str {
        // Access the name field directly to avoid recursion
        &self.name
    }

    fn cfg(&self) -> Arc<dyn crate::governor::Config> {
        // This is sync, so we need to block
        // Use std::thread::scope to avoid blocking the runtime thread
        let handle = tokio::runtime::Handle::current();
        let self_ref = &**self; // Dereference Arc to get &LifetimeManager
        std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(async {
                    LifetimeManager::cfg(self_ref).await
                })
            }).join().unwrap()
        })
    }

    fn replicas(&self) -> usize {
        // Call the struct method directly to avoid recursion
        LifetimeManager::replicas(&**self) as usize
    }

    fn serve(&self, t: Arc<dyn crate::governor::Transport>) {
        let lifetime_mgr_clone = Arc::clone(self);
        tokio::task::spawn(async move {
            LifetimeManager::serve(&*lifetime_mgr_clone, t).await;
        });
    }

    fn transport(&self) -> Arc<dyn crate::governor::Transport> {
        // Use std::thread::scope to avoid blocking the runtime thread
        let handle = tokio::runtime::Handle::current();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(async {
                    self.transport.lock().await.clone().unwrap()
                })
            }).join().unwrap()
        })
    }
}

