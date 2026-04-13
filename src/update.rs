//! Self-update mechanism: downloads the latest GitHub release and atomically
//! replaces the current binary.
//!
//! Main stays synchronous — we use `reqwest::blocking` here so no tokio runtime
//! is needed even for `mvt update`.
//!
//! Behaviour is documented in `PLAN.md` (Implementation order step 11) and in
//! the `cli-creator` skill's "Self-Update Mechanism" section.
//!
//! Test injection env vars:
//! - `MVT_CURRENT_VERSION`       — override `CARGO_PKG_VERSION`
//! - `MVT_GITHUB_API_URL`        — override `https://api.github.com`
//! - `MVT_GITHUB_DOWNLOAD_URL`   — override `https://github.com`
//! - `MVT_SELF_PATH`             — override `std::env::current_exe()`
//! - `MVT_SKIP_CARGO_CHECK`      — bypass cargo-install detection

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

const OWNER: &str = "alfredvc";
const REPO: &str = "matvaretabellen-cli";
const DEFAULT_API_URL: &str = "https://api.github.com";
const DEFAULT_DOWNLOAD_URL: &str = "https://github.com";
const USER_AGENT: &str = "mvt-selfupdate";

pub struct UpdateArgs {
    /// If true, only report latest-vs-current without downloading.
    pub check_only: bool,
}

