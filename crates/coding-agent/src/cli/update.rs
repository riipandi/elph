//! The `elph update` command.
//!
//! Releases are built by `.github/workflows/release.yml`. Keep the asset
//! naming rules here in sync with that workflow and the install scripts.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::style::{CliStyle, S_ACCENT, S_MUTED, S_OK, S_WARN};
use crate::platform::scaffold::VersionFile;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, UpdateChannel};
use crate::utils::path::AppPaths;

const RELEASES_URL: &str = "https://api.github.com/repos/riipandi/elph/releases?per_page=100";
const USER_AGENT: &str = concat!("elph/", env!("CARGO_PKG_VERSION"));

#[derive(Args)]
#[command(next_help_heading = "Update options")]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Emit machine-readable JSON output (for --check)
    #[arg(long)]
    pub json: bool,

    /// Force re-download and install even if already up to date
    #[arg(long)]
    pub force_reinstall: bool,

    /// Install a specific version (e.g. 0.1.2 or 0.1.3-canary)
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,

    /// Switch to the canary release channel (faster updates, may have bugs)
    #[arg(long, conflicts_with = "stable")]
    pub canary: bool,

    /// Switch to the stable release channel (default, weekly releases)
    #[arg(long, conflicts_with = "canary")]
    pub stable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Stable,
    Canary,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Canary => "canary",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
