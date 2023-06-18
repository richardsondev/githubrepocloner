# GitHub Repository Clone Tool

This is a simple Rust command-line tool that clones GitHub repositories of a specific organization to a local folder. It also takes into account the last update date of the repositories.

## Build Requirements

1. Rust and Cargo (latest stable) for building the application.

## Runtime Requirements

1. Git installed and properly set up on your machine.

## Usage

```bash
githubrepocloner <organization> <clone_folder> <days_last_updated>
```

Where:
- `<organization>` is the name of the GitHub organization whose repositories you want to clone.
- `<clone_folder>` is the path to the local directory where the repositories will be cloned.
- `<days_last_updated>` is the number of days since the repository was last updated (repositories updated more recently than this will be cloned).

For example, to clone all Microsoft repositories updated within a year into `/mnt/external`, you'd run:

```bash
githubrepocloner microsoft /mnt/external 365
```

The application will clone all non-fork public repositories from the given organization that have been updated in the last 365 days. It clones each repository with a depth of 1, meaning only the most recent commit in the specified branch is downloaded. If a folder for the repository already exists in the destination folder, the repository will be skipped.

## Build

To build an executable for this tool, run:

```bash
cargo build --release
```

This will produce an executable in the .targetrelease directory.

## Note

The application respects GitHub's rate limit by waiting 60 seconds after each repository clone. If the rate limit is reached, GitHub will return a 403 or 429 response and the application will sleep for 60 seconds before retrying.

The application does not use GitHub's API with an authenticated client. Consider adding authentication if you plan to make a large number of requests or want to clone private repositories as well.