/// Synchronous entry point.
pub fn run(args: UpdateArgs) -> Result<()> {
    // 1. Cargo-install detection.
    cargo_install_guard()?;

    // 2. Current version.
    let current_version = current_version();
    let current_sv = semver::Version::parse(&current_version)
        .with_context(|| format!("invalid current version: {current_version}"))?;

    // 3. Fetch latest release.
    let client = build_client()?;
    let latest_tag = fetch_latest_tag(&client)?;
    let latest_version_str = strip_v_prefix(&latest_tag).to_string();
    let latest_sv = semver::Version::parse(&latest_version_str)
        .with_context(|| format!("invalid latest version: {latest_version_str}"))?;

    let needs_update = latest_sv > current_sv;

    // 4. Handle --check-only.
    if args.check_only {
        let out = CheckOutput {
            current_version: current_version.clone(),
            latest_version: latest_version_str,
            needs_update,
        };
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    // 5. Already latest.
    if !needs_update {
        let out = UpdateOutput {
            current_version: current_version.clone(),
            latest_version: latest_version_str,
            updated: false,
            installed_at: None,
        };
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    // 6. Download and install.
    let triple = platform_triple()?;
    let asset_name = format!("mvt-{triple}.tar.gz");

    let target_path = resolve_target_path()?;
    let target_dir = target_path
        .parent()
        .ok_or_else(|| anyhow!("target binary has no parent directory"))?
        .to_path_buf();

    let download_base =
        std::env::var("MVT_GITHUB_DOWNLOAD_URL").unwrap_or_else(|_| DEFAULT_DOWNLOAD_URL.into());
    let download_url = format!(
        "{}/{}/{}/releases/download/{}/{}",
        download_base.trim_end_matches('/'),
        OWNER,
        REPO,
        latest_tag,
        asset_name
    );

    let tarball_bytes = download_tarball(&client, &download_url)?;

    // Extract into a tempdir inside the same directory as the target, so a
    // subsequent rename is same-filesystem.
    let staging = tempfile::Builder::new()
        .prefix(".mvt-update-")
        .tempdir_in(&target_dir)
        .with_context(|| format!("failed to create staging dir in {}", target_dir.display()))?;

    let extracted = extract_mvt_binary(&tarball_bytes, staging.path())?;

    // Atomic install.
    install_binary(&extracted, &target_path)?;

    let out = UpdateOutput {
        current_version: current_version.clone(),
        latest_version: latest_version_str,
        updated: true,
        installed_at: Some(target_path.display().to_string()),
    };
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

// --- Helpers ------------------------------------------------------------

fn current_version() -> String {
    std::env::var("MVT_CURRENT_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn cargo_install_guard() -> Result<()> {
    if std::env::var_os("MVT_SKIP_CARGO_CHECK").is_some() {
        return Ok(());
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };
    // Normalise to string for a contains check — works on both Unix and Windows.
    let exe_str = exe.to_string_lossy();
    if exe_str.contains(".cargo/bin") || exe_str.contains(".cargo\\bin") {
        bail!(
            "mvt was installed via cargo install — run 'cargo install matvaretabellen-cli' to update"
        );
    }
    Ok(())
}

fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")
}

fn fetch_latest_tag(client: &reqwest::blocking::Client) -> Result<String> {
    let api_base = std::env::var("MVT_GITHUB_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.into());
    let url = format!(
        "{}/repos/{}/{}/releases/latest",
        api_base.trim_end_matches('/'),
        OWNER,
        REPO
    );

    let mut req = client.get(&url);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }

    let resp = req.send().with_context(|| format!("failed to GET {url}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        let body = resp.text().unwrap_or_default();
        bail!(
            "GitHub rate limit reached — set GITHUB_TOKEN env var (HTTP {status}): {body}",
            status = status,
            body = body
        );
    }
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        bail!("failed to fetch latest release (HTTP {status}): {body}");
    }

    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    let release: Release = resp
        .json()
        .context("failed to parse GitHub release response")?;
    Ok(release.tag_name)
}

fn download_tarball(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("failed to GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("failed to download release asset from {url} (HTTP {status})");
    }
    resp.bytes()
        .map(|b| b.to_vec())
        .context("failed to read release asset body")
}

fn extract_mvt_binary(tarball_bytes: &[u8], dest: &Path) -> Result<PathBuf> {
    let decoder = flate2::read::GzDecoder::new(tarball_bytes);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .context("failed to extract release tarball")?;

    // Find a file named "mvt" anywhere under `dest`.
    find_mvt(dest)?.ok_or_else(|| anyhow!("release tarball did not contain an 'mvt' binary"))
}

fn find_mvt(dir: &Path) -> Result<Option<PathBuf>> {
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("mvt") {
            return Ok(Some(path));
        }
        if file_type.is_dir() {
            if let Some(found) = find_mvt(&path)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

fn resolve_target_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("MVT_SELF_PATH") {
        return Ok(PathBuf::from(p));
    }
    std::env::current_exe().context("failed to resolve current_exe()")
}

fn install_binary(src: &Path, target: &Path) -> Result<()> {
    // Copy bytes to "<target>.new" in the target directory, then rename onto
    // the target. This is the cross-platform-safe atomic replace on the same
    // filesystem. (A plain rename from `src` works too; this avoids any
    // partial-write surprises if cross-device despite same-directory tempdir.)
    let target_dir = target
        .parent()
        .ok_or_else(|| anyhow!("target has no parent"))?;
    let staging_path = {
        let mut p = target_dir.to_path_buf();
        let name = target.file_name().and_then(|s| s.to_str()).unwrap_or("mvt");
        p.push(format!("{name}.new"));
        p
    };

    // Copy src to staging_path (same dir as target -> same filesystem).
    std::fs::copy(src, &staging_path)
        .with_context(|| format!("failed to stage new binary at {}", staging_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to chmod {}", staging_path.display()))?;
    }

    std::fs::rename(&staging_path, target).with_context(|| {
        format!(
            "failed to rename {} -> {}",
            staging_path.display(),
            target.display()
        )
    })?;

    Ok(())
}

fn strip_v_prefix(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

fn platform_triple() -> Result<&'static str> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    Ok(match (os, arch) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => bail!("unsupported platform: {os}/{arch}"),
    })
}

// --- Output types -------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckOutput {
    current_version: String,
    latest_version: String,
    needs_update: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateOutput {
    current_version: String,
    latest_version: String,
    updated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_at: Option<String>,
}

// Silence unused-import warnings when neither compression nor tarball paths
// are exercised via unit tests on a given platform.
#[allow(dead_code)]
fn _ensure_used(_r: &dyn Read) {}

// --- Tests --------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_v_prefix_works() {
        assert_eq!(strip_v_prefix("v1.2.3"), "1.2.3");
        assert_eq!(strip_v_prefix("1.2.3"), "1.2.3");
        assert_eq!(strip_v_prefix(""), "");
    }

    #[test]
    fn version_compare_needs_update() {
        let current = semver::Version::parse("0.1.0").unwrap();
        let latest = semver::Version::parse("0.2.0").unwrap();
        assert!(latest > current, "0.2.0 should be greater than 0.1.0");
    }

    #[test]
    fn version_compare_no_update_when_equal() {
        let current = semver::Version::parse("0.2.0").unwrap();
        let latest = semver::Version::parse("0.2.0").unwrap();
        assert!((latest <= current));
    }

    #[test]
    fn version_compare_no_update_when_current_higher() {
        let current = semver::Version::parse("0.3.0").unwrap();
        let latest = semver::Version::parse("0.2.0").unwrap();
        assert!((latest <= current));
    }

    #[test]
    fn platform_triple_returns_valid_host_triple() {
        // Should not panic on any platform we support as a test host.
        let triple = platform_triple().expect("test host is a supported platform");
        assert!(
            triple.contains("linux") || triple.contains("darwin"),
            "unexpected triple: {triple}"
        );
        assert!(
            triple.starts_with("x86_64-") || triple.starts_with("aarch64-"),
            "unexpected arch in triple: {triple}"
        );
    }

    #[test]
    fn cargo_install_guard_bypassed_by_env() {
        // Regardless of where the test binary lives, setting the skip env var
        // must not error. Use a scoped-set/restore pattern because tests share
        // the process env.
        //
        // SAFETY: tests run single-threaded for this env var by default; we
        // accept the minor race risk — this is best-effort coverage of the
        // bypass path.
        let prev = std::env::var("MVT_SKIP_CARGO_CHECK").ok();
        std::env::set_var("MVT_SKIP_CARGO_CHECK", "1");
        let result = cargo_install_guard();
        // Restore.
        match prev {
            Some(v) => std::env::set_var("MVT_SKIP_CARGO_CHECK", v),
            None => std::env::remove_var("MVT_SKIP_CARGO_CHECK"),
        }
        assert!(result.is_ok(), "guard should be bypassed: {result:?}");
    }

    #[test]
    fn current_version_env_override() {
        let prev = std::env::var("MVT_CURRENT_VERSION").ok();
        std::env::set_var("MVT_CURRENT_VERSION", "9.9.9");
        let v = current_version();
        match prev {
            Some(p) => std::env::set_var("MVT_CURRENT_VERSION", p),
            None => std::env::remove_var("MVT_CURRENT_VERSION"),
        }
        assert_eq!(v, "9.9.9");
    }
}
