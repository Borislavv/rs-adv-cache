use anyhow::{Context, Result};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use std::sync::atomic::{AtomicI32, Ordering};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::config::{Config, ConfigTrait};
use crate::db::Storage;
use crate::time;
use crate::dedlog;

#[derive(Debug, thiserror::Error)]
#[error("persistence mode is not enabled")]
pub struct DumpNotEnabledError;

/// Dumper interface for cache persistence.
#[async_trait::async_trait]
pub trait Dumper: Send + Sync {
    /// Dumps cache to disk.
    async fn dump(&self, ctx: CancellationToken) -> Result<()>;

    /// Loads cache from disk.
    async fn load(&self, ctx: CancellationToken) -> Result<()>;

    /// Loads a specific version of cache dump.
    #[allow(dead_code)]
    async fn load_version(&self, ctx: CancellationToken, version: &str) -> Result<()>;
}

/// Dump implementation for cache persistence.
pub struct DumperImpl {
    cfg: Config,
    storage: Arc<dyn Storage>,
}

impl DumperImpl {
    /// Creates a new dumper.
    pub fn new(cfg: Config, storage: Arc<dyn Storage>) -> Result<Self> {
        Ok(Self {
            cfg,
            storage
        })
    }

    /// Gets the dump directory path.
    fn dump_dir(&self) -> Result<PathBuf> {
        let dir = self
            .cfg
            .data()
            .and_then(|d| d.dump.as_ref())
            .and_then(|d| d.dir.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("public/dump");

        Ok(PathBuf::from(dir))
    }

    /// Gets the dump filename.
    fn dump_name(&self) -> String {
        self.cfg
            .data()
            .and_then(|d| d.dump.as_ref())
            .and_then(|d| d.name.as_ref())
            .map(|s| s.clone())
            .unwrap_or_else(|| "cache.dump".to_string())
    }

    /// Gets the maximum number of versions to keep.
    fn max_versions(&self) -> usize {
        self.cfg
            .data()
            .and_then(|d| d.dump.as_ref())
            .and_then(|d| d.max_versions)
            .unwrap_or(3)
    }

    /// Checks if gzip compression is enabled.
    fn gzip_enabled(&self) -> bool {
        self.cfg
            .data()
            .and_then(|d| d.dump.as_ref())
            .map(|d| d.gzip)
            .unwrap_or(false)
    }

    /// Checks if CRC32 checksum is enabled.
    fn crc32_enabled(&self) -> bool {
        self.cfg
            .data()
            .and_then(|d| d.dump.as_ref())
            .map(|d| d.crc32_control)
            .unwrap_or(true)
    }

    /// Picks the next sequential version number.
    async fn next_version_dir(&self, base_dir: &Path) -> Result<u32> {
        let mut max_v = 0u32;
        let mut entries = fs::read_dir(base_dir).await.context("Failed to read dump directory")?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("v") {
                    if let Ok(v) = file_name[1..].parse::<u32>() {
                        if v > max_v {
                            max_v = v;
                        }
                    }
                }
            }
        }
        
        Ok(max_v + 1)
    }

    /// Rotates version directories, keeping only the newest max_of dirs.
    async fn rotate_version_dirs(&self, base_dir: &Path, max: usize) -> Result<()> {
        let mut entries_vec = Vec::new();
        let mut entries = fs::read_dir(base_dir).await.context("Failed to read dump directory")?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("v") {
                    if let Ok(metadata) = entry.metadata().await {
                        entries_vec.push((path, metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
                    }
                }
            }
        }

        if entries_vec.len() <= max {
            return Ok(());
        }

        // Sort by modification time (newest first)
        entries_vec.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove old directories
        for (path, _) in entries_vec.iter().skip(max) {
            if let Err(e) = fs::remove_dir_all(path).await {
                warn!(
                    component = "dump",
                    event = "cleanup_failed",
                    path = ?path,
                    error = %e,
                    "failed to remove old dump directory"
                );
            } else {
                info!(
                    component = "dump",
                    event = "cleanup_success",
                    path = ?path,
                    "removed old dump directory"
                );
            }
        }

        Ok(())
    }

    /// Gets the latest version directory.
    async fn get_latest_version_dir(&self, base_dir: &Path) -> Result<Option<PathBuf>> {
        let mut entries_vec = Vec::new();
        let mut entries = fs::read_dir(base_dir).await.context("Failed to read dump directory")?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("v") {
                    if let Ok(metadata) = entry.metadata().await {
                        entries_vec.push((path, metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
                    }
                }
            }
        }

        if entries_vec.is_empty() {
            return Ok(None);
        }

        // Sort by modification time (newest first)
        entries_vec.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(Some(entries_vec[0].0.clone()))
    }

    /// Formats timestamp as "20060102T150405".
    fn format_timestamp(&self) -> String {
        let now = time::now();
        let datetime = chrono::DateTime::<chrono::Utc>::from(now);
        datetime.format("%Y%m%dT%H%M%S").to_string()
    }
}