struct ReleaseVersion {
    major: u64,
    minor: u64,
    patch: u64,
    channel: ChannelOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
enum ChannelOrder {
    Stable,
    Canary,
}

impl ReleaseVersion {
    fn channel(self) -> Channel {
        match self.channel {
            ChannelOrder::Stable => Channel::Stable,
            ChannelOrder::Canary => Channel::Canary,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    #[serde(rename = "prerelease")]
    _prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckReport {
    current_version: String,
    latest_version: String,
    channel: &'static str,
    update_available: bool,
}

#[derive(Debug)]
struct Release {
    tag: String,
    version: ReleaseVersion,
    archive_url: Option<String>,
    checksum_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateNotice {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) channel: &'static str,
}

pub fn handle(args: &UpdateArgs, paths: &Paths) -> ExitCode {
    if args.json && !args.check {
        super::help::cli_error("--json is only valid together with --check");
        return EXIT_ERROR;
    }

    match elph_agent::runtime::try_block_on(run(args, paths)) {
        Ok(Ok(())) => EXIT_SUCCESS,
        Ok(Err(error)) | Err(error) => {
            super::help::cli_error(format!("update failed: {error:#}"));
            EXIT_ERROR
        }
    }
}

async fn run(args: &UpdateArgs, paths: &Paths) -> Result<()> {
    let channel = requested_channel(args);
    let requested_tag = args.version.as_deref().map(normalize_tag).transpose()?;
    if let Some(tag) = &requested_tag {
        let requested_channel = parse_tag(tag)
            .map(ReleaseVersion::channel)
            .context("invalid release version")?;
        if (args.canary && requested_channel != Channel::Canary)
            || (args.stable && requested_channel != Channel::Stable)
        {
            bail!("--version and the selected release channel do not match");
        }
    }

    let client = create_update_client()?;
    let releases = fetch_releases(&client).await?;
    let release = select_release(&releases, requested_tag.as_deref(), channel)?;

    let version_file = read_version_file(paths);
    let current_tag = current_tag(version_file.as_ref());
    let update_available = current_tag != release.tag;
    if args.check {
        if let Err(error) = record_check(paths, version_file, &release) {
            log::warn!("could not update version.json: {error:#}");
        }
        return print_check(args, &release, &current_tag, update_available);
    }

    if !update_available && !args.force_reinstall {
        if let Err(error) = record_check(paths, version_file, &release) {
            log::warn!("could not update version.json: {error:#}");
        }
        print_human_check(&release, &current_tag, false);
        return Ok(());
    }

    print_human_update(&release, &current_tag, args.force_reinstall && !update_available);
    install_release(&client, &release).await?;
    if let Err(error) = record_install(paths, version_file, &release) {
        log::warn!("binary updated but could not update version.json: {error:#}");
    }
    print_human_updated(&release, &current_tag, args.force_reinstall && !update_available);
    Ok(())
}

fn create_update_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(30))
        .build()
        .context("create update client")
}

/// Check for a usable release without downloading or installing it.
///
/// This is used by the interactive TUI's background startup check. A release
/// without both required install assets is ignored so the user only sees
/// actionable update notices.
pub(crate) async fn check_for_update(paths: &Paths, channel: UpdateChannel) -> Result<Option<UpdateNotice>> {
    let client = create_update_client()?;
    let releases = fetch_releases(&client).await?;
    let release = select_release(
        &releases,
        None,
        match channel {
            UpdateChannel::Stable => Channel::Stable,
            UpdateChannel::Canary => Channel::Canary,
        },
    )?;
    if release.archive_url.is_none() || release.checksum_url.is_none() {
        return Ok(None);
    }

    let version_file = read_version_file(paths);
    let current = current_tag(version_file.as_ref());
    if !is_newer_release(&current, &release) {
        if let Err(error) = record_check(paths, version_file, &release) {
            log::debug!("could not record automatic update check: {error:#}");
        }
        return Ok(None);
    }

    if let Err(error) = record_check(paths, version_file, &release) {
        log::debug!("could not record automatic update check: {error:#}");
    }
    Ok(Some(UpdateNotice {
        current_version: current.trim_start_matches('v').to_owned(),
        latest_version: release.tag.trim_start_matches('v').to_owned(),
        channel: release.version.channel().as_str(),
    }))
}

fn is_newer_release(current_tag: &str, release: &Release) -> bool {
    parse_tag(current_tag).is_none_or(|current| release.version > current)
}

pub(crate) fn format_update_notice(notice: &UpdateNotice) -> String {
    format!(
        "Update available — {} {} → {} · Run `elph update`",
        notice.channel, notice.current_version, notice.latest_version
    )
}

fn requested_channel(args: &UpdateArgs) -> Channel {
    if args.canary {
        Channel::Canary
    } else {
        // Stable is the default, including when the command is run from a
        // canary binary. Users must opt back into canary explicitly.
        Channel::Stable
    }
}

async fn fetch_releases(client: &reqwest::Client) -> Result<Vec<GithubRelease>> {
    let response = client
        .get(RELEASES_URL)
        .send()
        .await
        .context("request GitHub releases")?
        .error_for_status()
        .context("GitHub releases request returned an error")?;
    response.json().await.context("decode GitHub releases response")
}

fn select_release(releases: &[GithubRelease], requested_tag: Option<&str>, channel: Channel) -> Result<Release> {
    let selected = if let Some(tag) = requested_tag {
        releases.iter().find(|release| release.tag_name == tag)
    } else {
        releases
            .iter()
            .filter(|release| !release.draft && release_channel(&release.tag_name) == Some(channel))
            .filter_map(|release| parse_tag(&release.tag_name).map(|version| (release, version)))
            .max_by_key(|(_, version)| *version)
            .map(|(release, _)| release)
    }
    .with_context(|| {
        requested_tag.map_or_else(
            || format!("no {channel:?} release found on GitHub"),
            |tag| format!("release {tag} was not found on GitHub"),
        )
    })?;
    if selected.draft {
        bail!("release {} is a draft and cannot be installed", selected.tag_name);
    }

    let version = parse_tag(&selected.tag_name).context("GitHub release has an invalid tag")?;
    let archive_name = release_asset_name()?;
    let archive_url = selected
        .assets
        .iter()
        .find(|asset| asset.name == archive_name)
        .map(|asset| asset.browser_download_url.clone());
    let checksum_url = selected
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS")
        .map(|asset| asset.browser_download_url.clone());

    Ok(Release {
        tag: selected.tag_name.clone(),
        version,
        archive_url,
        checksum_url,
    })
}

async fn install_release(client: &reqwest::Client, release: &Release) -> Result<()> {
    let target = std::env::current_exe().context("locate the running elph binary")?;
    let parent = target.parent().context("running binary has no parent directory")?;
    let archive_name = release_asset_name()?;
    let archive_url = release
        .archive_url
        .as_deref()
        .context("the selected release has no matching platform archive")?;
    let checksum_url = release
        .checksum_url
        .as_deref()
        .context("the selected release has no SHA256SUMS asset")?;

    let sty = CliStyle::auto_stderr();
    eprintln!("{}", sty.paint(S_MUTED, format!("  Downloading {archive_name}...")));
    let archive = download(client, archive_url, parent)
        .await
        .with_context(|| format!("download {archive_name}"))?;
    let checksums = download(client, checksum_url, parent)
        .await
        .context("download SHA256SUMS")?;
    let checksums = fs::read(checksums.path()).context("read SHA256SUMS")?;
    verify_checksum(archive.path(), &checksums, archive_name)?;

    let mut binary = extract_binary(archive.path(), archive_name, parent)?;
    install_binary(&target, binary.as_file_mut())?;

    Ok(())
}

async fn download(client: &reqwest::Client, url: &str, directory: &Path) -> Result<NamedTempFile> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request {url}"))?
        .error_for_status()
        .with_context(|| format!("download request returned an error for {url}"))?;
    let mut output = NamedTempFile::new_in(directory).context("create download file")?;
    let mut response = response;
    while let Some(chunk) = response.chunk().await.context("read download chunk")? {
        output.write_all(&chunk).context("write download")?;
    }
    output.as_file().sync_all().context("flush download")?;
    Ok(output)
}

fn verify_checksum(archive: &Path, checksums: &[u8], archive_name: &str) -> Result<()> {
    let checksums = std::str::from_utf8(checksums).context("SHA256SUMS is not UTF-8")?;
    let expected = checksums
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            let name = fields.next()?.trim_start_matches('*');
            (name == archive_name).then_some(hash)
        })
        .next()
        .context("SHA256SUMS has no entry for the selected archive")?;

    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("SHA256SUMS contains an invalid hash for {archive_name}");
    }
    let mut file = fs::File::open(archive).context("open archive for checksum")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).context("read archive for checksum")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex_encode(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("checksum mismatch for {archive_name}: expected {expected}, got {actual}");
    }
    Ok(())
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn extract_binary(archive: &Path, archive_name: &str, directory: &Path) -> Result<NamedTempFile> {
    if archive_name.ends_with(".zip") {
        let archive_file = fs::File::open(archive).context("open zip archive")?;
        let mut zip = zip::ZipArchive::new(archive_file).context("open zip archive")?;
        let mut file = zip
            .by_name(binary_name())
            .with_context(|| format!("{} is missing from {archive_name}", binary_name()))?;
        let mut binary = NamedTempFile::new_in(directory).context("create extracted binary")?;
        std::io::copy(&mut file, binary.as_file_mut()).context("read binary from zip archive")?;
        binary.as_file().sync_all().context("flush extracted binary")?;
        return Ok(binary);
    }

    let archive_file = fs::File::open(archive).context("open tar archive")?;
    let decoder = flate2::read::GzDecoder::new(archive_file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries().context("read tar archive entries")? {
        let mut entry = entry.context("read tar archive entry")?;
        let is_binary = entry.header().entry_type().is_file()
            && entry
                .path()
                .ok()
                .is_some_and(|path| path.file_name() == Some(std::ffi::OsStr::new(binary_name())));
        if is_binary {
            let mut binary = NamedTempFile::new_in(directory).context("create extracted binary")?;
            std::io::copy(&mut entry, binary.as_file_mut()).context("read binary from tar archive")?;
            binary.as_file().sync_all().context("flush extracted binary")?;
            return Ok(binary);
        }
    }
    bail!("{} is missing from {archive_name}", binary_name())
}

fn install_binary(target: &Path, binary: &mut (impl Read + Seek)) -> Result<()> {
    // The extracted file was just written to, so its cursor is at EOF.
    // Rewind it before copying; otherwise the staged replacement is empty.
    binary.seek(SeekFrom::Start(0)).context("rewind extracted binary")?;
    let parent = target.parent().context("running binary has no parent directory")?;
    let mut staged = NamedTempFile::new_in(parent).context("create staged binary")?;
    std::io::copy(binary, &mut staged).context("write staged binary")?;
    staged.as_file().sync_all().context("flush staged binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(target)
            .context("read permissions of running binary")?
            .permissions()
            .mode();
        fs::set_permissions(staged.path(), fs::Permissions::from_mode(mode)).context("preserve binary permissions")?;
        staged.as_file().sync_all().context("flush binary permissions")?;
        staged
            .persist(target)
            .map_err(|error| error.error)
            .context("replace running binary")?;
    }

    #[cfg(windows)]
    {
        let staged_path = staged.into_temp_path().keep().context("keep staged binary")?;
        schedule_windows_replace(&staged_path, target)?;
    }

    Ok(())
}

#[cfg(windows)]
fn schedule_windows_replace(staged: &Path, target: &Path) -> Result<()> {
    // Windows does not allow replacing an executable while it is running.
    // `cmd` waits for this process to exit, then moves the staged file into
    // place. Windows paths cannot contain quotes, so quoting protects spaces
    // and shell metacharacters here.
    let command = format!(
        "timeout /t 1 /nobreak >nul && move /Y \"{}\" \"{}\" >nul",
        staged.display(),
        target.display()
    );
    std::process::Command::new("cmd")
        .args(["/C", &command])
        .spawn()
        .context("schedule replacement of running binary")?;
    Ok(())
}

fn normalize_tag(version: &str) -> Result<String> {
    let tag = if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    };
    parse_tag(&tag).with_context(|| format!("invalid version {version:?}; expected X.Y.Z or X.Y.Z-canary"))?;
    Ok(tag)
}

