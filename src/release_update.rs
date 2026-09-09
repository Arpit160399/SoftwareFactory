//! Published GitHub release updates. Only HTTPS assets from the configured
//! repository are accepted, with hashes supplied by GitHub's release API.
use super::*;
use std::process::Command;

pub const REPOSITORY: &str = "Arpit160399/SoftwareFactory";
const MAX_METADATA: u64 = 2 * 1024 * 1024;
const MAX_BINARY: u64 = 128 * 1024 * 1024;
const MAX_BRIDGE: u64 = 2 * 1024 * 1024;

trait Transport {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>>;
}
struct Https;
impl Transport for Https {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("download");
        let response = Command::new("curl")
            // -q must be first: user curl configuration must not alter verification.
            .args([
                "-q",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--tlsv1.2",
                "--connect-timeout",
                "10",
                "--max-time",
                "60",
                "--max-redirs",
                "5",
                "--user-agent",
                concat!("softwarefactory/", env!("CARGO_PKG_VERSION")),
                "--header",
                "Accept: application/vnd.github+json",
                "--header",
                "X-GitHub-Api-Version: 2022-11-28",
                "--max-filesize",
            ])
            .arg(limit.to_string())
            .arg("--output")
            .arg(&path)
            .args(["--write-out", "%{http_code}", "--url", url])
            .output()
            .context("Updates require curl; install curl and retry")?;
        let status = String::from_utf8_lossy(&response.stdout);
        ensure!(
            status != "404",
            "No matching published release or asset is available in {REPOSITORY}"
        );
        ensure!(
            status != "403" && status != "429",
            "GitHub denied the request or its rate limit was reached; retry later"
        );
        ensure!(
            response.status.success() && status == "200",
            "Release download failed (HTTP {status}); check your connection and retry. The active version was not changed"
        );
        ensure!(
            fs::metadata(&path)?.len() <= limit,
            "Release response exceeds the download size limit"
        );
        Ok(fs::read(path)?)
    }
}
#[derive(Deserialize)]
struct PublishedRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
    state: String,
}
#[derive(Debug, Clone, Serialize)]
struct Download {
    path: String,
    url: String,
    size: u64,
    sha256: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Update {
    pub repository: String,
    pub prefix: PathBuf,
    pub running_version: String,
    pub from_version: String,
    pub to_version: String,
    pub update_available: bool,
    pub release_url: String,
    previous_active: Option<String>,
    files: Vec<Download>,
}
fn platform() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") if cfg!(target_env = "gnu") => Ok("x86_64-unknown-linux-gnu"),
        _ => anyhow::bail!(
            "No published update build supports this operating system and architecture; install a local build"
        ),
    }
}
pub fn check(path: &Path, to: Option<&str>) -> Result<Update> {
    check_with(path, to, &Https)
}
fn check_with(path: &Path, to: Option<&str>, transport: &impl Transport) -> Result<Update> {
    let target = platform()?;
    let prefix = prefix(path)?;
    let previous_active = active(&prefix)?;
    let from_version = previous_active.as_deref().unwrap_or(VERSION).to_string();
    let endpoint = match to {
        Some(to) => {
            version(to)?;
            format!("tags/v{to}")
        }
        None => "latest".into(),
    };
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/{endpoint}");
    let release: PublishedRelease = serde_json::from_slice(&transport.get(&url, MAX_METADATA)?)
        .context("Invalid GitHub release response")?;
    let to_version = release
        .tag_name
        .strip_prefix('v')
        .context("Release tag must start with v")?;
    let parsed = version(to_version)?;
    ensure!(
        !release.draft && !release.prerelease && parsed.pre.is_empty(),
        "Automatic updates require a published stable release"
    );
    if let Some(requested) = to {
        ensure!(
            requested == to_version,
            "Release version does not match the requested version"
        );
    }
    let update_available = if to.is_some() {
        to_version != from_version
    } else {
        parsed > version(&from_version)?
    };
    let mut files = vec![];
    if update_available {
        let binary = format!("softwarefactory-{target}");
        for (path, name, limit) in [
            (FILES[0], binary.as_str(), MAX_BINARY),
            (FILES[1], "notion_review.py", MAX_BRIDGE),
            (FILES[2], "native_check.py", MAX_BRIDGE),
        ] {
            let matching: Vec<_> = release.assets.iter().filter(|a| a.name == name).collect();
            ensure!(
                matching.len() == 1,
                "Release {} must contain exactly one {name} asset",
                release.tag_name
            );
            let asset = matching[0];
            let expected_url = format!(
                "https://github.com/{REPOSITORY}/releases/download/{}/{name}",
                release.tag_name
            );
            ensure!(
                asset.browser_download_url == expected_url && asset.state == "uploaded",
                "Release asset has an unexpected origin or is not completely uploaded"
            );
            ensure!(
                asset.size > 0 && asset.size <= limit,
                "Release asset {name} exceeds supported size limits"
            );
            let hash = asset
                .digest
                .as_deref()
                .and_then(|value| value.strip_prefix("sha256:"))
                .context(
                    "Release asset has no GitHub SHA-256 digest; ask the publisher to re-upload it",
                )?;
            ensure!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid release asset SHA-256"
            );
            files.push(Download {
                path: path.into(),
                url: expected_url,
                size: asset.size,
                sha256: hash.to_ascii_lowercase(),
            });
        }
    }
    Ok(Update {
        repository: REPOSITORY.into(),
        prefix,
        running_version: VERSION.into(),
        from_version,
        to_version: to_version.into(),
        update_available,
        release_url: format!(
            "https://github.com/{REPOSITORY}/releases/tag/{}",
            release.tag_name
        ),
        previous_active,
        files,
    })
}
pub fn apply(update: &Update) -> Result<()> {
    apply_with(update, &Https)
}
fn apply_with(update: &Update, transport: &impl Transport) -> Result<()> {
    if !update.update_available {
        return Ok(());
    }
    let mut files = BTreeMap::new();
    let mut expected = BTreeMap::new();
    // Fetch and validate all files before creating or changing the installation.
    for file in &update.files {
        let bytes = transport.get(&file.url, file.size)?;
        ensure!(
            bytes.len() as u64 == file.size && digest(&bytes) == file.sha256,
            "Checksum or size mismatch for {}; update cancelled and active version preserved",
            file.path
        );
        files.insert(file.path.clone(), bytes);
        expected.insert(file.path.clone(), file.sha256.clone());
    }
    ensure!(
        FILES.iter().all(|path| files.contains_key(*path)),
        "Incomplete release update"
    );
    ensure!(
        prefix(&update.prefix)? == update.prefix,
        "Installation prefix changed while downloading; retry"
    );
    fs::create_dir_all(&update.prefix)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(safe_path(&update.prefix, ".install.lock")?)?;
    file.try_lock_exclusive()
        .context("Another installation or update is in progress")?;
    let _guard = InstallLock(file);
    ensure!(
        active(&update.prefix)? == update.previous_active,
        "Active release changed while downloading; check for updates again"
    );
    let destination = release_path(&update.prefix, &update.to_version)?;
    if destination.exists() {
        ensure!(
            inspect(&update.prefix, &update.to_version)?.0 == expected,
            "Installed version contains a different build; immutable releases cannot be overwritten"
        );
    } else {
        install_files(&update.prefix, &update.to_version, &expected, &files)?;
    }
    activate(&update.prefix, &update.to_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::cell::RefCell;

    fn directory() -> tempfile::TempDir {
        tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap()
    }
    struct Fixture {
        release: Value,
        assets: BTreeMap<String, Vec<u8>>,
        requests: RefCell<Vec<String>>,
    }
    impl Fixture {
        fn new() -> Self {
            let names = [
                format!("softwarefactory-{}", platform().unwrap()),
                "notion_review.py".into(),
                "native_check.py".into(),
            ];
            let mut assets = BTreeMap::new();
            let metadata: Vec<_> = names.iter().map(|name| {
                let bytes = format!("fixture bytes for {name}").into_bytes();
                let url = format!("https://github.com/{REPOSITORY}/releases/download/v9.9.9/{name}");
                let item = json!({"name":name,"browser_download_url":url,"size":bytes.len(),"digest":format!("sha256:{}",digest(&bytes)),"state":"uploaded"});
                assets.insert(url, bytes);
                item
            }).collect();
            Self {
                release: json!({"tag_name":"v9.9.9","draft":false,"prerelease":false,"assets":metadata}),
                assets,
                requests: RefCell::new(vec![]),
            }
        }
    }
    impl Transport for Fixture {
        fn get(&self, url: &str, _: u64) -> Result<Vec<u8>> {
            self.requests.borrow_mut().push(url.into());
            if url.starts_with("https://api.github.com/") {
                return Ok(serde_json::to_vec(&self.release)?);
            }
            self.assets
                .get(url)
                .cloned()
                .context("Simulated download failure")
        }
    }
    #[test]
    fn check_only_fetches_metadata_and_writes_nothing() {
        let d = directory();
        let prefix = d.path().join("not-installed");
        let fixture = Fixture::new();
        let update = check_with(&prefix, None, &fixture).unwrap();
        assert!(update.update_available);
        assert_eq!(update.to_version, "9.9.9");
        assert!(!prefix.exists());
        assert_eq!(fixture.requests.borrow().len(), 1);
    }
    #[test]
    fn download_install_activate_and_recheck() {
        let d = directory();
        let fixture = Fixture::new();
        let update = check_with(d.path(), None, &fixture).unwrap();
        apply_with(&update, &fixture).unwrap();
        let inventory = list(d.path()).unwrap();
        assert_eq!(inventory.active_version.as_deref(), Some("9.9.9"));
        assert_eq!(inventory.releases[0].integrity, "verified_local_manifest");
        let next = check_with(d.path(), None, &fixture).unwrap();
        assert!(!next.update_available);
        assert!(next.files.is_empty());
        apply_with(&next, &fixture).unwrap();
    }
    #[test]
    fn bad_checksum_or_failed_download_keeps_previous_release() {
        for corruption in [false, true] {
            let d = directory();
            super::super::apply(&preview(d.path(), None, true).unwrap()).unwrap();
            let mut fixture = Fixture::new();
            let update = check_with(d.path(), None, &fixture).unwrap();
            let asset_url = update.files[2].url.clone();
            if corruption {
                fixture.assets.insert(asset_url, b"corrupted".to_vec());
            } else {
                fixture.assets.remove(&asset_url);
            }
            assert!(apply_with(&update, &fixture).is_err());
            assert_eq!(active(d.path()).unwrap().as_deref(), Some(VERSION));
            assert!(!d.path().join("releases/9.9.9").exists());
        }
    }
    #[test]
    fn latest_never_automatically_downgrades() {
        let d = directory();
        let fixture = Fixture::new();
        apply_with(&check_with(d.path(), None, &fixture).unwrap(), &fixture).unwrap();
        let mut older = Fixture::new();
        older.release["tag_name"] = json!("v0.1.0");
        older.release["assets"] = json!([]);
        let update = check_with(d.path(), None, &older).unwrap();
        assert!(!update.update_available);
        apply_with(&update, &older).unwrap();
        assert_eq!(active(d.path()).unwrap().as_deref(), Some("9.9.9"));
    }
    #[test]
    fn invalid_release_metadata_is_rejected_before_downloads() {
        let d = directory();
        for mode in 0..9 {
            let mut f = Fixture::new();
            match mode {
                0 => f.release["assets"][0]["digest"] = Value::Null,
                1 => {
                    f.release["assets"][0]["browser_download_url"] =
                        json!("https://untrusted.example/binary")
                }
                2 => f.release["assets"][0]["size"] = json!(MAX_BINARY + 1),
                3 => f.release["draft"] = json!(true),
                4 => f.release["prerelease"] = json!(true),
                5 => f.release["tag_name"] = json!("v../../escape"),
                6 => f.release["assets"][0]["name"] = json!("wrong-platform"),
                7 => f.release["assets"][0]["state"] = json!("new"),
                _ => {
                    let duplicate = f.release["assets"][0].clone();
                    f.release["assets"].as_array_mut().unwrap().push(duplicate);
                }
            }
            assert!(check_with(d.path(), None, &f).is_err(), "mode {mode}");
            assert_eq!(f.requests.borrow().len(), 1);
            assert!(!d.path().join("releases").exists());
        }
    }
    #[test]
    fn requested_version_must_match_response() {
        let d = directory();
        let f = Fixture::new();
        assert!(check_with(d.path(), Some("1.2.3"), &f).is_err());
        assert!(f.requests.borrow()[0].ends_with("/tags/v1.2.3"));
    }
    #[test]
    fn stale_activation_and_immutable_conflicts_are_rejected() {
        let d = directory();
        let f = Fixture::new();
        let update = check_with(d.path(), None, &f).unwrap();
        super::super::apply(&preview(d.path(), None, true).unwrap()).unwrap();
        assert!(apply_with(&update, &f).is_err());
        let update = check_with(d.path(), None, &f).unwrap();
        fs::create_dir_all(d.path().join("releases/9.9.9")).unwrap();
        assert!(apply_with(&update, &f).is_err());
        assert_eq!(active(d.path()).unwrap().as_deref(), Some(VERSION));
    }
    #[test]
    fn completed_staging_can_be_retried_and_rolled_back_without_project_changes() {
        let d = directory();
        super::super::apply(&preview(d.path(), None, true).unwrap()).unwrap();
        fs::write(d.path().join("project-data"), "preserve").unwrap();
        let f = Fixture::new();
        let update = check_with(d.path(), None, &f).unwrap();
        apply_with(&update, &f).unwrap();
        // Recreate the state where installation finished but activation did not.
        activate(d.path(), VERSION).unwrap();
        apply_with(&update, &f).unwrap();
        assert_eq!(active(d.path()).unwrap().as_deref(), Some("9.9.9"));
        super::super::apply(&preview(d.path(), Some(VERSION), true).unwrap()).unwrap();
        assert_eq!(
            fs::read_to_string(d.path().join("project-data")).unwrap(),
            "preserve"
        );
    }
}
