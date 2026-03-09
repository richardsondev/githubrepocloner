use githubrepocloner::{create_client, clone_repos_with_client_and_url};
use reqwest::Error;
use std::env;
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Usage: githubrepocloner <organization> <folder> <days_last_updated> [--archive] [--depth=N]");
        println!("  --archive:  Download repositories as tar.gz archives instead of git cloning");
        println!("  --depth=N:  Clone depth (default: 1). Cannot be used with --archive");
        return Ok(());
    }

    let organization = &args[1];
    let folder = &args[2];
    let days_last_updated: i64 = args[3].parse().unwrap_or(365);
    
    // Parse optional flags from remaining args
    let optional_args = &args[4..];
    let use_archive = optional_args.iter().any(|a| a == "--archive");
    let has_depth_flag = optional_args.iter().any(|a| a.starts_with("--depth="));
    let clone_depth: u32 = optional_args.iter()
        .find(|a| a.starts_with("--depth="))
        .and_then(|a| a.strip_prefix("--depth="))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    
    if use_archive && has_depth_flag {
        eprintln!("Error: --depth cannot be used with --archive. Archive mode always downloads the full source tree.");
        std::process::exit(1);
    }

    if use_archive {
        println!("Archive mode enabled - repositories will be downloaded as tar.gz files");
    } else {
        // Verify git is available on PATH before attempting to clone
        match Command::new("git").arg("--version").output() {
            Ok(output) if output.status.success() => {},
            _ => {
                eprintln!("Error: git is not installed or not found on PATH. Install git or use --archive mode instead.");
                std::process::exit(1);
            }
        }
        if clone_depth != 1 {
            println!("Clone depth set to {clone_depth}");
        }
    }

    let token = env::var("GITHUB_TOKEN").ok();
    if token.is_some() {
        println!("Using authenticated GitHub API requests (GITHUB_TOKEN detected)");
    } else {
        println!("Using unauthenticated GitHub API requests (60 req/hour). Set GITHUB_TOKEN for higher limits.");
    }

    let client = create_client(token.as_deref())?;
    clone_repos_with_client_and_url(
        &client,
        "https://api.github.com",
        organization,
        folder,
        days_last_updated,
        use_archive,
        clone_depth,
    ).await
}
