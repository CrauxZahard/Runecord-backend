//! Runecord server library.
//!
//! The binary is a thin launcher; everything testable lives here.

pub mod bootstrap;
pub mod config;
pub mod state;

mod api;
mod cache;
mod compat;
mod crypto;
mod gateway_v10;
mod media;
mod platform;
mod ratelimit;
mod store;

pub use state::{AppState, PendingInteraction};

/// Parses the process arguments and runs the server.
pub fn run() {
    init_logging();

    let config = match config::Config::parse_env() {
        config::Parsed::Run(config) => config,
        config::Parsed::Help => {
            println!("{}", config::HELP);
            return;
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            tracing::error!("could not start async runtime: {err}");
            std::process::exit(1);
        }
    };

    runtime.block_on(bootstrap::run(config));
}

/// Installs the process-wide log subscriber.
///
/// `RUST_LOG` selects the filter; the default keeps startup and error output
/// visible without being chatty.
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