fn parse_tag(tag: &str) -> Option<ReleaseVersion> {
    let raw = tag.strip_prefix('v')?;
    let (base, channel) = match raw.strip_suffix("-canary") {
        Some(base) => (base, ChannelOrder::Canary),
        None => (raw, ChannelOrder::Stable),
    };
    let mut numbers = base.split('.');
    let version = ReleaseVersion {
        major: numbers.next()?.parse().ok()?,
        minor: numbers.next()?.parse().ok()?,
        patch: numbers.next()?.parse().ok()?,
        channel,
    };
    (numbers.next().is_none()).then_some(version)
}

fn release_channel(tag: &str) -> Option<Channel> {
    parse_tag(tag).map(ReleaseVersion::channel)
}

fn read_version_file(paths: &Paths) -> Option<VersionFile> {
    let contents = fs::read_to_string(paths.version_path()).ok()?;
    serde_json::from_str(&contents).ok()
}

fn current_tag(version_file: Option<&VersionFile>) -> String {
    let identity = super::version::build_identity();
    let version = version_file.map_or(identity.version.as_str(), |file| file.version.as_str());
    let suffix = if identity.suffix == "-canary" { "-canary" } else { "" };
    format!("v{}{}", version.trim_start_matches('v'), suffix)
}

fn record_check(paths: &Paths, version_file: Option<VersionFile>, release: &Release) -> Result<()> {
    let mut file = version_file.unwrap_or_else(|| VersionFile::defaults(env!("CARGO_PKG_VERSION")));
    let version = release.tag.trim_start_matches('v').to_owned();
    match release.version.channel() {
        Channel::Stable => file.stable_version = Some(version),
        Channel::Canary => file.canary_version = Some(version),
    }
    file.last_checked_at = Some(chrono::Utc::now().to_rfc3339());
    elph_agent::fs::write_json_file(&paths.version_path(), &file)
}

