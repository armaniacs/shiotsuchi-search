use clap::Parser;
use shiotsuchi_core::config::ShiotsuchiConfig;
use shiotsuchi_core::db::NoteDatabase;
use shiotsuchi_core::server::handlers::{AppState, create_router};
use shiotsuchi_core::tokenizer::get_tokenizer;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(about = crate::messages::SERVE_ABOUT)]
pub struct ServeArgs {
    /// Port to listen on (overrides config)
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Host to bind to (overrides config)
    #[arg(long)]
    pub host: Option<String>,

    /// API key for authentication (overrides SHIOTSUCHI_SERVER_API_KEY env var)
    #[arg(long)]
    pub api_key: Option<String>,
}

/// Resolve API key: CLI option > env var > None.
/// Config.toml is NOT used — API keys should only come from CLI or env vars for security.
fn resolve_api_key(cli_key: &Option<String>) -> Option<String> {
    // 1. CLI option (highest priority)
    if let Some(ref key) = cli_key {
        if !key.is_empty() {
            return Some(key.clone());
        }
    }
    // 2. Environment variable
    if let Ok(key) = std::env::var("SHIOTSUCHI_SERVER_API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    // 3. No authentication
    None
}

pub async fn run_serve(
    args: &ServeArgs,
    config: &ShiotsuchiConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let port = args.port.unwrap_or(config.server.port);
    let host = args
        .host
        .clone()
        .unwrap_or_else(|| config.server.host.clone());

    let api_key = resolve_api_key(&args.api_key);

    let db_path = config.resolved_db_path();
    if !db_path.exists() {
        eprintln!(
            "{}",
            crate::msg_fmt!(crate::messages::ERR_SERVE_DB_NOT_FOUND, db_path.display())
        );
        std::process::exit(1);
    }
    let db = NoteDatabase::open(&db_path)?;

    let tokenizer = get_tokenizer().map_err(|e| {
        eprintln!("{}", crate::msg_fmt!(crate::messages::ERR_SERVE_TOKENIZER, e));
        e
    })?;

    let state = Arc::new(AppState {
        db: Arc::new(tokio::sync::Mutex::new(db)),
        tokenizer: Some(tokenizer),
        synonyms: config.synonyms.clone(),
        hybrid_alpha: config.hybrid_alpha,
        config: Some(config.clone()),
        api_key: api_key.clone(),
        sensitive_config: Some(config.sensitive_data.clone()),
    });

    let app = create_router(state, config);

    let addr = format!("{}:{}", host, port);

    // Security warning for non-localhost binds
    if host != "127.0.0.1" && host != "localhost" && host != "::1" {
        if api_key.is_some() {
            eprintln!("\x1b[32m{}\x1b[0m", crate::messages::SERVE_AUTH_ENABLED);
        } else {
            eprintln!("\x1b[33m{}\x1b[0m", crate::msg_fmt!(crate::messages::WARN_SERVE_BIND_NON_LOCALHOST, host));
            eprintln!("\x1b[33m{}\x1b[0m", crate::msg_fmt!(crate::messages::WARN_SERVE_NETWORK_ACCESS, host, port));
            eprintln!("\x1b[33m{}\x1b[0m", crate::messages::WARN_SERVE_USE_LOCALHOST);
            eprintln!("\x1b[33m{}\x1b[0m\n", crate::messages::WARN_SERVE_ENABLE_AUTH);
        }
    }

    println!("{}", crate::msg_fmt!(crate::messages::SERVE_LISTENING, addr));

    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            crate::msg_fmt!(crate::messages::ERR_SERVE_PORT_IN_USE, port)
        } else {
            crate::msg_fmt!(crate::messages::ERR_SERVE_BIND_FAILED, addr, e)
        }
    })?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("{}", crate::messages::SERVE_SHUTDOWN_GRACEFUL);
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("{}", crate::messages::SERVE_SHUTDOWN_SIGNAL);
}
