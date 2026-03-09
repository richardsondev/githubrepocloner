// Integration tests for githubrepocloner using mock HTTP server
#![allow(clippy::unwrap_used, clippy::expect_used)]

use githubrepocloner::{create_client, clone_repos_with_client_and_url};

/// Helper: create an unauthenticated client for testing
fn test_client() -> reqwest::Client {
    create_client(None).unwrap()
}
use mockito::Server;
use tempfile::TempDir;

/// Helper: create a JSON string for a single mock GitHub repo
fn mock_repo_json(name: &str, fork: bool, updated_at: &str, default_branch: &str) -> String {
    format!(
        r#"{{"name":"{name}","default_branch":"{default_branch}","fork":{fork},"updated_at":"{updated_at}"}}"#
    )
}

/// Helper: set up the standard page-2 empty response to stop pagination
async fn mock_empty_page2(server: &mut Server) -> mockito::Mock {
    server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=2")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create_async()
        .await
}

#[tokio::test]
async fn test_archive_mode_downloads_files() {
    let mut server = Server::new_async().await;
    let temp_dir = TempDir::new().unwrap();
    let clone_folder = temp_dir.path().to_str().unwrap();

    // Recent non-fork repo
    let repo_json = mock_repo_json("my-project", false, "2026-02-20T12:00:00Z", "main");

    let _m1 = server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!("[{repo_json}]"))
        .create_async()
        .await;

    let _m2 = mock_empty_page2(&mut server).await;

    // Serve dummy tarball bytes
    let _m3 = server
        .mock("GET", "/repos/testorg/my-project/tarball/main")
        .with_status(200)
        .with_header("content-type", "application/gzip")
        .with_body(b"\x1f\x8b fake-archive-data")
        .create_async()
        .await;

    let client = test_client();
    let result = clone_repos_with_client_and_url(
        &client,
        &server.url(),
        "testorg",
        clone_folder,
        365,
        true, // archive mode
        1,
    )
    .await;

    assert!(result.is_ok());
    let archive_path = temp_dir.path().join("my-project.tar.gz");
    assert!(archive_path.exists(), "Expected archive file to be created");
    assert!(
        std::fs::metadata(&archive_path).unwrap().len() > 0,
        "Archive file should not be empty"
    );
}

#[tokio::test]
async fn test_fork_repos_are_skipped() {
    let mut server = Server::new_async().await;
    let temp_dir = TempDir::new().unwrap();
    let clone_folder = temp_dir.path().to_str().unwrap();

    let repo_json = mock_repo_json("forked-repo", true, "2026-02-20T12:00:00Z", "main");

    let _m1 = server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!("[{repo_json}]"))
        .create_async()
        .await;

    let _m2 = mock_empty_page2(&mut server).await;

    // No tarball mock — it should never be requested for a fork
    let client = test_client();
    let result = clone_repos_with_client_and_url(
        &client,
        &server.url(),
        "testorg",
        clone_folder,
        365,
        true,
        1,
    )
    .await;

    assert!(result.is_ok());
    let archive_path = temp_dir.path().join("forked-repo.tar.gz");
    assert!(!archive_path.exists(), "Fork should not be downloaded");
}

#[tokio::test]
async fn test_old_repos_are_filtered_by_date() {
    let mut server = Server::new_async().await;
    let temp_dir = TempDir::new().unwrap();
    let clone_folder = temp_dir.path().to_str().unwrap();

    // Repo last updated in 2020 — well outside the 30-day window
    let repo_json = mock_repo_json("old-repo", false, "2020-01-01T12:00:00Z", "main");

    let _m1 = server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!("[{repo_json}]"))
        .create_async()
        .await;

    let _m2 = mock_empty_page2(&mut server).await;

    let client = test_client();
    let result = clone_repos_with_client_and_url(
        &client,
        &server.url(),
        "testorg",
        clone_folder,
        30, // only repos updated in last 30 days
        true,
        1,
    )
    .await;

    assert!(result.is_ok());
    let archive_path = temp_dir.path().join("old-repo.tar.gz");
    assert!(!archive_path.exists(), "Old repo should not be downloaded");
}

#[tokio::test]
async fn test_existing_repo_is_skipped() {
    let mut server = Server::new_async().await;
    let temp_dir = TempDir::new().unwrap();
    let clone_folder = temp_dir.path().to_str().unwrap();

    // Pre-create the repo folder so it will be skipped
    std::fs::create_dir(temp_dir.path().join("existing-repo")).unwrap();

    let repo_json = mock_repo_json("existing-repo", false, "2026-02-20T12:00:00Z", "main");

    let _m1 = server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!("[{repo_json}]"))
        .create_async()
        .await;

    let _m2 = mock_empty_page2(&mut server).await;

    // No tarball mock — should never be requested for an existing repo
    let client = test_client();
    let result = clone_repos_with_client_and_url(
        &client,
        &server.url(),
        "testorg",
        clone_folder,
        365,
        true,
        1,
    )
    .await;

    assert!(result.is_ok());
    let archive_path = temp_dir.path().join("existing-repo.tar.gz");
    assert!(!archive_path.exists(), "Existing repo folder should cause skip");
}

#[tokio::test]
async fn test_pagination_fetches_multiple_pages() {
    let mut server = Server::new_async().await;
    let temp_dir = TempDir::new().unwrap();
    let clone_folder = temp_dir.path().to_str().unwrap();

    // Page 1: one repo
    let repo1 = mock_repo_json("repo-page1", false, "2026-02-20T12:00:00Z", "main");
    let _m1 = server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!("[{repo1}]"))
        .create_async()
        .await;

    // Page 2: another repo
    let repo2 = mock_repo_json("repo-page2", false, "2026-02-18T12:00:00Z", "main");
    let _m2 = server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=2")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!("[{repo2}]"))
        .create_async()
        .await;

    // Page 3: empty (end pagination)
    let _m3 = server
        .mock("GET", "/orgs/testorg/repos?per_page=100&page=3")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create_async()
        .await;

    // Tarball mocks for both repos
    let _m4 = server
        .mock("GET", "/repos/testorg/repo-page1/tarball/main")
        .with_status(200)
        .with_header("content-type", "application/gzip")
        .with_body(b"\x1f\x8b page1-archive")
        .create_async()
        .await;

    let _m5 = server
        .mock("GET", "/repos/testorg/repo-page2/tarball/main")
        .with_status(200)
        .with_header("content-type", "application/gzip")
        .with_body(b"\x1f\x8b page2-archive")
        .create_async()
        .await;

    let client = test_client();
    let result = clone_repos_with_client_and_url(
        &client,
        &server.url(),
        "testorg",
        clone_folder,
        365,
        true,
        1,
    )
    .await;

    assert!(result.is_ok());
    assert!(
        temp_dir.path().join("repo-page1.tar.gz").exists(),
        "Repo from page 1 should be downloaded"
    );
    assert!(
        temp_dir.path().join("repo-page2.tar.gz").exists(),
        "Repo from page 2 should be downloaded"
    );
}

#[test]
fn test_client_creation() {
    let client = create_client(None);
    assert!(client.is_ok(), "Should be able to create HTTP client");
}

#[test]
fn test_client_creation_with_token() {
    let client = create_client(Some("ghp_testtoken123"));
    assert!(client.is_ok(), "Should be able to create authenticated HTTP client");
}