#[async_trait::async_trait]
impl Dumper for DumperImpl {
    async fn dump(&self, ctx: CancellationToken) -> Result<()> {
        let start = time::now();
        let dump_cfg = self
            .cfg
            .data()
            .and_then(|d| d.dump.as_ref())
            .ok_or(DumpNotEnabledError)?;

        if !self.cfg.is_enabled() || !dump_cfg.enabled {
            return Err(DumpNotEnabledError.into());
        }

        let dump_dir = self.dump_dir()?;
        let dump_name = self.dump_name();

        // Create base dump directory
        fs::create_dir_all(&dump_dir)
            .await
            .context("Failed to create dump directory")?;

        // Create version directory
        let version_num = self.next_version_dir(&dump_dir).await?;
        let version_dir = dump_dir.join(format!("v{}", version_num));
        fs::create_dir_all(&version_dir)
            .await
            .context("Failed to create version directory")?;

        let timestamp = self.format_timestamp();
        let success = Arc::new(AtomicI32::new(0));
        let failures = Arc::new(AtomicI32::new(0));
        let gzip = self.gzip_enabled();
        let crc32_control = self.crc32_enabled();

        // Collect shard data first
        use std::sync::Mutex;
        let shards_data = Arc::new(Mutex::new(Vec::new()));
        let shards_data_clone = shards_data.clone();
        let ctx_walk = ctx.clone();
        
        self.storage.walk_shards(
            ctx_walk.clone(),
            Box::new(move |shard_key, shard| {
                // Collect all entries from shard synchronously
                let mut entries = Vec::new();
                shard.walk_r(&ctx_walk, |_key, entry| {
                    entries.push(entry.to_bytes());
                    true
                });
                shards_data_clone.lock().unwrap().push((shard_key, entries));
            }),
        );

        // Extract shards data from Mutex
        let shards_data_final: Vec<(u64, Vec<Vec<u8>>)> = {
            let guard = shards_data.lock().unwrap();
            guard.clone()
        };

        // Process all shards asynchronously
        let mut tasks = Vec::new();
        for (shard_key, entries) in shards_data_final.into_iter() {
            let dump_name_clone = dump_name.clone();
            let version_dir_clone = version_dir.clone();
            let timestamp_clone = timestamp.clone();
            let success_clone = success.clone();
            let failures_clone = failures.clone();
            let ctx_clone = ctx.clone();
            let entries_clone = entries;

            let (tx, rx) = oneshot::channel();
            let handle = tokio::task::spawn_blocking(move || {
                let ext = if gzip { ".dump.gz" } else { ".dump" };
                let name = format!("{}-shard-{}-{}{}", dump_name_clone, shard_key, timestamp_clone, ext);
                let file_path = version_dir_clone.join(&name);
                let tmp_path = file_path.with_file_name(format!("{}.tmp", name));

                // Create temporary file
                let file = match std::fs::File::create(&tmp_path) {
                    Ok(f) => f,
                    Err(e) => {
                        dedlog::err(Some(&e as &dyn std::error::Error), Some("file"), "[dump] create error");
                        failures_clone.fetch_add(1, Ordering::Relaxed);
                        let _ = tx.send(());
                        return;
                    }
                };

                // Setup writer (with optional gzip)
                let writer: Box<dyn Write> = if gzip {
                    Box::new(GzEncoder::new(file, Compression::default()))
                } else {
                    Box::new(BufWriter::with_capacity(512 * 1024, file))
                };

                let mut buf_writer = BufWriter::with_capacity(512 * 1024, writer);

                // Write entries
                for entry_bytes in entries_clone {
                    if ctx_clone.is_cancelled() {
                        break;
                    }

                    let crc = if crc32_control {
                        crc32fast::hash(&entry_bytes)
                    } else {
                        0
                    };

                    // Write length (4 bytes) + CRC32 (4 bytes) + data
                    let len = entry_bytes.len() as u32;
                    let len_bytes = len.to_le_bytes();
                    let crc_bytes = crc.to_le_bytes();
                    let mut meta_buf = [0u8; 8];
                    meta_buf[0..4].copy_from_slice(&len_bytes);
                    meta_buf[4..8].copy_from_slice(&crc_bytes);

                    if buf_writer.write_all(&meta_buf).is_err() {
                        failures_clone.fetch_add(1, Ordering::Relaxed);
                        let _ = tx.send(());
                        return;
                    }

                    if buf_writer.write_all(&entry_bytes).is_err() {
                        failures_clone.fetch_add(1, Ordering::Relaxed);
                        let _ = tx.send(());
                        return;
                    }

                    success_clone.fetch_add(1, Ordering::Relaxed);
                }

                // Flush and finish
                if buf_writer.flush().is_err() {
                    failures_clone.fetch_add(1, Ordering::Relaxed);
                    let _ = tx.send(());
                    return;
                }

                // Finish gzip encoder if used
                drop(buf_writer);

                // Rename tmp to final file
                if let Err(e) = std::fs::rename(&tmp_path, &file_path) {
                    dedlog::err(Some(&e as &dyn std::error::Error), Some("file"), "[dump] rename error");
                    failures_clone.fetch_add(1, Ordering::Relaxed);
                }

                let _ = tx.send(());
            });

            tasks.push((handle, rx));
        }

        // Wait for all tasks to complete
        for (handle, rx) in tasks {
            let _ = rx.await;
            let _ = handle.await;
        }

        let max_versions = self.max_versions();
        if max_versions > 0 {
            self.rotate_version_dirs(&dump_dir, max_versions).await?;
        }

        let duration = time::since(start);
        let written = success.load(Ordering::Relaxed);
        let fails = failures.load(Ordering::Relaxed);

        info!(
            component = "dump",
            event = "dump_complete",
            written,
            fails,
            duration_secs = duration.as_secs_f64(),
            "dumping finished"
        );

        if fails > 0 {
            anyhow::bail!("dump finished with {} errors", fails);
        }

        Ok(())
    }

