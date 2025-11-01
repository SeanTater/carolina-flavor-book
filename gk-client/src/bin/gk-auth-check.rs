use anyhow::Result;
use clap::Parser;

/// Check authentication with the server
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    /// URL of the server to check
    #[arg(long, default_value = "https://gallagher.kitchen")]
    server: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    if dotenvy::dotenv().is_err() {
        eprintln!("Warning: Failed to load .env file");
    }
    let args = Args::parse();

    let client = reqwest::Client::new();

    // Get the service principal secret from environment
    let secret = match dotenvy::var("PRINCIPAL_SECRET") {
        std::result::Result::Ok(s) => s,
        Err(_) => {
            eprintln!("Error: PRINCIPAL_SECRET not found in environment");
            eprintln!("Please set PRINCIPAL_SECRET in your .env file or environment");
            std::process::exit(1);
        }
    };

    println!("Testing authentication with {}...", args.server);

    let url = format!("{}/api/auth/check", args.server);
    let resp = match client
        .get(&url)
        .header("Authorization", format!("Bearer {secret}"))
        .send()
        .await
    {
        std::result::Result::Ok(r) => r,
        Err(e) => {
            eprintln!("\nError connecting to server!");
            eprintln!("URL: {url}");
            if args.server.starts_with("https://localhost") || args.server.starts_with("https://127.0.0.1") {
                eprintln!("\nHint: Local dev servers typically use HTTP, not HTTPS.");
                eprintln!("Try: cargo run --bin gk-auth-check -- --server http://localhost:3000");
            }
            return Err(e.into());
        }
    };

    let status = resp.status();
    println!("Status: {status}");

    if status.is_success() {
        let body: serde_json::Value = resp.json().await?;
        println!("\nAuthentication successful!");
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else {
        let body = resp.text().await?;
        println!("\nAuthentication failed!");
        println!("{body}");
        std::process::exit(1);
    }

    Result::Ok(())
}