fn record_install(paths: &Paths, version_file: Option<VersionFile>, release: &Release) -> Result<()> {
    let mut file = version_file.unwrap_or_else(|| VersionFile::defaults(env!("CARGO_PKG_VERSION")));
    let version = release.tag.trim_start_matches('v').to_owned();
    file.version = version.clone();
    match release.version.channel() {
        Channel::Stable => file.stable_version = Some(version),
        Channel::Canary => file.canary_version = Some(version),
    }
    file.last_checked_at = Some(chrono::Utc::now().to_rfc3339());
    elph_agent::fs::write_json_file(&paths.version_path(), &file)
}

fn print_check(args: &UpdateArgs, release: &Release, current: &str, update_available: bool) -> Result<()> {
    if args.json {
        let report = CheckReport {
            current_version: current.trim_start_matches('v').to_owned(),
            latest_version: release.tag.trim_start_matches('v').to_owned(),
            channel: release.version.channel().as_str(),
            update_available,
        };
        println!("{}", serde_json::to_string(&report).context("encode update report")?);
    } else {
        print_human_check(release, current, update_available);
    }
    Ok(())
}

fn print_human_check(release: &Release, current: &str, update_available: bool) {
    let sty = CliStyle::auto();
    let summary = human_check_summary(release, current, update_available);
    if update_available {
        println!("{}", sty.paint(S_WARN, summary));
        println!("  Run `elph update` to install it.");
    } else {
        println!("{}", sty.paint(S_OK, summary));
    }
}