    async fn load(&self, ctx: CancellationToken) -> Result<()> {
        let dump_dir = self.dump_dir()?;
        let version_dir = match self.get_latest_version_dir(&dump_dir).await? {
            Some(dir) => dir,
            None => {
                anyhow::bail!("no versioned dump dirs found in {:?}", dump_dir);
            }
        };
        self.load_from_dir(ctx, &version_dir).await
    }

    async fn load_version(&self, ctx: CancellationToken, version: &str) -> Result<()> {
        let dump_dir = self.dump_dir()?;
        let version_dir = dump_dir.join(version);
        if !version_dir.exists() {
            anyhow::bail!("Dump version {} not found", version);
        }
        self.load_from_dir(ctx, &version_dir).await
    }
}

impl DumperImpl {
    /// Internal method to load dump from a specific directory.
    async fn load_from_dir(&self, ctx: CancellationToken, dir: &Path) -> Result<()> {
        let start = time::now();
        let dump_name = self.dump_name();
        let cfg = self.cfg.clone();
        let storage = self.storage.clone();
        let crc32_control = self.crc32_enabled();

        // Find all dump files matching the pattern
        let pattern_prefix = format!("{}-shard-", dump_name);
        let mut dump_files = Vec::new();
        let mut entries = fs::read_dir(dir).await.context("Failed to read dump directory")?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with(&pattern_prefix) && (file_name.ends_with(".dump") || file_name.ends_with(".dump.gz")) {
                    if let Ok(metadata) = entry.metadata().await {
                        dump_files.push((path, metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
                    }
                }
            }
        }

        if dump_files.is_empty() {
            anyhow::bail!("no dump files found in {:?}", dir);
        }

        // Extract latest timestamp from filenames
        let latest_timestamp = dump_files.iter()
            .filter_map(|(path, _)| {
                path.file_name()?
                    .to_str()?
                    .split('-')
                    .last()?
                    .trim_end_matches(".dump")
                    .trim_end_matches(".gz")
                    .to_string()
                    .into()
            })
            .max();

        // Filter files by latest timestamp
        if let Some(ts) = latest_timestamp {
            dump_files.retain(|(path, _)| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.contains(&ts))
                    .unwrap_or(false)
            });
        }

        let success = Arc::new(AtomicI32::new(0));
        let failures = Arc::new(AtomicI32::new(0));
        let mut tasks = Vec::new();

        // Load each dump file
        for (file_path, _) in dump_files {
            let cfg_clone = cfg.clone();
            let storage_clone = storage.clone();
            let ctx_clone = ctx.clone();
            let success_clone = success.clone();
            let failures_clone = failures.clone();
            let crc32_control_clone = crc32_control;
            let is_gzip = file_path.to_string_lossy().ends_with(".gz");

            let (tx, rx) = oneshot::channel();
            let handle = tokio::task::spawn_blocking(move || {
                let file = match std::fs::File::open(&file_path) {
                    Ok(f) => f,
                    Err(e) => {
                            dedlog::err(Some(&e as &dyn std::error::Error), Some("file"), "[load] open error");
                        failures_clone.fetch_add(1, Ordering::Relaxed);
                        let _ = tx.send(());
                        return;
                    }
                };

                // Setup reader (with optional gzip)
                let reader: Box<dyn Read> = if is_gzip {
                    Box::new(GzDecoder::new(file))
                } else {
                    Box::new(file)
                };

                let mut buf_reader = BufReader::with_capacity(512 * 1024, reader);

                loop {
                    if ctx_clone.is_cancelled() {
                        break;
                    }

                    // Read meta buffer (8 bytes: length + CRC32)
                    let mut meta_buf = [0u8; 8];
                    match buf_reader.read_exact(&mut meta_buf) {
                        Ok(_) => {},
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        Err(e) => {
                            dedlog::err(Some(&e as &dyn std::error::Error), Some("file"), "[load] read meta error");
                            failures_clone.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    }

                    let sz = u32::from_le_bytes([meta_buf[0], meta_buf[1], meta_buf[2], meta_buf[3]]) as usize;
                    let exp_crc = u32::from_le_bytes([meta_buf[4], meta_buf[5], meta_buf[6], meta_buf[7]]);

                    // Read entry data
                    let mut buf = vec![0u8; sz];
                    match buf_reader.read_exact(&mut buf) {
                        Ok(_) => {},
                        Err(e) => {
                            dedlog::err(Some(&e as &dyn std::error::Error), Some("file"), "[load] read entry error");
                            failures_clone.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    }

                    // Verify CRC32 if enabled
                    if crc32_control_clone {
                        let calc_crc = crc32fast::hash(&buf);
                        if calc_crc != exp_crc {
                            dedlog::err(None, Some("file"), "[load] crc mismatch");
                            failures_clone.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    }

                    // Deserialize entry
                    match crate::model::to_bytes::from_bytes(&buf, &cfg_clone) {
                        Ok(entry) => {
                            storage_clone.set(entry);
                            success_clone.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            dedlog::err(Some(&*e), Some("file"), "[load] entry decode error");
                            failures_clone.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                let _ = tx.send(());
            });

            tasks.push((handle, rx));
        }

        // Wait for all tasks to complete
        for (handle, rx) in tasks {
            let _ = rx.await;
            let _ = handle.await;
        }

        let duration = time::since(start);
        let restored = success.load(Ordering::Relaxed);
        let fails = failures.load(Ordering::Relaxed);

        info!(
            component = "dump",
            event = "load_complete",
            restored,
            fails,
            duration_secs = duration.as_secs_f64(),
            "restoring dump"
        );

        if fails > 0 {
            anyhow::bail!("load finished with {} errors", fails);
        }

        Ok(())
    }
}
