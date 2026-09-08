//! User-local releases. Preview is read-only; installation and launcher selection
//! are serialized. Project configuration is deliberately a separate transaction.
use crate::setup::{self, VERSION, digest, safe_path};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::{self, File, OpenOptions}, path::{Component, Path, PathBuf}};
use uuid::Uuid;

const FILES: [&str; 3] = ["softwarefactory", "bridges/notion_review.py", "bridges/native_check.py"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Manifest {
    schema_version: u32,
    version: String,
    sha256: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Release {
    pub version: String,
    pub path: PathBuf,
    pub active: bool,
    pub integrity: String,
}
#[derive(Debug, Serialize)]
pub struct Inventory {
    pub running_version: String,
    pub prefix: PathBuf,
    pub active_version: Option<String>,
    pub releases: Vec<Release>,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Plan {
    pub prefix: PathBuf,
    pub from_version: Option<String>,
    pub to_version: String,
    pub install: bool,
    pub activate: bool,
    pub integrity: String,
    fingerprints: BTreeMap<String, String>,
}

fn version(value: &str) -> Result<semver::Version> {
    let parsed = semver::Version::parse(value).context("Use an exact version, for example 0.2.0")?;
    ensure!(parsed.to_string() == value, "Version must be canonical");
    Ok(parsed)
}
fn prefix(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "Installation prefix must be absolute");
    ensure!(!path.components().any(|c| matches!(c, Component::ParentDir)), "Prefix cannot contain '..'");
    if path.exists() {
        ensure!(path.is_dir(), "Installation prefix must be a directory");
        return Ok(fs::canonicalize(path)?);
    }
    ensure!(fs::symlink_metadata(path).is_err(), "Installation prefix is a dangling symlink");
    Ok(prefix(path.parent().context("Invalid prefix")?)?.join(path.file_name().context("Invalid prefix")?))
}
fn release_path(prefix: &Path, name: &str) -> Result<PathBuf> {
    version(name)?;
    safe_path(prefix, &format!("releases/{name}"))
}
fn hashes(release: &Path) -> Result<BTreeMap<String, String>> {
    FILES.iter().map(|name| {
        let path = safe_path(release, name)?;
        ensure!(path.is_file(), "Incomplete release: {} is missing", path.display());
        if *name == "softwarefactory" {
            ensure!(setup::executable_available(&path.to_string_lossy()), "Release binary is not executable");
        }
        Ok((name.to_string(), digest(&fs::read(path)?)))
    }).collect()
}
fn inspect(prefix: &Path, name: &str) -> Result<(BTreeMap<String, String>, String)> {
    let release = release_path(prefix, name)?;
    let actual = hashes(&release)?;
    let manifest = safe_path(&release, "manifest.json")?;
    if manifest.exists() {
        let stored: Manifest = serde_json::from_slice(&fs::read(manifest)?)?;
        ensure!(stored.schema_version == 1 && stored.version == name && stored.sha256 == actual,
            "Release {name} failed integrity verification; restore it from a trusted build");
        Ok((actual, "verified_local_manifest".into()))
    } else {
        // 0.1.0 used the same immutable layout without a manifest.
        ensure!(name == "0.1.0", "Release {name} is missing its manifest");
        Ok((actual, "legacy_without_manifest".into()))
    }
}
fn active(prefix: &Path) -> Result<Option<String>> {
    let bin = safe_path(prefix, "bin")?;
    let launch = bin.join("softwarefactory");
    let metadata = match fs::symlink_metadata(&launch) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    ensure!(metadata.file_type().is_symlink(), "Existing launcher is not a managed symlink; move it aside explicitly before updating");
    let target = fs::read_link(&launch)?;
    let target = if target.is_absolute() { target } else { bin.join(target) };
    let name = target.parent().and_then(Path::file_name).and_then(|s| s.to_str()).context("Unrecognised launcher target")?;
    let expected = release_path(prefix, name)?.join("softwarefactory");
    // Canonicalize the chosen prefix above, but never accept an external launcher.
    ensure!(fs::canonicalize(&target).context("Launcher target is missing")? == expected,
        "Launcher points outside managed releases");
    Ok(Some(name.into()))
}
pub fn list(path: &Path) -> Result<Inventory> {
    let prefix = prefix(path)?;
    let active_version = active(&prefix)?;
    let dir = safe_path(&prefix, "releases")?;
    let mut releases = vec![];
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') { continue; }
            version(&name)?;
            let integrity = match inspect(&prefix, &name) {
                Ok((_, state)) => state,
                Err(error) => format!("invalid: {error:#}"),
            };
            releases.push(Release { active: active_version.as_deref() == Some(&name), path: release_path(&prefix, &name)?, version: name, integrity });
        }
    }
    releases.sort_by(|a,b| version(&b.version).unwrap().cmp(&version(&a.version).unwrap()));
    Ok(Inventory { running_version: VERSION.into(), prefix, active_version, releases })
}
fn current_hashes() -> Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        (FILES[0].into(), digest(&fs::read(std::env::current_exe()?)?)),
        (FILES[1].into(), digest(include_bytes!("../bridges/notion_review.py"))),
        (FILES[2].into(), digest(include_bytes!("../bridges/native_check.py"))),
    ]))
}
pub fn preview(path: &Path, to: Option<&str>, activate: bool) -> Result<Plan> {
    ensure!(cfg!(unix), "Versioned installation and switching currently require Unix (macOS or Linux)");
    let prefix = prefix(path)?;
    let from_version = active(&prefix)?;
    let to_version = to.unwrap_or(VERSION).to_string();
    let target = release_path(&prefix, &to_version)?;
    let install = !target.exists();
    let (fingerprints, integrity) = if install {
        ensure!(to.is_none(), "Release {to_version} is not installed; run install using that release's binary first");
        (current_hashes()?, "verified_local_manifest".into())
    } else {
        let (hashes, integrity) = inspect(&prefix, &to_version)?;
        if to.is_none() {
            ensure!(hashes == current_hashes()?, "Version {to_version} already contains a different build; releases are immutable, so use a new version");
        }
        (hashes, integrity)
    };
    Ok(Plan { prefix, from_version: from_version.clone(), to_version: to_version.clone(), install,
        activate: (activate || from_version.is_none()) && from_version.as_deref() != Some(&to_version), integrity, fingerprints })
}
struct InstallLock(File);
impl Drop for InstallLock {
    fn drop(&mut self) { let _ = FileExt::unlock(&self.0); }
}
pub fn apply(plan: &Plan) -> Result<()> {
    fs::create_dir_all(&plan.prefix)?;
    let lock_path = safe_path(&plan.prefix, ".install.lock")?;
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path)?;
    file.try_lock_exclusive().context("Another installation or update is in progress")?;
    let _guard = InstallLock(file);
    let fresh = preview(&plan.prefix, if plan.install { None } else { Some(&plan.to_version) }, plan.activate)?;
    ensure!(&fresh == plan, "Installation changed after preview; preview the update again");
    if plan.install { install(plan)?; }
    if plan.activate { activate(&plan.prefix, &plan.to_version)?; }
    Ok(())
}
fn install(plan: &Plan) -> Result<()> {
    let release = release_path(&plan.prefix, &plan.to_version)?;
    let parent = release.parent().unwrap();
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".staging-{}", Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let result = (|| -> Result<()> {
        fs::copy(std::env::current_exe()?, staging.join(FILES[0]))?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(staging.join(FILES[0]), fs::Permissions::from_mode(0o755))?;
        }
        fs::create_dir(staging.join("bridges"))?;
        fs::write(staging.join(FILES[1]), include_bytes!("../bridges/notion_review.py"))?;
        fs::write(staging.join(FILES[2]), include_bytes!("../bridges/native_check.py"))?;
        ensure!(hashes(&staging)? == plan.fingerprints, "Build changed after preview");
        let manifest = Manifest { schema_version: 1, version: plan.to_version.clone(), sha256: plan.fingerprints.clone() };
        fs::write(staging.join("manifest.json"), setup::json(&manifest)?)?;
        for name in FILES.iter().chain(std::iter::once(&"manifest.json")) { File::open(staging.join(name))?.sync_all()?; }
        File::open(staging.join("bridges"))?.sync_all()?;
        File::open(&staging)?.sync_all()?;
        fs::rename(&staging, &release)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_dir_all(staging); }
    result
}
fn activate(prefix: &Path, name: &str) -> Result<()> {
    inspect(prefix, name)?;
    active(prefix)?;
    let bin = safe_path(prefix, "bin")?;
    fs::create_dir_all(&bin)?;
    let temp = bin.join(format!(".launcher-{}", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        #[cfg(unix)] std::os::unix::fs::symlink(release_path(prefix, name)?.join("softwarefactory"), &temp)?;
        fs::rename(&temp, bin.join("softwarefactory"))?;
        File::open(&bin)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(temp); }
    result
}
