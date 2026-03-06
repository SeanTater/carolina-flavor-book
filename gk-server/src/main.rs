use anyhow::{Context, Result};
use clap::Parser;
use gk_server::{
    auth::AuthService,
    config::Config,
    database::Database,
    search, AppState, TagAxes, build_app,
};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
struct Args {
    /// The path to the configuration file
    config_path: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    let file_appender = tracing_appender::rolling::daily(
        if std::fs::exists("/app")? {
            "/app/data/logs".into()
        } else {
            std::env::current_dir()?
        },
        "access.log",
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .json()
        .with_writer(non_blocking)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command line arguments
    let args = Args::parse();

    // Load the configuration file
    let config = Config::load(&args.config_path).context("Parsing configuration")?;

    // Connect to the database
    let default_db = Database::connect(&config.database)
        .await
        .context("Connecting to database")?;

    // Setup oauth, sessions, and authentication
    let auth = AuthService::new_from_config(&config.auth)
        .await
        .context("Setting up authentication")?;

    // Setup an embedding model and the search engine
    let embedder = search::model::EmbeddingModel::new().context("Building embedding model")?;
    let document_index = search::DocumentIndexHandle::new(default_db.clone(), embedder);
    document_index
        .refresh_index()
        .context("Refreshing document index")?;
    // use the embeddings to index the recipes in the background
    tokio::spawn(document_index.clone().background_index());

    let tag_axes = TagAxes::load();
    tracing::info!("Loaded tag axes: {} cuisine, {} occasion, {} season tags",
        tag_axes.cuisine.len(), tag_axes.occasion.len(), tag_axes.season.len());

    let app = build_app(AppState {
        db: default_db,
        doc_index: document_index,
        auth,
        tag_axes,
    });

    // In development, use HTTP. In production, use HTTPS.
    if let Some(tls) = config.server.tls {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");
        let tls_config =
            axum_server::tls_rustls::RustlsConfig::from_pem_file(tls.cert_path, tls.key_path)
                .await
                .context("Loading TLS certificate")?;

        let addr = config.server.address.parse()?;
        tracing::info!("Listening on {}", addr);
        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await
            .context("Starting TLS server")?;
    } else {
        let listener = tokio::net::TcpListener::bind(config.server.address).await?;
        axum::serve(listener, app).await?;
    }
    Ok(())
}
