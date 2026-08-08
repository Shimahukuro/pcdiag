use reqwest::{blocking::Client, redirect::Policy};
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

const API_URL: &str = "https://api.github.com/repos/Shimahukuro/pcdiag/releases?per_page=100";
const RELEASES_URL: &str = "https://github.com/Shimahukuro/pcdiag/releases";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct AvailableUpdate {
    current: Version,
    latest: Version,
}

pub(crate) fn notify_if_available() {
    if let Some(update) = check_quietly(env!("CARGO_PKG_VERSION"), fetch_releases) {
        eprintln!(
            "pcdiag: 新しいバージョンを利用できます: {} → {}",
            update.current, update.latest
        );
        eprintln!("pcdiag: {RELEASES_URL}");
    }
}

fn fetch_releases() -> Result<Vec<Release>, ()> {
    let response = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::none())
        .user_agent(concat!("pcdiag/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| ())?
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(reqwest::blocking::Response::text)
        .map_err(|_| ())?;
    parse_releases(&response).ok_or(())
}

fn parse_releases(response: &str) -> Option<Vec<Release>> {
    serde_json::from_str(response).ok()
}

fn check_quietly<F, E>(current: &str, fetch: F) -> Option<AvailableUpdate>
where
    F: FnOnce() -> Result<Vec<Release>, E>,
{
    find_update(current, fetch().ok()?)
}

fn find_update(current: &str, releases: Vec<Release>) -> Option<AvailableUpdate> {
    let current = Version::parse(current).ok()?;
    let latest = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let tag = release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name);
            Version::parse(tag).ok()
        })
        .max()?;
    (latest > current).then_some(AvailableUpdate { current, latest })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag_name: &str, draft: bool) -> Release {
        Release {
            tag_name: tag_name.into(),
            draft,
        }
    }

    #[test]
    fn selects_newest_published_semantic_version() {
        let update = find_update(
            "0.3.0",
            vec![
                release("v0.4.0-alpha.1", false),
                release("not-a-version", false),
                release("v9.0.0", true),
                release("v0.4.0", false),
            ],
        )
        .expect("a newer release should be found");

        assert_eq!(update.current, Version::parse("0.3.0").unwrap());
        assert_eq!(update.latest, Version::parse("0.4.0").unwrap());
    }

    #[test]
    fn includes_github_prereleases_and_uses_semver_precedence() {
        let releases =
            parse_releases(r#"[{"tag_name":"v0.4.0-alpha.2","draft":false,"prerelease":true}]"#)
                .expect("GitHub prerelease response should parse");
        let update =
            find_update("0.4.0-alpha.1", releases).expect("a newer prerelease should be found");

        assert_eq!(update.latest, Version::parse("0.4.0-alpha.2").unwrap());
    }

    #[test]
    fn does_not_notify_for_same_or_older_versions() {
        assert_eq!(
            find_update(
                "0.3.0",
                vec![release("v0.2.0", false), release("v0.3.0", false)]
            ),
            None
        );
    }

    #[test]
    fn malformed_current_version_and_invalid_release_data_are_ignored() {
        assert_eq!(find_update("invalid", vec![release("v1.0.0", false)]), None);
        assert_eq!(find_update("0.3.0", vec![release("invalid", false)]), None);
        assert!(parse_releases("not-json").is_none());
        assert!(parse_releases(r#"[{"draft":false}]"#).is_none());
    }

    #[test]
    fn transport_and_api_failures_are_non_fatal() {
        for failure in ["offline", "timeout", "rate_limited", "http_error"] {
            let result = check_quietly("0.3.0", || Err::<Vec<Release>, _>(failure));
            assert_eq!(result, None);
        }
    }
}
