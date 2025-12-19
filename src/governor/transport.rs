//! Transport for service communication.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::service::Config;

const RETRIES: usize = 5;

/// Transport interface for service communication.
/// Sending methods are synchronous (just channel sends), but made async for dyn compatibility
/// Receiving methods are async - they wait for messages
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends a start signal.
    fn start(&self) -> bool;

    /// Sends an on signal.
    fn on(&self) -> bool;

    /// Sends an off signal.
    fn off(&self) -> bool;

    /// Sends a reload signal with configuration.
    fn reload(&self, cfg: Arc<dyn Config>) -> bool;

    /// Sends a scale signal with replica count.
    fn scale_to(&self, n: usize) -> bool;

    /// Sends a stop signal.
    fn stop(&self) -> bool;

    /// Waits for a start signal.
    async fn on_start(&self) -> ();

    /// Waits for an on signal.
    async fn on_on(&self) -> ();

    /// Waits for an off signal.
    async fn on_off(&self) -> ();

    /// Waits for a reload signal.
    async fn on_reload(&self) -> Arc<dyn Config>;

    /// Waits for a scale signal.
    async fn on_scale_to(&self) -> usize;

    /// Waits for a stop signal.
    async fn on_stop(&self) -> ();
}

/// ChanneledTransport implements Transport using channels.
pub struct ChanneledTransport {
    start_tx: mpsc::Sender<()>,
    start_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<()>>>,
    on_tx: mpsc::Sender<()>,
    on_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<()>>>,
    off_tx: mpsc::Sender<()>,
    off_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<()>>>,
    reload_tx: mpsc::Sender<Arc<dyn Config>>,
    reload_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Arc<dyn Config>>>>,
    scale_tx: mpsc::Sender<usize>,
    scale_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<usize>>>,
    stop_tx: mpsc::Sender<()>,
    stop_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<()>>>,
}

impl ChanneledTransport {
    /// Creates a new ChanneledTransport.
    pub fn new() -> Arc<Self> {
        let (start_tx, start_rx) = mpsc::channel(1);
        let (on_tx, on_rx) = mpsc::channel(1);
        let (off_tx, off_rx) = mpsc::channel(1);
        let (reload_tx, reload_rx) = mpsc::channel(1);
        let (scale_tx, scale_rx) = mpsc::channel(1);
        let (stop_tx, stop_rx) = mpsc::channel(1);

        Arc::new(Self {
            start_tx,
            start_rx: Arc::new(tokio::sync::Mutex::new(start_rx)),
            on_tx,
            on_rx: Arc::new(tokio::sync::Mutex::new(on_rx)),
            off_tx,
            off_rx: Arc::new(tokio::sync::Mutex::new(off_rx)),
            reload_tx,
            reload_rx: Arc::new(tokio::sync::Mutex::new(reload_rx)),
            scale_tx,
            scale_rx: Arc::new(tokio::sync::Mutex::new(scale_rx)),
            stop_tx,
            stop_rx: Arc::new(tokio::sync::Mutex::new(stop_rx)),
        })
    }

    fn try_send<T: Send>(tx: &mpsc::Sender<T>, value: T) -> bool {
        tx.try_send(value).is_ok()
    }

    fn try_retry<F>(f: F) -> bool
    where
        F: Fn() -> bool,
    {
        for _ in 0..RETRIES {
            if f() {
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl Transport for ChanneledTransport {
    fn start(&self) -> bool {
        Self::try_retry(|| Self::try_send(&self.start_tx, ()))
    }

    fn on(&self) -> bool {
        Self::try_retry(|| Self::try_send(&self.on_tx, ()))
    }

    fn off(&self) -> bool {
        Self::try_retry(|| Self::try_send(&self.off_tx, ()))
    }

    fn reload(&self, cfg: Arc<dyn Config>) -> bool {
        Self::try_retry(|| Self::try_send(&self.reload_tx, cfg.clone()))
    }

    fn scale_to(&self, n: usize) -> bool {
        Self::try_retry(|| Self::try_send(&self.scale_tx, n))
    }

    fn stop(&self) -> bool {
        Self::try_retry(|| Self::try_send(&self.stop_tx, ()))
    }

    async fn on_start(&self) -> () {
        let mut rx = self.start_rx.lock().await;
        rx.recv().await;
    }

    async fn on_on(&self) -> () {
        let mut rx = self.on_rx.lock().await;
        rx.recv().await;
    }

    async fn on_off(&self) -> () {
        let mut rx = self.off_rx.lock().await;
        rx.recv().await;
    }

    async fn on_reload(&self) -> Arc<dyn Config> {
        let mut rx = self.reload_rx.lock().await;
        // If channel is closed, this is a logic error; propagate empty config to terminate loop.
        rx.recv().await.expect("reload channel closed unexpectedly")
    }

    async fn on_scale_to(&self) -> usize {
        let mut rx = self.scale_rx.lock().await;
        rx.recv().await.unwrap_or(0)
    }

    async fn on_stop(&self) -> () {
        let mut rx = self.stop_rx.lock().await;
        rx.recv().await;
    }
}
