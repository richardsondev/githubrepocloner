use reqwest::{Error, Response, StatusCode};
use serde_derive::Deserialize;
use std::env;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::path::Path;

#[derive(Deserialize)]
struct Repo {
    name: String,
    #[serde(rename = "default_branch")]
    default_branch: String,
    fork: bool,
    #[serde(rename = "updated_at")]
    updated_at: String,
}

fn calculate_delay(n: u32) -> u64 {
    let base: u64 = 30;
    let delay: u64 = base * 2u64.pow(n);
    delay
}

async fn clone_repos(org: &str, clone_folder: &str, days_last_updated: i64) -> Result<(), Error> {
    let mut page = 1;
    let client = reqwest::Client::builder()
        .user_agent("richardsondev/githubrepocloner")
        .build()?;

    loop {
        let mut resp: Response;
        loop {
            resp = client.get(&format!(
                "https://api.github.com/orgs/{}/repos?per_page=100&page={}",
                org, page
            )).send().await?;

            // Retry if 403 or 429 is received
            let mut wait_counter: u32 = 0;
            if resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::TOO_MANY_REQUESTS {
                wait_counter += 1;
                let wait_seconds = calculate_delay(wait_counter);
                println!("Rate limit hit {} time(s) for this request, sleeping for {} seconds...", wait_counter, wait_seconds);
                thread::sleep(Duration::from_secs(wait_seconds));
                continue;
            }
            break;
        }

        let repo_data: Vec<Repo> = resp.json().await?;

        if repo_data.is_empty() {
            break;
        }

        for repo in repo_data {
            let repo_path = format!("{}/{}", clone_folder, repo.name);
            if Path::new(&repo_path).exists() {
                println!("Skipping {}, folder already exists.", repo.name);
                continue;
            }

            if !repo.fork && (chrono::Utc::now().naive_utc() - chrono::DateTime::parse_from_rfc3339(&repo.updated_at).unwrap().naive_utc()).num_days() < days_last_updated {
                println!("Cloning {}...", repo.name);
                let status = Command::new("git")
                    .arg("clone")
                    .arg("--depth=1")
                    .arg(format!("--branch={}", repo.default_branch))
                    .arg(format!(
                        "https://github.com/{}/{}.git",
                        org, repo.name
                    ))
                    .arg(repo_path)
                    .status()
                    .unwrap();

                if status.success() {
                    println!("Successfully cloned {}!", repo.name);
                } else {
                    println!("Failed to clone {}.", repo.name);
                }
            }

            println!("Done cloning repo! Waiting 5 seconds");
            thread::sleep(Duration::from_secs(5));
        }

        page += 1;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Usage: githubrepocloner <organization> <folder> <days_last_updated>");
        return Ok(());
    }

    let organization = &args[1];
    let folder = &args[2];
    let days_last_updated: i64 = match args[3].parse() {
        Ok(n) => n,
        Err(_) => 365
    };

    return clone_repos(organization, folder, days_last_updated).await;
}
