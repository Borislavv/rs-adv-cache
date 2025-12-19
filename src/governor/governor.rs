//! Service orchestration functionality.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

use super::api::Governor;
use super::service::{Config, Service};
use super::transport::ChanneledTransport;

/// Orchestrator manages services and their lifecycle.
pub struct Orchestrator {
    srvs: Mutex<HashMap<String, Arc<dyn Service>>>,
}

impl Orchestrator {
    /// Creates a new Orchestrator.
    pub fn new() -> Self {
        Self {
            srvs: Mutex::new(HashMap::with_capacity(8)),
        }
    }

    fn with_srv<F, T>(&self, name: &str, f: F) -> Result<T>
    where
        F: FnOnce(&Arc<dyn Service>) -> Result<T>,
    {
        let srvs = self.srvs.lock().unwrap();
        let srv = srvs
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("orchestrator: no such {} service", name))?;
        f(srv)
    }

    fn send_signal<F>(&self, name: &str, action: &'static str, send: F) -> Result<()>
    where
        F: FnOnce(&Arc<dyn super::transport::Transport>) -> bool,
    {
        self.with_srv(name, |srv| {
            let transport = srv.transport();
            if !send(&transport) {
                Err(anyhow::anyhow!(format!(
                    "orchestrator: cannot {action} {}, signal was not sent",
                    srv.name()
                )))
            } else {
                info!(srv = %srv.name(), action = action, "orchestrator: action sent");
                Ok(())
            }
        })
    }
}

impl Governor for Orchestrator {
    fn register(&self, name: String, s: Arc<dyn Service>) {
        let mut srvs = self.srvs.lock().unwrap();
        if srvs.contains_key(&name) {
            return;
        }

        let transport = ChanneledTransport::new();
        s.serve(transport.clone());
        srvs.insert(name, s);
    }

    fn cfg(&self, name: &str) -> Result<Arc<dyn Config>> {
        self.with_srv(name, |srv| Ok(srv.cfg()))
    }

    fn on(&self, name: &str) -> Result<()> {
        self.send_signal(name, "turning on", |t| t.on())
    }

    fn off(&self, name: &str) -> Result<()> {
        self.send_signal(name, "turning off", |t| t.off())
    }

    fn start(&self, name: &str) -> Result<()> {
        self.send_signal(name, "starting", |t| t.start())
    }

    fn reload(&self, name: &str, cfg: Arc<dyn Config>) -> Result<()> {
        self.send_signal(name, "reloading", move |t| t.reload(cfg.clone()))
    }

    fn scale_to(&self, name: &str, n: usize) -> Result<()> {
        self.send_signal(name, "scaling", move |t| t.scale_to(n))
    }

    fn stop(&self) {
        let srvs = self.srvs.lock().unwrap();
        for srv in srvs.values() {
            let transport = srv.transport();
            let ok = transport.stop();
            if !ok {
                error!(srv = %srv.name(), "orchestrator: cannot stop, signal was not sent");
            } else {
                info!(srv = %srv.name(), "orchestrator: stopping...");
            }
        }
    }
}
