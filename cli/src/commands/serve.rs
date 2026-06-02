use clap::Parser;
use shiotsuchi_core::config::ShiotsuchiConfig;
use shiotsuchi_core::db::NoteDatabase;
use shiotsuchi_core::server::handlers::{AppState, create_router};
use shiotsuchi_core::tokenizer::get_tokenizer;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(about = "Start HTTP API server")]
pub struct ServeArgs {
    /// Port to listen on (overrides config)
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Host to bind to (overrides config)
    #[arg(long)]
    pub host: Option<String>,
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

    let db_path = config.resolved_db_path();
    if !db_path.exists() {
        eprintln!(
            "Error: database not found at {}. Run 'shiotsuchi index' first.",
            db_path.display()
        );
        std::process::exit(1);
    }
    let db = NoteDatabase::open(&db_path)?;

    let tokenizer = get_tokenizer().map_err(|e| {
        eprintln!("Error: tokenizer not available: {}. Run 'shiotsuchi setup'.", e);
        e
    })?;

    let state = Arc::new(AppState {
        db: Arc::new(tokio::sync::Mutex::new(db)),
        tokenizer: Some(tokenizer),
        synonyms: config.synonyms.clone(),
        hybrid_alpha: config.hybrid_alpha,
    });

    let app = create_router(state, config);

    let addr = format!("{}:{}", host, port);
    println!("shiotsuchi server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            format!(
                "Port {} is already in use. Specify a different port with --port",
                port
            )
        } else {
            format!("Failed to bind to {}: {}", addr, e)
        }
    })?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("Server shut down gracefully.");
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

    println!("Shutdown signal received, starting graceful shutdown...");
}
