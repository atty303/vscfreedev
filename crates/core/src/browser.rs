use crate::error::{BrowserError, Result};
use tracing::{debug, info, warn};
use url::Url;

/// Open a URL in the default browser
pub async fn open_url(url: &str) -> Result<()> {
    debug!("Attempting to open URL: {}", url);

    // Validate URL format
    validate_url(url)?;

    // For now, this is a mock implementation
    // In a real implementation, this would use platform-specific methods
    // to open the URL in the default browser
    info!("Mock: Opening URL in browser: {}", url);

    Ok(())
}

/// Validate URL format
fn validate_url(url: &str) -> Result<()> {
    match Url::parse(url) {
        Ok(parsed_url) => {
            debug!(
                "URL validation successful: scheme={}, host={:?}",
                parsed_url.scheme(),
                parsed_url.host()
            );

            // Check for supported schemes
            match parsed_url.scheme() {
                "http" | "https" | "file" | "ftp" => Ok(()),
                unsupported => {
                    warn!("Unsupported URL scheme: {}", unsupported);
                    Err(BrowserError::InvalidUrl {
                        url: url.to_string(),
                    }
                    .into())
                }
            }
        }
        Err(e) => {
            warn!("URL validation failed for '{}': {}", url, e);
            Err(BrowserError::InvalidUrl {
                url: url.to_string(),
            }
            .into())
        }
    }
}

/// Open multiple URLs in sequence
pub async fn open_urls(urls: &[&str]) -> Result<Vec<Result<()>>> {
    debug!("Opening {} URLs", urls.len());

    let mut results = Vec::new();
    for url in urls {
        results.push(open_url(url).await);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_valid_url() {
        let result = open_url("https://example.com").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let result = open_url("not-a-url").await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            crate::error::YuhaError::Browser(BrowserError::InvalidUrl { .. })
        ));
    }

    #[tokio::test]
    async fn test_unsupported_scheme() {
        let result = open_url("javascript:alert('test')").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://localhost:8080").is_ok());
        assert!(validate_url("file:///home/user/test.html").is_ok());

        assert!(validate_url("not-a-url").is_err());
        assert!(validate_url("javascript:alert('test')").is_err());
    }

    #[tokio::test]
    async fn test_open_multiple_urls() {
        let urls = vec![
            "https://example.com",
            "https://github.com",
            "http://localhost:8080",
        ];

        let results = open_urls(&urls).await.unwrap();
        assert_eq!(results.len(), 3);

        for result in results {
            assert!(result.is_ok());
        }
    }
}
