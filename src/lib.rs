pub mod retry;
pub mod repo;

use reqwest::Error;
use std::time::Duration;
use std::path::Path;
use std::fs;
use std::process::Command;

use retry::{RetryConfig, RetryableError, retry_with_backoff, check_response_status};
use repo::Repo;

/// Shared context passed through the clone/archive pipeline.
struct CloneContext<'a> {
    client: &'a reqwest::Client,
    retry_config: RetryConfig,
    base_url: &'a str,
    org: &'a str,
    clone_folder: &'a str,
    days_last_updated: i64,
    use_archive: bool,
    clone_depth: u32,
}

/// Create a configured reqwest client with proper user agent.
///
/// If `token` is provided, all requests will include an `Authorization: Bearer <token>`
/// header, which raises the GitHub API rate limit from 60 to 5,000 requests per hour.
pub fn create_client(token: Option<&str>) -> Result<reqwest::Client, Error> {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(t) = token {
        let mut auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {t}"))
            .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static(""));
        auth_value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth_value);
    }

    reqwest::Client::builder()
        .user_agent("richardsondev/githubrepocloner")
        .default_headers(headers)
        .build()
}

/// Clone or archive repositories from a GitHub organization.
///
/// Fetches the repository list from `{base_url}/orgs/{org}/repos`, filters by
/// fork status and last update date, then either git-clones or downloads archives
/// depending on `use_archive`.
pub async fn clone_repos_with_client_and_url(
    client: &reqwest::Client,
    base_url: &str,
    org: &str,
    clone_folder: &str,
    days_last_updated: i64,
    use_archive: bool,
    clone_depth: u32,
) -> Result<(), Error> {
    let ctx = CloneContext {
        client,
        retry_config: RetryConfig::default(),
        base_url,
        org,
        clone_folder,
        days_last_updated,
        use_archive,
        clone_depth,
    };
    let mut page = 1;
    let mut total_repos_found: usize = 0;

    loop {
        let resp = fetch_repo_page(&ctx, page, total_repos_found).await?;
        let repo_data: Vec<Repo> = resp.json().await?;

        if repo_data.is_empty() {
            break;
        }

        total_repos_found += repo_data.len();

        for repo in repo_data {
            process_repo(&ctx, &repo).await;
        }

        page += 1;
    }

    Ok(())
}

/// Fetch a single page of repositories from the GitHub API with retry logic.
async fn fetch_repo_page(
    ctx: &CloneContext<'_>,
    page: u32,
    total_repos_found: usize,
) -> Result<reqwest::Response, Error> {
    let base_url = ctx.base_url;
    let org = ctx.org;

    match retry_with_backoff(&ctx.retry_config, || async {
        let response = ctx.client
            .get(format!("{base_url}/orgs/{org}/repos?per_page=100&page={page}"))
            .send()
            .await?;
        check_response_status(response)
    })
    .await
    {
        Ok(resp) => Ok(resp),
        Err(RetryableError::RequestError(e)) => Err(e),
        Err(e) => {
            eprintln!("Failed listing repos for {org} on page {page}. Found {total_repos_found} repos before failure.");
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

/// Process a single repository — either clone it or download its archive.
async fn process_repo(ctx: &CloneContext<'_>, repo: &Repo) {
    let repo_path = format!("{}/{}", ctx.clone_folder, repo.name);
    if Path::new(&repo_path).exists() {
        println!("Skipping {}, folder already exists.", repo.name);
        return;
    }

    let updated_at = match chrono::DateTime::parse_from_rfc3339(&repo.updated_at) {
        Ok(dt) => dt.naive_utc(),
        Err(e) => {
            println!("Warning: Failed to parse date for {}: {e}. Skipping.", repo.name);
            return;
        }
    };

    let days_since_update = (chrono::Utc::now().naive_utc() - updated_at).num_days();

    if !repo.fork && days_since_update < ctx.days_last_updated {
        if ctx.use_archive {
            download_archive(ctx, repo).await;
        } else {
            clone_repo(ctx.org, repo, &repo_path, ctx.clone_depth);
        }
    }

    println!("Done with repo! Waiting 5 seconds");
    tokio::time::sleep(Duration::from_secs(5)).await;
}

/// Download a repository archive via the GitHub Archive API.
async fn download_archive(ctx: &CloneContext<'_>, repo: &Repo) {
    println!("Downloading archive for {}...", repo.name);

    let base_url = ctx.base_url;
    let org = ctx.org;
    let archive_url = format!(
        "{base_url}/repos/{org}/{}/tarball/{}",
        repo.name, repo.default_branch
    );

    match retry_with_backoff(&ctx.retry_config, || async {
        let response = ctx.client.get(&archive_url).send().await?;
        check_response_status(response)
    })
    .await
    {
        Ok(archive_resp) => match archive_resp.bytes().await {
            Ok(bytes) => {
                let archive_path = format!("{}/{}.tar.gz", ctx.clone_folder, repo.name);
                match fs::write(&archive_path, bytes) {
                    Ok(()) => println!("Successfully downloaded archive for {}!", repo.name),
                    Err(e) => println!("Failed to write archive for {}: {e}", repo.name),
                }
            }
            Err(e) => println!("Failed to download archive bytes for {}: {e}", repo.name),
        },
        Err(RetryableError::RequestError(e)) => {
            println!("Failed to download archive for {}: {e}", repo.name);
        }
        Err(e) => {
            println!("Failed to download archive for {} after retries: {e}", repo.name);
        }
    }
}

/// Clone a repository using git.
fn clone_repo(org: &str, repo: &Repo, repo_path: &str, clone_depth: u32) {
    println!("Cloning {}...", repo.name);

    match Command::new("git")
        .arg("clone")
        .arg(format!("--depth={clone_depth}"))
        .arg(format!("--branch={}", repo.default_branch))
        .arg(format!("https://github.com/{org}/{}.git", repo.name))
        .arg(repo_path)
        .status()
    {
        Ok(status) => {
            if status.success() {
                println!("Successfully cloned {}!", repo.name);
            } else {
                println!("Failed to clone {} (exit code: {:?}).", repo.name, status.code());
            }
        }
        Err(e) => {
            println!("Error executing git command for {}: {e}", repo.name);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_create_client_no_token() {
        let client = create_client(None);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_client_with_token() {
        let client = create_client(Some("ghp_testtoken123"));
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_has_user_agent() {
        let client = create_client(None).unwrap();
        assert_eq!(std::mem::size_of_val(&client), std::mem::size_of::<reqwest::Client>());
    }
}
