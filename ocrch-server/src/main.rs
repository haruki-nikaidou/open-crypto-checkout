//! Open Crypto Checkout Server
//!
//! A headless cryptocurrency checkout counter for accepting stablecoin payments.

mod config;
mod server;
mod shutdown;
mod state;

use clap::Parser;
use config::{ConfigLoader, get_database_url};
use server::{build_router, run_server};
use shutdown::spawn_config_reload_handler;
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Open Crypto Checkout - Headless cryptocurrency payment gateway
#[derive(Parser, Debug)]
#[command(name = "ocrch-server")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long, default_value = "./ocrch-config.toml")]
    config: PathBuf,

    /// Override the listen address (e.g., 0.0.0.0:3000)
    #[arg(short, long)]
    listen: Option<SocketAddr>,

    /// Run database migrations on startup
    #[arg(long, default_value = "false")]
    migrate: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    init_tracing();

    // Parse command line arguments
    let args = Args::parse();

    tracing::info!("Starting ocrch-server v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config_loader = Arc::new(ConfigLoader::new(&args.config, args.listen));
    let loaded_config = config_loader.load().map_err(|e| {
        tracing::error!("Failed to load configuration: {}", e);
        e
    })?;

    let listen_addr = loaded_config.server.listen;
    tracing::info!("Configuration loaded from {:?}", args.config);

    // Convert to shared config with separate locks for each section
    let shared_config = loaded_config.into_shared();

    // Get database URL from environment
    let database_url = get_database_url().map_err(|e| {
        tracing::error!("DATABASE_URL environment variable not set");
        e
    })?;

    // Create database connection pool
    tracing::info!("Connecting to database...");
    let db_pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to connect to database: {}", e);
            e
        })?;
    tracing::info!("Database connection established");

    // Run migrations if requested
    if args.migrate {
        tracing::info!("Running database migrations...");
        sqlx::migrate!("../migrations")
            .run(&db_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to run migrations: {}", e);
                e
            })?;
        tracing::info!("Migrations completed successfully");
    }

    // Create application state
    let state = AppState::new(db_pool.clone(), shared_config);

    // Spawn config reload handler (listens for SIGHUP)
    let shutdown_notify = spawn_config_reload_handler(state.clone(), config_loader);

    // Build the router
    let router = build_router(state);

    // Run the server
    tracing::info!("Starting HTTP server on {}", listen_addr);
    let result = run_server(router, listen_addr).await;

    // Signal the config reload handler to stop
    shutdown_notify.notify_one();

    // Close database connections gracefully
    tracing::info!("Closing database connections...");
    db_pool.close().await;
    tracing::info!("Server shutdown complete");

    result.map_err(Into::into)
}

/// Initialize the tracing subscriber with environment-based filtering.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