fn print_human_update(release: &Release, current: &str, reinstall: bool) {
    let sty = CliStyle::auto();
    println!("{}", sty.paint(S_ACCENT, human_update_summary(release, current, reinstall)));
}

fn print_human_updated(release: &Release, current: &str, reinstall: bool) {
    let sty = CliStyle::auto();
    println!("{}", sty.paint(S_OK, human_updated_summary(release, current, reinstall)));
    #[cfg(windows)]
    println!("  Restart elph to use the new version.");
}

fn human_check_summary(release: &Release, current: &str, update_available: bool) -> String {
    let current = current.trim_start_matches('v');
    let latest = release.tag.trim_start_matches('v');
    let channel = release.version.channel().as_str();
    if update_available {
        format!("Update available — {channel} {current} → {latest}")
    } else {
        format!("Already up to date — {channel} {current}")
    }
}

fn human_update_summary(release: &Release, current: &str, reinstall: bool) -> String {
    let current = current.trim_start_matches('v');
    let latest = release.tag.trim_start_matches('v');
    let channel = release.version.channel().as_str();
    if reinstall {
        format!("Reinstalling — {channel} {current}")
    } else {
        format!("Updating — {channel} {current} → {latest}")
    }
}

fn human_updated_summary(release: &Release, current: &str, reinstall: bool) -> String {
    let current = current.trim_start_matches('v');
    let latest = release.tag.trim_start_matches('v');
    let channel = release.version.channel().as_str();
    if reinstall {
        format!("Reinstalled — {channel} {latest}")
    } else {
        format!("Updated — {channel} {current} → {latest}")
    }
}

#[cfg(target_os = "linux")]
fn release_asset_name() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("elph-linux-x86_64.tar.gz"),
        "aarch64" if is_raspberry_pi() => Ok("elph-linux-armv8.tar.gz"),
        "aarch64" => Ok("elph-linux-arm64.tar.gz"),
        other => bail!("unsupported Linux architecture: {other}"),
    }
}

#[cfg(target_os = "macos")]
fn release_asset_name() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("elph-macos-x86_64.tar.gz"),
        "aarch64" => Ok("elph-macos-aarch64.tar.gz"),
        other => bail!("unsupported macOS architecture: {other}"),
    }
}

#[cfg(target_os = "windows")]
fn release_asset_name() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("elph-windows-x86_64.zip"),
        other => bail!("unsupported Windows architecture: {other}"),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn release_asset_name() -> Result<&'static str> {
    bail!("self-update is not supported on this operating system")
}

fn binary_name() -> &'static str {
    if cfg!(windows) { "elph.exe" } else { "elph" }
}

