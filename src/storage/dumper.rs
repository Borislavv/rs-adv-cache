// Package storage provides dump/load functionality for persistence.

use anyhow::{Context, Result};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use crc32fast::Hasher;

use crate::config::{Config, ConfigTrait};
use crate::model::Entry;
use crate::model::to_bytes::from_bytes;
use crate::storage::Storage;
use crate::storage::map::Shard;
use crate::shared::time;

const DUMP_BUFFER_SIZE: usize = 512 * 1024; // 512 KiB

/// Error for when dump is not enabled.
#[derive(Debug, thiserror::Error)]
#[error("persistence mode is not enabled")]
pub struct DumpNotEnabledError;

/// Trait for persistence operations.
#[async_trait::async_trait]
pub trait Dumper: Send + Sync {
    /// Dumps the storage to disk.
    async fn dump(&self, ctx: CancellationToken) -> Result<()>;
    
    /// Loads the storage from the latest dump.
    async fn load(&self, ctx: CancellationToken) -> Result<()>;
    
    /// Loads the storage from a specific version.
    async fn load_version(&self, ctx: CancellationToken, version: String) -> Result<()>;
}

/// Dumper implementation.
pub struct DumperImpl {
    cfg: Config,
    storage: Arc<dyn Storage>,
}

impl DumperImpl {
    /// Creates a new dumper.
    pub fn new(
        cfg: Config,
        storage: Arc<dyn Storage>,
        _backend: Arc<dyn crate::upstream::Upstream>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            cfg,
            storage,
        }))
    }
}

#[async_trait::async_trait]
impl Dumper for DumperImpl {
    async fn dump(&self, ctx: CancellationToken) -> Result<()> {
        let start = Instant::now();
        let dump_cfg = self.cfg.data()
            .and_then(|d| d.dump.as_ref())
            .ok_or_else(|| anyhow::anyhow!("dump config not found"))?;
        
        if !self.cfg.is_enabled() || !dump_cfg.enabled {
            return Err(anyhow::anyhow!(DumpNotEnabledError));
        }
        
        let dump_dir = dump_cfg.dir.as_ref()
            .ok_or_else(|| anyhow::anyhow!("dump dir not configured"))?;
        let dump_name = dump_cfg.name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("dump name not configured"))?;
        
        // Create base dump dir
        fs::create_dir_all(dump_dir)
            .with_context(|| format!("create base dump dir: {}", dump_dir))?;
        
        // Create version dir
        let version_num = next_version_dir(dump_dir)?;
        let version_dir = PathBuf::from(dump_dir).join(format!("v{}", version_num));
        fs::create_dir_all(&version_dir)
            .with_context(|| format!("create version dir: {}", version_dir.display()))?;
        
        // Format timestamp: 20060102T150405
        let timestamp = chrono::Local::now().format("%Y%m%dT%H%M%S").to_string();
        
        let success = Arc::new(AtomicI32::new(0));
        let failures = Arc::new(AtomicI32::new(0));
        
        // Walk shards and dump each one (using blocking spawn for file I/O)
        let storage_clone = self.storage.clone();
        let cfg_clone = self.cfg.clone();
        let dump_dir_clone = version_dir.clone();
        let dump_name_clone = dump_name.clone();
        let timestamp_clone = timestamp.clone();
        let success_clone = success.clone();
        let failures_clone = failures.clone();
        let gzip = dump_cfg.gzip;
        let crc32_control = dump_cfg.crc32_control;
        
        let mut handles: Vec<JoinHandle<()>> = Vec::new();
        
