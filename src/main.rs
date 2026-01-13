//! Logline Agent - Lightweight log streaming agent
//!
//! A single-purpose CLI tool that streams log files to a Logline server.
//!
//! Usage:
//!   logline-agent --name <PROJECT_NAME> --server <IP:PORT> --file <LOG_FILE_PATH>
//!
//! Example:
//!   logline-agent --name "payment-service" --server "192.168.1.10:12500" --file "/var/log/payment.log"

mod connection;
mod protocol;
mod tail;

use clap::Parser;
use connection::{ConnectionConfig, ReconnectingConnection};
use std::path::PathBuf;
use tail::FileTail;
use tokio::sync::mpsc;

/// Logline Agent - Stream logs to Logline server
#[derive(Parser, Debug)]
#[command(name = "logline-agent")]
#[command(author = "Logline Team")]
#[command(version = "0.1.0")]
#[command(about = "Lightweight log streaming agent for Logline", long_about = None)]
struct Args {
    /// Project/service name identifier
    #[arg(short, long)]
    name: String,

    /// Logline server address (host:port)
    #[arg(short, long, default_value = "127.0.0.1:12500")]
    server: String,

    /// Log file path to monitor
    #[arg(short, long)]
    file: PathBuf,

    /// Stream existing file content from beginning
    #[arg(long, default_value = "false")]
    from_start: bool,

    /// Send last N bytes of existing content (default: 64KB)
    #[arg(short = 't', long, default_value = "65536")]
    tail_bytes: u64,

    /// Verbose logging
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    tracing::info!("Logline Agent starting...");
    tracing::info!("  Project: {}", args.name);
    tracing::info!("  Server: {}", args.server);
    tracing::info!("  File: {}", args.file.display());

    // Verify file exists
    if !args.file.exists() {
        anyhow::bail!("Log file does not exist: {}", args.file.display());
    }

    // Create channel for file data
    let (tx, rx) = mpsc::channel::<Vec<u8>>(1000);

    // Create file tail watcher
    let tail = if args.from_start {
        FileTail::from_start(&args.file)?
    } else if args.tail_bytes > 0 {
        tracing::info!("  Tail bytes: {}", args.tail_bytes);
        FileTail::with_tail_bytes(&args.file, args.tail_bytes)?
    } else {
        FileTail::new(&args.file)?
    };

    // Create connection manager
    let conn_config = ConnectionConfig::new(args.server, args.name);
    let connection = ReconnectingConnection::new(conn_config);

    // Spawn file watcher task
    let file_handle = tokio::spawn(async move {
        if let Err(e) = tail.watch(tx).await {
            tracing::error!("File watcher error: {}", e);
        }
    });

    // Spawn connection task
    let conn_handle = tokio::spawn(async move {
        if let Err(e) = connection.run(rx).await {
            tracing::error!("Connection error: {}", e);
        }
    });

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down...");

    // Abort tasks
    file_handle.abort();
    conn_handle.abort();

    Ok(())
}