#[cfg(target_os = "linux")]
fn is_raspberry_pi() -> bool {
    ["/proc/device-tree/model", "/sys/firmware/devicetree/base/model"]
        .iter()
        .filter_map(|path| fs::read(path).ok())
        .any(|model| String::from_utf8_lossy(&model).contains("Raspberry Pi"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stable_and_canary_tags() {
        assert_eq!(parse_tag("v1.2.3").map(ReleaseVersion::channel), Some(Channel::Stable));
        assert_eq!(parse_tag("v1.2.3-canary").map(ReleaseVersion::channel), Some(Channel::Canary));
        assert!(parse_tag("1.2.3").is_none());
        assert!(parse_tag("v1.2").is_none());
        assert!(parse_tag("v1.2.3-alpha").is_none());
    }

    #[test]
    fn normalizes_versions() {
        assert_eq!(normalize_tag("1.2.3").expect("valid tag"), "v1.2.3");
        assert_eq!(normalize_tag("v1.2.3-canary").expect("valid tag"), "v1.2.3-canary");
        assert!(normalize_tag("latest").is_err());
    }

    #[test]
    fn formats_compact_up_to_date_summary() {
        let release = Release {
            tag: "v0.1.3".into(),
            version: parse_tag("v0.1.3").expect("valid release"),
            archive_url: None,
            checksum_url: None,
        };
        assert_eq!(
            human_check_summary(&release, "v0.1.3", false),
            "Already up to date — stable 0.1.3"
        );
    }

    #[test]
    fn formats_available_update_summary() {
        let release = Release {
            tag: "v0.1.4".into(),
            version: parse_tag("v0.1.4").expect("valid release"),
            archive_url: None,
            checksum_url: None,
        };
        assert_eq!(
            human_check_summary(&release, "v0.1.3", true),
            "Update available — stable 0.1.3 → 0.1.4"
        );
    }

    #[test]
    fn formats_update_action_and_result_summaries() {
        let release = Release {
            tag: "v0.1.4".into(),
            version: parse_tag("v0.1.4").expect("valid release"),
            archive_url: None,
            checksum_url: None,
        };
        assert_eq!(
            human_update_summary(&release, "v0.1.3", false),
            "Updating — stable 0.1.3 → 0.1.4"
        );
        assert_eq!(
            human_updated_summary(&release, "v0.1.3", false),
            "Updated — stable 0.1.3 → 0.1.4"
        );
        assert_eq!(human_update_summary(&release, "v0.1.4", true), "Reinstalling — stable 0.1.4");
        assert_eq!(human_updated_summary(&release, "v0.1.4", true), "Reinstalled — stable 0.1.4");
    }

    #[test]
    fn startup_check_does_not_report_a_downgrade() {
        let release = Release {
            tag: "v0.2.2".into(),
            version: parse_tag("v0.2.2").expect("valid release"),
            archive_url: Some("archive".into()),
            checksum_url: Some("checksums".into()),
        };
        assert!(!is_newer_release("v0.3.1-canary", &release));
    }

    #[test]
    fn formats_startup_update_notice() {
        let notice = UpdateNotice {
            current_version: "0.2.2".into(),
            latest_version: "0.3.0".into(),
            channel: "stable",
        };
        assert_eq!(
            format_update_notice(&notice),
            "Update available — stable 0.2.2 → 0.3.0 · Run `elph update`"
        );
    }

    #[test]
    fn verifies_checksum_entries_and_hashes() {
        let mut archive = NamedTempFile::new().expect("archive file");
        archive.write_all(b"release").expect("write archive");
        let hash = hex_encode(&Sha256::digest(b"release"));
        let sums = format!("{hash}  elph-linux-x86_64.tar.gz\n");
        verify_checksum(archive.path(), sums.as_bytes(), "elph-linux-x86_64.tar.gz").expect("checksum");
        assert!(verify_checksum(archive.path(), b"bad  elph-linux-x86_64.tar.gz", "elph-linux-x86_64.tar.gz").is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn installs_the_entire_extracted_binary() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join(binary_name());
        fs::write(&target, b"old binary").expect("write existing binary");

        let mut binary = NamedTempFile::new_in(directory.path()).expect("extracted binary");
        binary.write_all(b"new binary").expect("write extracted binary");
        install_binary(&target, binary.as_file_mut()).expect("install binary");

        assert_eq!(fs::read(target).expect("read installed binary"), b"new binary");
    }

    #[test]
    fn selects_latest_release_for_channel() {
        let releases = vec![
            GithubRelease {
                tag_name: "v1.2.0".into(),
                draft: false,
                _prerelease: false,
                assets: vec![],
            },
            GithubRelease {
                tag_name: "v1.3.0-canary".into(),
                draft: false,
                _prerelease: true,
                assets: vec![],
            },
            GithubRelease {
                tag_name: "v1.1.0".into(),
                draft: false,
                _prerelease: false,
                assets: vec![],
            },
        ];
        assert_eq!(
            select_release(&releases, None, Channel::Stable)
                .expect("stable release")
                .tag,
            "v1.2.0"
        );
        assert_eq!(
            select_release(&releases, None, Channel::Canary)
                .expect("canary release")
                .tag,
            "v1.3.0-canary"
        );
    }
}