        // Collect entries from each shard and spawn dump tasks
        // In Go: WalkShards -> goroutine per shard -> shard.WalkR -> write
        // In Rust: walk_shards -> collect entries per shard -> spawn_blocking -> write
        storage_clone.walk_shards(ctx.clone(), Box::new(|shard_key, shard| {
            let dump_dir_task = dump_dir_clone.clone();
            let dump_name_task = dump_name_clone.clone();
            let timestamp_task = timestamp_clone.clone();
            let success_task = success_clone.clone();
            let failures_task = failures_clone.clone();
            let ctx_task = ctx.clone();
            
            // Collect entries from this shard
            let mut entries: Vec<Entry> = Vec::new();
            shard.walk_r(&ctx_task, |_k, entry: &Entry| {
                entries.push(entry.clone());
                true
            });
            
            // Spawn blocking task to write dump file
            let handle = tokio::task::spawn_blocking(move || {
                let ext = if gzip { ".dump.gz" } else { ".dump" };
                // Use shard.id() for consistency (though shard_key is already the id)
                let actual_shard_id = shard_key; // In walk_shards, the key is the shard id
                let name = dump_dir_task.join(format!("{}-shard-{}-{}{}", 
                    dump_name_task, actual_shard_id, timestamp_task, ext));
                let tmp_name = name.with_extension(format!("dump.tmp"));
                
                // Create file
                let file = match std::fs::File::create(&tmp_name) {
                    Ok(f) => f,
                    Err(e) => {
                        error!(file = %tmp_name.display(), error = %e, "[dump] create error");
                        failures_task.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                };
                
                // Create writer (with optional gzip)
                let result = if gzip {
                    let gz_encoder = GzEncoder::new(file, Compression::default());
                    let mut writer = BufWriter::with_capacity(DUMP_BUFFER_SIZE, gz_encoder);
                    
                    // Write entries
                    for entry in entries {
                        if ctx_task.is_cancelled() {
                            break;
                        }
                        
                        let data = entry.to_bytes();
                        let crc = if crc32_control {
                            let mut hasher = Hasher::new();
                            hasher.update(&data);
                            hasher.finalize()
                        } else {
                            0
                        };
                        
                        // Write meta: [len: u32][crc: u32]
                        let mut meta_buf = [0u8; 8];
                        byteorder::LittleEndian::write_u32(&mut meta_buf[0..4], data.len() as u32);
                        byteorder::LittleEndian::write_u32(&mut meta_buf[4..8], crc);
                        
                        if writer.write_all(&meta_buf).is_err() || writer.write_all(&data).is_err() {
                            failures_task.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        
                        success_task.fetch_add(1, Ordering::Relaxed);
                    }
                    
                    writer.flush().and_then(|_| {
                        // Finish gzip encoder - extract from BufWriter
                        writer.into_inner().and_then(|gz| gz.finish())
                    })
                } else {
                    let mut writer = BufWriter::with_capacity(DUMP_BUFFER_SIZE, file);
                    
                    // Write entries
                    for entry in entries {
                        if ctx_task.is_cancelled() {
                            break;
                        }
                        
                        let data = entry.to_bytes();
                        let crc = if crc32_control {
                            let mut hasher = Hasher::new();
                            hasher.update(&data);
                            hasher.finalize()
                        } else {
                            0
                        };
                        
                        // Write meta: [len: u32][crc: u32]
                        let mut meta_buf = [0u8; 8];
                        byteorder::LittleEndian::write_u32(&mut meta_buf[0..4], data.len() as u32);
                        byteorder::LittleEndian::write_u32(&mut meta_buf[4..8], crc);
                        
                        if writer.write_all(&meta_buf).is_err() || writer.write_all(&data).is_err() {
                            failures_task.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        
                        success_task.fetch_add(1, Ordering::Relaxed);
                    }
                    
                    writer.flush()
                };
                
                // Handle flush result
                if let Err(e) = result {
                    error!(file = %tmp_name.display(), error = %e, "[dump] flush error");
                    failures_task.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                
                // Rename tmp to final
                if let Err(e) = fs::rename(&tmp_name, &name) {
                    error!(file = %tmp_name.display(), error = %e, "[dump] rename error");
                    failures_task.fetch_add(1, Ordering::Relaxed);
                }
            });
            
            handles.push(handle);
        }
        
        // Wait for all tasks
        for handle in handles {
            let _ = handle.await;
        }
        
        // Rotate version dirs if needed
        if let Some(max_versions) = dump_cfg.max_versions {
            if max_versions > 0 {
                rotate_version_dirs(dump_dir, max_versions)?;
            }
        }
        
        let elapsed = start.elapsed();
        let success_count = success.load(Ordering::Relaxed);
        let failure_count = failures.load(Ordering::Relaxed);
        
        info!(
            written = success_count,
            fails = failure_count,
            elapsed = ?elapsed,
            "dumping finished"
        );
        
        if failure_count > 0 {
            return Err(anyhow::anyhow!("dump finished with {} errors", failure_count));
        }
        
        Ok(())
    }
    
    async fn load(&self, ctx: CancellationToken) -> Result<()> {
        let dump_cfg = self.cfg.data()
            .and_then(|d| d.dump.as_ref())
            .ok_or_else(|| anyhow::anyhow!("dump config not found"))?;
        
        let dump_dir = dump_cfg.dir.as_ref()
            .ok_or_else(|| anyhow::anyhow!("dump dir not configured"))?;
        
        let dir = get_latest_version_dir(dump_dir)?
            .ok_or_else(|| anyhow::anyhow!("no versioned dump dirs found in {}", dump_dir))?;
        
        self.load_from_dir(ctx, dir).await
    }
    
    async fn load_version(&self, ctx: CancellationToken, version: String) -> Result<()> {
        let dump_cfg = self.cfg.data()
            .and_then(|d| d.dump.as_ref())
            .ok_or_else(|| anyhow::anyhow!("dump config not found"))?;
        
        let dump_dir = dump_cfg.dir.as_ref()
            .ok_or_else(|| anyhow::anyhow!("dump dir not configured"))?;
        
        let dir = PathBuf::from(dump_dir).join(version);
        self.load_from_dir(ctx, dir).await
    }
}

impl DumperImpl {
    async fn load_from_dir(&self, ctx: CancellationToken, dir: PathBuf) -> Result<()> {
        let start = Instant::now();
        let dump_cfg = self.cfg.data()
            .and_then(|d| d.dump.as_ref())
            .ok_or_else(|| anyhow::anyhow!("dump config not found"))?;
        
        let dump_name = dump_cfg.name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("dump name not configured"))?;
        
        // Find dump files
        let pattern = format!("{}-shard-*.dump*", dump_name);
        let files = find_dump_files(&dir, &pattern)?;
        
        if files.is_empty() {
            return Err(anyhow::anyhow!("no dump files found in {}", dir.display()));
        }
        
        let ts = extract_latest_timestamp(&files)?;
        let files = filter_files_by_timestamp(&files, &ts);
        
        let success = Arc::new(AtomicI32::new(0));
        let failures = Arc::new(AtomicI32::new(0));
        let cfg_clone = self.cfg.clone();
        let storage_clone = self.storage.clone();
        let crc32_control = dump_cfg.crc32_control;
        
        // Load each file
        let mut handles: Vec<JoinHandle<()>> = Vec::new();
        
        for file in files {
            let file_clone = file.clone();
            let cfg_task = cfg_clone.clone();
            let storage_task = storage_clone.clone();
            let success_task = success.clone();
            let failures_task = failures.clone();
            let ctx_task = ctx.clone();
            
            let handle = tokio::task::spawn_blocking(move || {
                let f = match std::fs::File::open(&file_clone) {
                    Ok(f) => f,
                    Err(e) => {
                        error!(file = %file_clone.display(), error = %e, "[load] open error");
                        failures_task.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                };
                
                // Create reader (with optional gzip)
                let mut reader: Box<dyn Read> = if file_clone.to_string_lossy().ends_with(".gz") {
                    Box::new(BufReader::with_capacity(DUMP_BUFFER_SIZE, GzDecoder::new(f)))
                } else {
                    Box::new(BufReader::with_capacity(DUMP_BUFFER_SIZE, f))
                };
                
                let mut meta_buf = [0u8; 8];
                
                loop {
                    if ctx_task.is_cancelled() {
                        break;
                    }
                    
                    // Read meta
                    match reader.read_exact(&mut meta_buf) {
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                            break; // EOF is normal
                        }
                        Err(e) => {
                            error!(file = %file_clone.display(), error = %e, "[load] read meta error");
                            failures_task.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    }
                    
                    let sz = byteorder::LittleEndian::read_u32(&meta_buf[0..4]) as usize;
                    let exp_crc = byteorder::LittleEndian::read_u32(&meta_buf[4..8]);
                    
                    // Read entry data
                    let mut buf = vec![0u8; sz];
                    if let Err(e) = reader.read_exact(&mut buf) {
                        error!(file = %file_clone.display(), error = %e, "[load] read entry error");
                        failures_task.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                    
                    // Check CRC if enabled
                    if crc32_control {
                        let mut hasher = Hasher::new();
                        hasher.update(&buf);
                        let actual_crc = hasher.finalize();
                        if actual_crc != exp_crc {
                            error!(file = %file_clone.display(), "[load] crc mismatch");
                            failures_task.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    }
                    
                    // Decode entry
                    let entry = match from_bytes(&buf, &cfg_task) {
                        Ok(e) => e,
                        Err(e) => {
                            error!(file = %file_clone.display(), error = %e, "[load] entry decode error");
                            failures_task.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    };
                    
                    // Store entry (synchronous call)
                    storage_task.set(entry);
                    success_task.fetch_add(1, Ordering::Relaxed);
                }
            });
            
            handles.push(handle);
        }
        
        // Wait for all tasks
        for handle in handles {
            let _ = handle.await;
        }
        
        let elapsed = start.elapsed();
        let success_count = success.load(Ordering::Relaxed);
        let failure_count = failures.load(Ordering::Relaxed);
        
        info!(
            restored = success_count,
            fails = failure_count,
            elapsed = ?elapsed,
            "restoring dump"
        );
        
        if failure_count > 0 {
            return Err(anyhow::anyhow!("load finished with {} errors", failure_count));
        }
        
        Ok(())
    }
}

/// Picks the next sequential version number.
fn next_version_dir(base_dir: &str) -> Result<usize> {
    let base_path = Path::new(base_dir);
    if !base_path.exists() {
        return Ok(1);
    }
    
    let mut max_v = 0;
    let entries = fs::read_dir(base_path)?;
    
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        if name_str.starts_with('v') {
            if let Some(v_str) = name_str.strip_prefix('v') {
                if let Ok(v) = v_str.parse::<usize>() {
                    if v > max_v {
                        max_v = v;
                    }
                }
            }
        }
    }
    
    Ok(max_v + 1)
}

/// Keeps only the newest `max_of` dirs, removes the rest.
fn rotate_version_dirs(base_dir: &str, max_of: usize) -> Result<()> {
    let base_path = Path::new(base_dir);
    if !base_path.exists() {
        return Ok(());
    }
    
    let mut entries: Vec<(PathBuf, fs::Metadata)> = Vec::new();
    let dir_entries = fs::read_dir(base_path)?;
    
    for entry in dir_entries {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        if name_str.starts_with('v') {
            if let Ok(metadata) = entry.metadata() {
                entries.push((entry.path(), metadata));
            }
        }
    }
    
    if entries.len() <= max_of {
        return Ok(());
    }
    
    // Sort by modification time (newest first)
    entries.sort_by(|a, b| {
        b.1.modified().unwrap_or(std::time::UNIX_EPOCH)
            .cmp(&a.1.modified().unwrap_or(std::time::UNIX_EPOCH))
    });
    
    // Remove old dirs
    for (dir, _) in entries.iter().skip(max_of) {
        if let Err(e) = fs::remove_dir_all(dir) {
            error!(dir = %dir.display(), error = %e, "[dump] failed to remove old dump dir");
        } else {
            info!(dir = %dir.display(), "[dump] removed old dump dir");
        }
    }
    
    Ok(())
}

/// Returns the most recently modified version dir.
fn get_latest_version_dir(base_dir: &str) -> Result<Option<PathBuf>> {
    let base_path = Path::new(base_dir);
    if !base_path.exists() {
        return Ok(None);
    }
    
    let mut entries: Vec<(PathBuf, fs::Metadata)> = Vec::new();
    let dir_entries = fs::read_dir(base_path)?;
    
    for entry in dir_entries {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        if name_str.starts_with('v') {
            if let Ok(metadata) = entry.metadata() {
                entries.push((entry.path(), metadata));
            }
        }
    }
    
    if entries.is_empty() {
        return Ok(None);
    }
    
    // Sort by modification time (newest first)
    entries.sort_by(|a, b| {
        b.1.modified().unwrap_or(std::time::UNIX_EPOCH)
            .cmp(&a.1.modified().unwrap_or(std::time::UNIX_EPOCH))
    });
    
    Ok(Some(entries[0].0.clone()))
}

/// Picks the largest timestamp suffix among files.
fn extract_latest_timestamp(files: &[PathBuf]) -> Result<String> {
    let mut ts_list: Vec<String> = Vec::new();
    
    for file in files {
        let file_name = file.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
        let parts: Vec<&str> = file_name.split('-').collect();
        if parts.len() >= 4 {
            if let Some(last_part) = parts.last() {
                let ts = last_part.trim_end_matches(".dump").trim_end_matches(".gz");
                ts_list.push(ts.to_string());
            }
        }
    }
    
    ts_list.sort();
    ts_list.last()
        .map(|s| s.clone())
        .ok_or_else(|| anyhow::anyhow!("no timestamp found in files"))
}

/// Returns only files containing the given timestamp.
fn filter_files_by_timestamp(files: &[PathBuf], ts: &str) -> Vec<PathBuf> {
    files.iter()
        .filter(|f| {
            f.to_string_lossy().contains(ts)
        })
        .cloned()
        .collect()
}

/// Finds dump files matching the pattern.
fn find_dump_files(dir: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    
    let mut files = Vec::new();
    let prefix = pattern.split('*').next().unwrap_or("");
    let suffix = pattern.split('*').last().unwrap_or("");
    
    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        
        if file_name_str.starts_with(prefix) && file_name_str.ends_with(suffix) {
            files.push(entry.path());
        }
    }
    
    Ok(files)
}

