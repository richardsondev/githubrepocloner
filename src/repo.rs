use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct Repo {
    pub name: String,
    #[serde(rename = "default_branch")]
    pub default_branch: String,
    pub fork: bool,
    #[serde(rename = "updated_at")]
    pub updated_at: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_deserialization() {
        let json_data = r#"{
            "name": "test-repo",
            "default_branch": "main",
            "fork": false,
            "updated_at": "2025-11-20T12:00:00Z"
        }"#;

        let repo: Repo = serde_json::from_str(json_data).unwrap();
        assert_eq!(repo.name, "test-repo");
        assert_eq!(repo.default_branch, "main");
        assert!(!repo.fork);
        assert_eq!(repo.updated_at, "2025-11-20T12:00:00Z");
    }

    #[test]
    fn test_repo_deserialization_fork() {
        let json_data = r#"{
            "name": "forked-repo",
            "default_branch": "master",
            "fork": true,
            "updated_at": "2024-01-01T00:00:00Z"
        }"#;

        let repo: Repo = serde_json::from_str(json_data).unwrap();
        assert_eq!(repo.name, "forked-repo");
        assert_eq!(repo.default_branch, "master");
        assert!(repo.fork);
        assert_eq!(repo.updated_at, "2024-01-01T00:00:00Z");
    }

    #[test]
    fn test_repo_deserialization_with_alternate_branch() {
        let json_data = r#"{
            "name": "custom-branch-repo",
            "default_branch": "develop",
            "fork": false,
            "updated_at": "2025-06-15T08:30:00Z"
        }"#;

        let repo: Repo = serde_json::from_str(json_data).unwrap();
        assert_eq!(repo.default_branch, "develop");
    }

    #[test]
    fn test_repo_array_deserialization() {
        let json_data = r#"[
            {
                "name": "repo1",
                "default_branch": "main",
                "fork": false,
                "updated_at": "2025-11-20T12:00:00Z"
            },
            {
                "name": "repo2",
                "default_branch": "master",
                "fork": true,
                "updated_at": "2024-01-01T00:00:00Z"
            }
        ]"#;

        let repos: Vec<Repo> = serde_json::from_str(json_data).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "repo1");
        assert_eq!(repos[1].name, "repo2");
        assert!(!repos[0].fork);
        assert!(repos[1].fork);
    }

    #[test]
    fn test_empty_repo_array_deserialization() {
        let json_data = r"[]";
        let repos: Vec<Repo> = serde_json::from_str(json_data).unwrap();
        assert_eq!(repos.len(), 0);
    }

    #[test]
    fn test_repo_invalid_date_format() {
        // Test that invalid date format doesn't cause panic
        let json_data = r#"{
            "name": "invalid-date-repo",
            "default_branch": "main",
            "fork": false,
            "updated_at": "invalid-date"
        }"#;

        let repo: Repo = serde_json::from_str(json_data).unwrap();
        // The parse should fail gracefully, not panic
        let result = chrono::DateTime::parse_from_rfc3339(&repo.updated_at);
        assert!(result.is_err());
    }

    #[test]
    fn test_date_parsing_valid() {
        // Test that valid RFC3339 dates parse correctly
        let date_str = "2025-11-20T12:00:00Z";
        let result = chrono::DateTime::parse_from_rfc3339(date_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_date_parsing_with_timezone() {
        // Test that dates with timezone offsets parse correctly
        let date_str = "2025-11-20T12:00:00+05:30";
        let result = chrono::DateTime::parse_from_rfc3339(date_str);
        assert!(result.is_ok());
    }
}
