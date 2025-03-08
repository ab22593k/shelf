use anyhow::Result;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use serde::Serialize;

const GITHUB_API_VERSION: &str = "2022-11-28";
const GITHUB_API_URL: &str = "https://api.github.com/user/repos";

#[derive(Serialize)]
struct CreateRepoRequest {
    name: String,
    description: String,
    homepage: String,
    private: bool,
}

struct GitHubClient {
    client: reqwest::Client,
    token: String,
}

impl GitHubClient {
    /// Creates a new GitHub client with the provided authentication token
    pub fn new(token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            token,
        }
    }

    /// Builds the required HTTP headers for GitHub API requests
    ///
    /// Returns a HeaderMap containing:
    /// - Accept header for GitHub JSON
    /// - Authorization with Bearer token
    /// - GitHub API version
    /// - User Agent
    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::with_capacity(4);

        let header_values = [
            (ACCEPT, "application/vnd.github+json"),
            (AUTHORIZATION, &format!("Bearer {}", self.token)),
            (
                HeaderName::from_static("x-github-api-version"),
                GITHUB_API_VERSION,
            ),
            (USER_AGENT, "shelf-github-api-client"),
        ];

        for (key, value) in header_values {
            headers.insert(
                key,
                HeaderValue::from_str(value).expect("Invalid header value"),
            );
        }

        headers
    }
}

pub async fn create_remote(name: &str) -> Result<String> {
    // Hard-coded tokens should be moved to configuration/environment
    let github_token = "TOKEN";
    let github_client = GitHubClient::new(github_token.to_string());

    let request_body = build_repository_request(name);
    let response = send_create_request(&github_client, &request_body).await?;
    let url = handle_response(response, &request_body.name).await?;

    Ok(url)
}

fn build_repository_request(name: &str) -> CreateRepoRequest {
    CreateRepoRequest {
        name: name.to_string(),
        description: "This is your first repo!".to_string(),
        homepage: "https://github.com".to_string(),
        private: false,
    }
}

async fn send_create_request(
    client: &GitHubClient,
    request_body: &CreateRepoRequest,
) -> Result<reqwest::Response> {
    Ok(client
        .client
        .post(GITHUB_API_URL)
        .headers(client.build_headers())
        .json(request_body)
        .send()
        .await?)
}

async fn handle_response(response: reqwest::Response, repo_name: &str) -> Result<String> {
    let status = response.status();
    let response_json: serde_json::Value = response.json().await?;

    if status.is_success() {
        println!(
            "\x1b[32mRepository '{}' created successfully!\x1b[0m",
            repo_name
        );

        let html_url = response_json["html_url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing html_url in response"))?;

        Ok(html_url.to_string())
    } else if status.as_u16() == 422 {
        if let Some(errors) = response_json.get("errors") {
            for error in errors.as_array().unwrap_or(&Vec::new()) {
                if let (Some(field), Some(_)) = (error.get("field"), error.get("message")) {
                    if field.as_str() == Some("name") {
                        return Err(anyhow::anyhow!("Repository '{}' already exists", repo_name));
                    }
                }
            }
        }
        Err(anyhow::anyhow!(
            "Unprocessable Entity error. Error details: {}",
            serde_json::to_string_pretty(&response_json)?
        ))
    } else {
        Err(anyhow::anyhow!(
            "Error creating repository. Status code: {}. Error details: {}",
            status,
            serde_json::to_string_pretty(&response_json)?
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;

    #[tokio::test]
    async fn test_build_headers() {
        let token = "test_token";
        let client = GitHubClient::new(token.to_string());
        let headers = client.build_headers();

        assert_eq!(headers.get(ACCEPT).unwrap(), "application/vnd.github+json");
        assert_eq!(
            headers.get(AUTHORIZATION).unwrap(),
            &format!("Bearer {}", token)
        );
        assert_eq!(
            headers.get("x-github-api-version").unwrap(),
            GITHUB_API_VERSION
        );
        assert_eq!(headers.get(USER_AGENT).unwrap(), "shelf-github-api-client");
    }

    #[tokio::test]
    async fn test_build_repository_request() {
        let name = "test_repo";
        let request = build_repository_request(name);

        assert_eq!(request.name, name);
        assert_eq!(request.description, "This is your first repo!");
        assert_eq!(request.homepage, "https://github.com");
        assert_eq!(request.private, false);
    }

    #[tokio::test]
    async fn test_create_remote_success() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let repo_name = "test_repo";
        let mock_response = format!(
            r#"{{"html_url": "https://github.com/testuser/{}"}}"#,
            repo_name
        );
        let github_token = "TOKEN";
        let github_client = GitHubClient::new(github_token.to_string());

        let _m = server
            .mock("POST", "/user/repos")
            .match_header("accept", "application/vnd.github+json")
            .match_header("authorization", "Bearer TOKEN")
            .match_header("x-github-api-version", GITHUB_API_VERSION)
            .match_header("user-agent", "shelf-github-api-client")
            .match_body(Matcher::JsonString(
                serde_json::to_string(&build_repository_request(repo_name)).unwrap(),
            ))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(mock_response)
            .create_async()
            .await;

        let result = create_remote(repo_name).await;

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            format!("https://github.com/testuser/{}", repo_name)
        );
    }

    #[tokio::test]
    async fn test_create_remote_already_exists() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let repo_name = "existing_repo";
        let mock_response = r#"{
            "message": "Repository creation failed.",
            "errors": [
                {
                    "resource": "Repository",
                    "code": "already_exists",
                    "field": "name"
                }
            ]
        }"#;

        let github_token = "TOKEN";
        let github_client = GitHubClient::new(github_token.to_string());
        let _m = server
            .mock("POST", "/user/repos")
            .match_header("accept", "application/vnd.github+json")
            .match_header("authorization", "Bearer TOKEN")
            .match_header("x-github-api-version", GITHUB_API_VERSION)
            .match_header("user-agent", "shelf-github-api-client")
            .match_body(Matcher::JsonString(
                serde_json::to_string(&build_repository_request(repo_name)).unwrap(),
            ))
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(mock_response)
            .create_async()
            .await;

        let result = create_remote(repo_name).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            format!("Repository '{}' already exists", repo_name)
        );
    }

    #[tokio::test]
    async fn test_create_remote_other_error() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let repo_name = "error_repo";
        let mock_response = r#"{
            "message": "Server Error"
        }"#;
        let github_token = "TOKEN";
        let github_client = GitHubClient::new(github_token.to_string());

        let _m = server
            .mock("POST", "/user/repos")
            .match_header("accept", "application/vnd.github+json")
            .match_header("authorization", "Bearer TOKEN")
            .match_header("x-github-api-version", GITHUB_API_VERSION)
            .match_header("user-agent", "shelf-github-api-client")
            .match_body(Matcher::JsonString(
                serde_json::to_string(&build_repository_request(repo_name)).unwrap(),
            ))
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(mock_response)
            .create_async()
            .await;

        let result = create_remote(repo_name).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Server Error"));
    }
}
