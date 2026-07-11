//! Versioned, checksummed `SQLite` operator export bundles.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use pvlog_storage::DatabaseTarget;
use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, SqliteConnection};
use thiserror::Error;

const MANIFEST: &str = "manifest.json";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub format_version: u32,
    pub application_version: String,
    pub scope: String,
    pub source_backend: String,
    pub files: Vec<BundleFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleFile {
    pub relative_path: String,
    pub byte_length: u64,
    pub blake3: String,
}

/// Exports all `SQLite` database files into a versioned directory bundle.
///
/// # Errors
///
/// Returns an error for unsupported backends, existing output, or failed I/O.
pub async fn export_bundle(
    target: &DatabaseTarget,
    output: &Path,
) -> Result<BundleManifest, BundleError> {
    export_bundle_scope(target, output, None).await
}

/// Exports one opaque account database as a transferable bundle.
///
/// # Errors
///
/// Returns an error for unsafe account filenames, missing data, or failed I/O.
pub async fn export_account_bundle(
    target: &DatabaseTarget,
    output: &Path,
    account_database: &str,
) -> Result<BundleManifest, BundleError> {
    if Path::new(account_database)
        .file_name()
        .and_then(|name| name.to_str())
        != Some(account_database)
    {
        return Err(BundleError::UnsafePath(account_database.to_owned()));
    }
    export_bundle_scope(target, output, Some(account_database)).await
}

