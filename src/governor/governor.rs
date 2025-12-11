// Package orchestrator provides service orchestration functionality.

use anyhow::{Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tracing::{error, info};

use super::api::Governor;
use super::service::{Config, Service};

/// Orchestrator manages services and their lifecycle.
pub struct Orchestrator {
    // Use RwLock for async operations
    srvs_async: Arc<RwLock<HashMap<String, Arc<dyn Service>>>>,
    // Use Mutex for sync operations (Governor trait methods) - matches Go's sync.Mutex
    srvs_sync: Arc<Mutex<HashMap<String, Arc<dyn Service>>>>,
}

impl Orchestrator {
    /// Creates a new Orchestrator.
    pub fn new() -> Self {
        Self {
            srvs_async: Arc::new(RwLock::new(HashMap::with_capacity(8))),
            srvs_sync: Arc::new(Mutex::new(HashMap::with_capacity(8))),
        }
    }
}

impl Governor for Orchestrator {
    fn register(&self, name: String, s: Arc<dyn Service>) {
        // Use sync Mutex for sync operations (matches Go's sync.Mutex)
        let mut srvs = self.srvs_sync.lock().unwrap();
        if srvs.contains_key(&name) {
            return;
        }
        let transport = super::transport::ChanneledTransport::new();
        s.serve(transport.clone());
        srvs.insert(name.clone(), s.clone());
        // Also update async map for async operations
        let srvs_async = self.srvs_async.clone();
        let name_clone = name.clone();
        let s_clone = s.clone();
        tokio::runtime::Handle::current().spawn(async move {
            let mut srvs_guard = srvs_async.write().await;
            srvs_guard.insert(name_clone, s_clone);
        });
    }

    fn cfg(&self, name: &str) -> Result<Arc<dyn Config>> {
        // Use sync Mutex for sync operations
        let srvs = self.srvs_sync.lock().unwrap();
        if let Some(srv) = srvs.get(name) {
            Ok(srv.cfg())
        } else {
            Err(anyhow::anyhow!("orchestrator: no such {} service", name))
        }
    }

    fn on(&self, name: &str) -> Result<()> {
        // Use sync Mutex for sync operations (matches Go's sync.Mutex)
        let srvs = self.srvs_sync.lock().unwrap();
        let srv = srvs.get(name)
            .ok_or_else(|| anyhow::anyhow!("orchestrator: no such {} service", name))?;
        
        // Transport methods are async for dyn compatibility, but do synchronous work
        // Use std::thread::spawn to avoid blocking the runtime thread
        let transport = srv.transport();
        let handle = tokio::runtime::Handle::current();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(transport.on())
            }).join().unwrap()
        });
        if !result {
            Err(anyhow::anyhow!("orchestrator: cannot turn on {}, signal was not sent", srv.name()))
        } else {
            info!(srv = %srv.name(), "orchestrator: turning on...");
            Ok(())
        }
    }

    fn off(&self, name: &str) -> Result<()> {
        let srvs = self.srvs_sync.lock().unwrap();
        let srv = srvs.get(name)
            .ok_or_else(|| anyhow::anyhow!("orchestrator: no such {} service", name))?;
        
        let transport = srv.transport();
        let handle = tokio::runtime::Handle::current();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(transport.off())
            }).join().unwrap()
        });
        if !result {
            Err(anyhow::anyhow!("orchestrator: cannot turn off {}, signal was not sent", srv.name()))
        } else {
            info!(srv = %srv.name(), "orchestrator: turning off...");
            Ok(())
        }
    }

    fn start(&self, name: &str) -> Result<()> {
        let srvs = self.srvs_sync.lock().unwrap();
        let srv = srvs.get(name)
            .ok_or_else(|| anyhow::anyhow!("orchestrator: no such {} service", name))?;
        
        let transport = srv.transport();
        let handle = tokio::runtime::Handle::current();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(transport.start())
            }).join().unwrap()
        });
        if !result {
            Err(anyhow::anyhow!("orchestrator: cannot start {}, signal was not sent", srv.name()))
        } else {
            info!(srv = %srv.name(), "orchestrator: starting...");
            Ok(())
        }
    }

    fn reload(&self, name: &str, cfg: Arc<dyn Config>) -> Result<()> {
        let srvs = self.srvs_sync.lock().unwrap();
        let srv = srvs.get(name)
            .ok_or_else(|| anyhow::anyhow!("orchestrator: no such {} service", name))?;
        
        let transport = srv.transport();
        let handle = tokio::runtime::Handle::current();
        let cfg_clone = cfg.clone();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(transport.reload(cfg_clone))
            }).join().unwrap()
        });
        if !result {
            Err(anyhow::anyhow!("orchestrator: cannot reload {}, signal was not sent", srv.name()))
        } else {
            info!(srv = %srv.name(), "orchestrator: reloading...");
            Ok(())
        }
    }

    fn scale_to(&self, name: &str, n: usize) -> Result<()> {
        let srvs = self.srvs_sync.lock().unwrap();
        let srv = srvs.get(name)
            .ok_or_else(|| anyhow::anyhow!("orchestrator: no such {} service", name))?;
        
        let transport = srv.transport();
        let handle = tokio::runtime::Handle::current();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(transport.scale_to(n))
            }).join().unwrap()
        });
        if !result {
            Err(anyhow::anyhow!("orchestrator: cannot scale {}, signal was not sent", name))
        } else {
            info!(srv = %name, need_replicas = n, "orchestrator: scaling...");
            Ok(())
        }
    }

    fn stop(&self) {
        let srvs = self.srvs_sync.lock().unwrap();
        let handle = tokio::runtime::Handle::current();
        for srv in srvs.values() {
            let transport = srv.transport();
            let _srv_name = srv.name().to_string();
            let result = std::thread::scope(|scope| {
                scope.spawn(|| {
                    handle.block_on(transport.stop())
                }).join().unwrap()
            });
            if !result {
                error!(srv = %srv.name(), "orchestrator: cannot stop, signal was not sent");
            } else {
                info!(srv = %srv.name(), "orchestrator: stopping...");
            }
        }
    }
}
