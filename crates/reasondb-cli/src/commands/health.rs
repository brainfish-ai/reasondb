//! Health check command

use anyhow::Result;
use colored::Colorize;

pub async fn run(url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let health_url = format!("{}/health", url);

    println!("Checking server health at {}...", url.cyan());

    match client.get(&health_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                // Try to parse as JSON, fall back to text
                let body = response.text().await?;
                println!();
                println!(
                    "  {} Server is {}",
                    "✓".green().bold(),
                    "healthy".green().bold()
                );

                // Try to parse as JSON for additional info
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(status) = json.get("status") {
                        println!(
                            "  {} {}",
                            "Status:".dimmed(),
                            status.as_str().unwrap_or("ok")
                        );
                    }
                    if let Some(version) = json.get("version") {
                        println!(
                            "  {} {}",
                            "Version:".dimmed(),
                            version.as_str().unwrap_or("unknown")
                        );
                    }
                } else {
                    println!("  {} {}", "Response:".dimmed(), body.trim());
                }
                println!();
            } else {
                println!();
                println!(
                    "  {} Server returned status: {}",
                    "✗".red().bold(),
                    response.status()
                );
                println!();
            }
        }
        Err(e) => {
            println!();
            println!("  {} Could not connect to server", "✗".red().bold());
            println!("  {} {}", "Error:".dimmed(), e);
            println!();
            println!(
                "  {} Make sure the server is running with: {}",
                "Tip:".yellow(),
                "reasondb serve".cyan()
            );
            println!();
            return Err(anyhow::anyhow!("Server not reachable"));
        }
    }

    Ok(())
}
