<p align="center">
  <img src="img/icon.svg" alt="GitHub Repository Clone Tool" width="96" height="96">
</p>

<h1 align="center">GitHub Repository Clone Tool</h1>

A Rust command-line tool that clones or archives GitHub repositories of a specific organization to a local folder. It filters by last update date, skips forks, and supports two modes of operation: **Git Clone** (requires `git` on PATH) and **Archive** (no external dependencies).

## Features

- **Two modes**: Shallow git clone (depth 1 by default, configurable) or tar.gz archive download (always full depth)
- **Smart filtering**: Skips forks and repositories not updated within the specified window
- **Retry with backoff**: Exponential backoff with jitter, respects `Retry-After` headers
- **Graceful error handling**: No panics — logs warnings and continues processing
- **Docker support**: Multi-stage Dockerfile included
- **Cross-platform**: Builds for Linux, macOS, and Windows (including ARM)

## Requirements

- **Build**: Rust and Cargo (latest stable)
- **Runtime (Git Clone mode)**: Git installed and on your PATH
- **Runtime (Archive mode)**: No additional dependencies

## Usage

```bash
githubrepocloner <organization> <folder> <days_last_updated> [--archive] [--depth=N]
```

### Arguments

| Argument | Required | Default | Description |
|---|---|---|---|
| `<organization>` | Yes | — | GitHub organization name to clone repositories from |
| `<folder>` | Yes | — | Local directory where repositories will be cloned or archives saved |
| `<days_last_updated>` | Yes | — | Only process repositories updated within this many days |
| `--archive` | No | Off (git clone) | Download repositories as `.tar.gz` archives instead of git cloning. Cannot be combined with `--depth` |
| `--depth=N` | No | `1` | Git clone depth. Use `0` for full history. Only valid in git clone mode (incompatible with `--archive`) |

### Git Clone Mode (default)

```bash
githubrepocloner <organization> <folder> <days_last_updated> [--depth=N]
```

Clones repositories using `git clone --depth=1` by default (shallow clone). Each repository is placed in `<folder>/<repo_name>/`. Use `--depth=N` to control how much history is fetched.

### Archive Mode

```bash
githubrepocloner <organization> <folder> <days_last_updated> --archive
```

Downloads repositories as `.tar.gz` archives via the GitHub Archive API. Each archive is saved as `<folder>/<repo_name>.tar.gz`. Archives always contain the full source tree. The `--depth` option cannot be used with `--archive` (the tool will exit with an error if both are specified).

### Examples

```bash
# Shallow clone all Microsoft repos updated in the last year (depth=1)
ghubrepocloner microsoft /mnt/external 365

# Clone with full history
ghubrepocloner microsoft /mnt/external 365 --depth=0

# Clone with last 10 commits
ghubrepocloner microsoft /mnt/external 365 --depth=10

# Download archives instead
githubrepocloner microsoft /mnt/external 365 --archive
```

The tool clones all non-fork public repositories from the given organization that have been updated within the specified number of days. If a folder (or archive) for the repository already exists, it is skipped.

## Git Clone vs. Archive Mode

| Use Case | Recommended Mode |
|---|---|
| Development / Active Work | Git Clone (`--depth=0` for full history) |
| Code Analysis / Scanning | Archive |
| Backup / Archival | Archive |
| Large Organizations | Archive (faster, smaller) |
| Need Full Git History | Git Clone with `--depth=0` |
| Quick Local Copy | Git Clone (default depth=1) |
| CI/CD Pipelines | Archive |

Archive mode doesn't require Git and produces smaller downloads (no `.git` directory). Archives always include the full source tree (equivalent to full depth), but do not include commit history.

### Extracting Archives

```bash
mkdir repo-name
tar -xzf repo-name.tar.gz -C repo-name --strip-components=1
```

## Build

```bash
cargo build --release
```

The binary is produced in `target/release/`.

### Supported Targets

| Platform | Target |
|---|---|
| 64-bit Linux | `x86_64-unknown-linux-gnu` |
| 32-bit Linux | `i686-unknown-linux-gnu` |
| ARM64 Linux | `aarch64-unknown-linux-gnu` |
| ARM32 ARMv7 Linux | `armv7-unknown-linux-gnueabihf` |
| ARM32 ARMv6 Linux | `arm-unknown-linux-gnueabi` |
| 64-bit macOS | `x86_64-apple-darwin` |
| ARM64 macOS | `aarch64-apple-darwin` |
| 64-bit Windows | `x86_64-pc-windows-gnu` |
| 32-bit Windows | `i686-pc-windows-gnu` |

## Docker

```bash
# Build
docker build -t githubrepocloner .

# Git clone mode
docker run -v $(pwd)/repos:/repos githubrepocloner richardsondev /repos 365

# Archive mode
docker run -v $(pwd)/repos:/repos githubrepocloner richardsondev /repos 365 --archive
```

## Authentication

By default, the tool makes unauthenticated API requests, which are limited to **60 requests per hour** by GitHub. To increase this to **5,000 requests per hour**, set the `GITHUB_TOKEN` environment variable:

```bash
export GITHUB_TOKEN=ghp_your_personal_access_token
ghubrepocloner microsoft /mnt/external 365
```

On Windows (PowerShell):

```powershell
$env:GITHUB_TOKEN = "ghp_your_personal_access_token"
ghubrepocloner microsoft /mnt/external 365
```

You can generate a personal access token at [github.com/settings/tokens](https://github.com/settings/tokens). No special scopes are required for accessing public repositories.

The tool will print whether it is running in authenticated or unauthenticated mode at startup.

## Rate Limiting & Retry Logic

The tool automatically retries on HTTP 429 (Too Many Requests) and 403 (Forbidden) responses using exponential backoff with jitter. If the server sends a `Retry-After` header, that value is used as the delay. Otherwise, the delay doubles on each attempt starting from 1 second, up to a maximum of 32 seconds, for up to 5 retries.

| Mode | Rate Limit |
|---|---|
| Unauthenticated (no token) | 60 requests/hour |
| Authenticated (`GITHUB_TOKEN` set) | 5,000 requests/hour |

For organizations with many repositories, authentication is strongly recommended to avoid hitting rate limits.

## Project Structure

```
src/
├── lib.rs     # Public API: HTTP client, clone/archive logic
├── main.rs    # CLI argument parsing and entry point
├── repo.rs    # Repo struct (GitHub API deserialization)
└── retry.rs   # RetryConfig, exponential backoff, Retry-After support
```

## Testing

```bash
# Run all tests (unit + integration)
cargo test

# Run only integration tests (uses mock HTTP server)
cargo test --test integration_tests

# Run with output
cargo test -- --nocapture
```

Unit tests cover exponential backoff, JSON deserialization, date parsing, and client creation. Integration tests use [mockito](https://crates.io/crates/mockito) to simulate the GitHub API without network access.

## License

See [LICENSE](LICENSE).
