//! File tailing module - Similar to `tail -f`
//!
//! Watches a file and streams new content as it's appended.

use anyhow::{Context, Result};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;

/// File tail watcher
pub struct FileTail {
    path: PathBuf,
    offset: u64,
    buffer_size: usize,
}

impl FileTail {
    /// Create a new file tail watcher
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Get initial file size
        let metadata = std::fs::metadata(&path).context("Failed to get file metadata")?;

        Ok(Self {
            path,
            offset: metadata.len(), // Start from end of file
            buffer_size: 64 * 1024, // 64KB buffer
        })
    }

    /// Create a file tail that starts from the beginning
    pub fn from_start(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Verify file exists
        if !path.exists() {
            anyhow::bail!("File does not exist: {}", path.display());
        }

        Ok(Self {
            path,
            offset: 0,
            buffer_size: 64 * 1024,
        })
    }

    /// Create a file tail that starts from last N bytes
    /// This will adjust the offset to start at a valid UTF-8 character boundary
    /// and preferably at a line boundary to avoid truncating log lines.
    pub fn with_tail_bytes(path: impl AsRef<Path>, tail_bytes: u64) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let metadata = std::fs::metadata(&path).context("Failed to get file metadata")?;

        let file_size = metadata.len();
        let mut offset = if tail_bytes >= file_size {
            0 // Send entire file
        } else {
            file_size - tail_bytes
        };

        // If we're not starting from the beginning, find a valid line boundary
        if offset > 0 {
            offset = Self::find_line_boundary(&path, offset)?;
        }

        Ok(Self {
            path,
            offset,
            buffer_size: 64 * 1024,
        })
    }

    /// Find the nearest line boundary (newline character) at or after the given offset.
    /// This ensures we don't start reading in the middle of a line or UTF-8 character.
    fn find_line_boundary(path: &Path, offset: u64) -> Result<u64> {
        let mut file = File::open(path).context("Failed to open file")?;
        let file_size = file.metadata()?.len();

        // If offset is 0 or beyond file, return as-is
        if offset == 0 || offset >= file_size {
            return Ok(offset);
        }

        file.seek(SeekFrom::Start(offset))?;

        // Read a small buffer to find the next newline
        let search_size = 4096.min((file_size - offset) as usize);
        let mut buffer = vec![0u8; search_size];
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(offset);
        }

        buffer.truncate(bytes_read);

        // Find the first newline in the buffer
        if let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
            // Return the position after the newline
            Ok(offset + pos as u64 + 1)
        } else {
            // No newline found in the search window, just use the offset
            // but make sure we're at a valid UTF-8 boundary
            Ok(Self::find_utf8_boundary(&buffer, 0) + offset)
        }
    }

    /// Find a valid UTF-8 character boundary starting from the given position.
    /// Returns the offset from the start of the buffer to a valid boundary.
    fn find_utf8_boundary(buffer: &[u8], start: usize) -> u64 {
        // UTF-8 continuation bytes start with 10xxxxxx (0x80-0xBF)
        // We need to skip any continuation bytes at the start
        let mut pos = start;
        while pos < buffer.len() {
            let byte = buffer[pos];
            // If it's not a continuation byte (doesn't start with 10xxxxxx),
            // we're at a valid character boundary
            if byte & 0xC0 != 0x80 {
                break;
            }
            pos += 1;
        }
        pos as u64
    }

    /// Read new content from the file
    pub fn read_new_content(&mut self) -> Result<Option<Vec<u8>>> {
        let mut file = File::open(&self.path).context("Failed to open file")?;

        let metadata = file.metadata()?;
        let current_size = metadata.len();

        // Handle file truncation (log rotation)
        if current_size < self.offset {
            tracing::info!("File truncated, resetting offset");
            self.offset = 0;
        }

        // No new content
        if current_size == self.offset {
            return Ok(None);
        }

        // Seek to last position
        file.seek(SeekFrom::Start(self.offset))?;

        // Read new content
        let bytes_to_read = (current_size - self.offset) as usize;
        let mut buffer = vec![0u8; bytes_to_read.min(self.buffer_size)];
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(None);
        }

        buffer.truncate(bytes_read);
        self.offset += bytes_read as u64;

        Ok(Some(buffer))
    }

    /// Start watching the file and stream changes
    pub async fn watch(mut self, tx: tokio_mpsc::Sender<Vec<u8>>) -> Result<()> {
        let (notify_tx, notify_rx) = channel();

        // Create file watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = notify_tx.send(event);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(100)),
        )
        .context("Failed to create file watcher")?;

        // Watch the parent directory
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));

        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .context("Failed to watch directory")?;

        tracing::info!("Started watching: {}", self.path.display());

        // Initial read - always send existing content from current offset to end
        if let Some(data) = self.read_new_content()? {
            tracing::info!("Sending initial {} bytes", data.len());
            if tx.send(data).await.is_err() {
                return Ok(());
            }
        }

        // Watch loop - use tokio interval for async-friendly polling
        let mut interval = tokio::time::interval(Duration::from_millis(200));

        loop {
            // Use tokio select to handle both file events and polling
            tokio::select! {
                _ = interval.tick() => {
                    // Drain all pending file events (non-blocking)
                    while let Ok(event) = notify_rx.try_recv() {
                        if Self::is_relevant_event(&event, &self.path) {
                            tracing::debug!("File event detected: {:?}", event.kind);
                        }
                    }

                    // Check for new content
                    if let Some(data) = self.read_new_content()? {
                        tracing::info!("Sending {} bytes", data.len());
                        if tx.send(data).await.is_err() {
                            tracing::info!("Channel closed, stopping file watcher");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if event is relevant to our file
    fn is_relevant_event(event: &Event, path: &Path) -> bool {
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => event.paths.iter().any(|p| p == path),
            _ => false,
        }
    }
}

/// Polling-based file tail (fallback when notify doesn't work well)
pub struct PollingFileTail {
    path: PathBuf,
    offset: u64,
    poll_interval: Duration,
}

impl PollingFileTail {
    pub fn new(path: impl AsRef<Path>, poll_interval: Duration) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let metadata = std::fs::metadata(&path).context("Failed to get file metadata")?;

        Ok(Self {
            path,
            offset: metadata.len(),
            poll_interval,
        })
    }

    pub async fn watch(mut self, tx: tokio_mpsc::Sender<Vec<u8>>) -> Result<()> {
        tracing::info!(
            "Started polling: {} (interval: {:?})",
            self.path.display(),
            self.poll_interval
        );

        loop {
            tokio::time::sleep(self.poll_interval).await;

            match self.read_new_content() {
                Ok(Some(data)) => {
                    if tx.send(data).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Error reading file: {}", e);
                }
            }
        }

        Ok(())
    }

    fn read_new_content(&mut self) -> Result<Option<Vec<u8>>> {
        let mut file = File::open(&self.path)?;
        let metadata = file.metadata()?;
        let current_size = metadata.len();

        if current_size < self.offset {
            self.offset = 0;
        }

        if current_size == self.offset {
            return Ok(None);
        }

        file.seek(SeekFrom::Start(self.offset))?;

        let bytes_to_read = (current_size - self.offset) as usize;
        let mut buffer = vec![0u8; bytes_to_read];
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(None);
        }

        buffer.truncate(bytes_read);
        self.offset += bytes_read as u64;

        Ok(Some(buffer))
    }
}