/// Packages a consistent `PostgreSQL` archive produced by the deployment backup hook.
///
/// # Errors
///
/// Returns an error when the target is not `PostgreSQL` or the archive cannot be read.
pub fn export_postgres_bundle(
    target: &DatabaseTarget,
    output: &Path,
    archive: &Path,
) -> Result<BundleManifest, BundleError> {
    if !matches!(target, DatabaseTarget::Postgres { .. }) {
        return Err(BundleError::UnsupportedBackend);
    }
    if output.exists() {
        return Err(BundleError::OutputExists(output.to_owned()));
    }
    fs::create_dir_all(output)?;
    let relative = Path::new("postgres.dump");
    let bytes = fs::read(archive)?;
    fs::write(output.join(relative), &bytes)?;
    let manifest = BundleManifest {
        format_version: 1,
        application_version: env!("CARGO_PKG_VERSION").to_owned(),
        scope: "instance".to_owned(),
        source_backend: "postgres".to_owned(),
        files: vec![BundleFile {
            relative_path: relative.to_string_lossy().into_owned(),
            byte_length: bytes.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
        }],
    };
    fs::write(output.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
    Ok(manifest)
}

async fn export_bundle_scope(
    target: &DatabaseTarget,
    output: &Path,
    account_database: Option<&str>,
) -> Result<BundleManifest, BundleError> {
    let DatabaseTarget::Sqlite {
        management_path,
        accounts_dir,
    } = target
    else {
        return Err(BundleError::UnsupportedBackend);
    };
    if output.exists() {
        return Err(BundleError::OutputExists(output.to_owned()));
    }
    fs::create_dir_all(output.join("accounts"))?;
    let mut sources = if account_database.is_none() {
        vec![(management_path.clone(), PathBuf::from("management.sqlite3"))]
    } else {
        Vec::new()
    };
    if let Some(account_database) = account_database {
        sources.push((
            accounts_dir.join(account_database),
            PathBuf::from("accounts").join(account_database),
        ));
    } else if accounts_dir.exists() {
        for entry in fs::read_dir(accounts_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                sources.push((
                    entry.path(),
                    PathBuf::from("accounts").join(entry.file_name()),
                ));
            }
        }
    }
    sources.sort_by(|left, right| left.1.cmp(&right.1));
    let mut files = Vec::with_capacity(sources.len());
    for (source, relative) in sources {
        files.push(snapshot_and_describe(&source, output, &relative).await?);
    }
    let manifest = BundleManifest {
        format_version: 1,
        application_version: env!("CARGO_PKG_VERSION").to_owned(),
        scope: account_database
            .map_or_else(|| "instance".to_owned(), |value| format!("account:{value}")),
        source_backend: "sqlite".to_owned(),
        files,
    };
    fs::write(output.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
    Ok(manifest)
}

/// Verifies the manifest, safe paths, lengths, and hashes in a bundle.
///
/// # Errors
///
/// Returns an error for malformed, unsupported, missing, or corrupt bundles.
pub fn verify_bundle(bundle: &Path) -> Result<BundleManifest, BundleError> {
    let manifest: BundleManifest = serde_json::from_slice(&fs::read(bundle.join(MANIFEST))?)?;
    if manifest.format_version != 1 {
        return Err(BundleError::UnsupportedFormat(manifest.format_version));
    }
    for file in &manifest.files {
        let relative = safe_relative(&file.relative_path)?;
        let bytes = fs::read(bundle.join(relative))?;
        if bytes.len() as u64 != file.byte_length
            || blake3::hash(&bytes).to_hex().as_str() != file.blake3
        {
            return Err(BundleError::Checksum(file.relative_path.clone()));
        }
    }
    Ok(manifest)
}

/// Validates and optionally restores a bundle into an empty `SQLite` target.
///
/// # Errors
///
/// Returns an error when verification fails or the destination is not empty.
pub fn import_bundle(
    target: &DatabaseTarget,
    bundle: &Path,
    dry_run: bool,
) -> Result<BundleManifest, BundleError> {
    let manifest = verify_bundle(bundle)?;
    if dry_run {
        return Ok(manifest);
    }
    let DatabaseTarget::Sqlite {
        management_path,
        accounts_dir,
    } = target
    else {
        return Err(BundleError::UnsupportedBackend);
    };
    let checkpoint = management_path.with_extension("import-state.json");
    let resuming = checkpoint.is_file();
    if (management_path.exists() || accounts_dir.exists()) && !resuming {
        return Err(BundleError::DestinationNotEmpty);
    }
    if let Some(parent) = management_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(accounts_dir)?;
    fs::write(&checkpoint, serde_json::to_vec(&manifest)?)?;
    for file in &manifest.files {
        let relative = safe_relative(&file.relative_path)?;
        let destination = if relative == Path::new("management.sqlite3") {
            management_path.clone()
        } else {
            accounts_dir.join(
                relative
                    .strip_prefix("accounts")
                    .map_err(|_| BundleError::UnsafePath(file.relative_path.clone()))?,
            )
        };
        if destination.exists() {
            let bytes = fs::read(&destination)?;
            if blake3::hash(&bytes).to_hex().as_str() != file.blake3 {
                return Err(BundleError::DestinationConflict(
                    destination.to_string_lossy().into_owned(),
                ));
            }
            continue;
        }
        fs::copy(bundle.join(relative), destination)?;
    }
    fs::remove_file(checkpoint)?;
    Ok(manifest)
}

async fn snapshot_and_describe(
    source: &Path,
    output: &Path,
    relative: &Path,
) -> Result<BundleFile, BundleError> {
    let destination = output.join(relative);
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(source)
        .read_only(true);
    let mut connection = SqliteConnection::connect_with(&options).await?;
    sqlx::query("VACUUM INTO ?")
        .bind(destination.to_string_lossy().as_ref())
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    let bytes = fs::read(destination)?;
    Ok(BundleFile {
        relative_path: relative.to_string_lossy().into_owned(),
        byte_length: bytes.len() as u64,
        blake3: blake3::hash(&bytes).to_hex().to_string(),
    })
}

fn safe_relative(value: &str) -> Result<&Path, BundleError> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(BundleError::UnsafePath(value.to_owned()));
    }
    Ok(path)
}

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("operator bundles currently require the SQLite profile")]
    UnsupportedBackend,
    #[error("unsupported bundle format version {0}")]
    UnsupportedFormat(u32),
    #[error("bundle output already exists: {0}")]
    OutputExists(PathBuf),
    #[error("import destination must not contain an existing database")]
    DestinationNotEmpty,
    #[error("existing import destination does not match the bundle: {0}")]
    DestinationConflict(String),
    #[error("unsafe bundle path: {0}")]
    UnsafePath(String),
    #[error("bundle checksum mismatch: {0}")]
    Checksum(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
