use anyhow::{Context, Result, bail};
use pixivarchive_application::{
    auth::{AuthConfig, AuthService, PasswordSync},
    installation::InstallationData,
    settings::SettingsService,
};
use pixivarchive_db::Db;
use pixivarchive_domain::rule::rule_catalog;
use pixivarchive_web::openapi::ApiDoc;
use std::path::{Path, PathBuf};
use utoipa::OpenApi;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    if command == "export-openapi" {
        let output = args
            .next()
            .context("usage: pixivarchive-admin export-openapi <output-path>")?;
        if args.next().is_some() {
            bail!("usage: pixivarchive-admin export-openapi <output-path>");
        }
        export_openapi(Path::new(&output))?;
        return Ok(());
    }
    if command == "export-rule-catalog" {
        let output = args
            .next()
            .context("usage: pixivarchive-admin export-rule-catalog <output-path>")?;
        if args.next().is_some() {
            bail!("usage: pixivarchive-admin export-rule-catalog <output-path>");
        }
        export_rule_catalog(Path::new(&output))?;
        return Ok(());
    }
    if command != "prepare" || args.next().is_some() {
        bail!(
            "usage: pixivarchive-admin <prepare|export-openapi <output-path>|export-rule-catalog <output-path>>"
        );
    }

    prepare_installation().await
}

async fn prepare_installation() -> Result<()> {
    let database_url = required_env("DATABASE_URL")?;
    let administrator_password = required_env("PIXIVARCHIVE_ADMIN_PASSWORD")?;
    if administrator_password.is_empty() {
        bail!("PIXIVARCHIVE_ADMIN_PASSWORD cannot be empty");
    }
    let bootstrap_media_root = required_absolute_path_env("PIXIVARCHIVE_MEDIA_ROOT")?;
    let db = Db::connect(&database_url).await?;
    MIGRATOR.run(db.pool()).await?;

    let settings = SettingsService::new(db.clone()).effective().await?;
    let media_root = settings.storage.active_media_root(bootstrap_media_root);
    let installation = InstallationData::new(&media_root);
    let legacy_cookie_key = optional_legacy_cookie_key()?;
    match legacy_cookie_key {
        Some((key_id, encoded_key)) => installation.prepare_with_legacy(&key_id, &encoded_key),
        None => installation.prepare(),
    }
    .with_context(|| {
        format!(
            "failed to prepare installation data at {}",
            media_root.display()
        )
    })?;

    let password_sync = AuthService::new(db, AuthConfig::new(8)?)
        .synchronize_password(administrator_password)
        .await?;
    match password_sync {
        PasswordSync::Created => tracing::info!("administrator account created"),
        PasswordSync::Unchanged => tracing::info!("administrator password is current"),
        PasswordSync::Updated => tracing::info!("administrator password updated"),
    }
    tracing::info!(media_root = %media_root.display(), "PixivArchive installation prepared");
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} is required"))
}

fn required_absolute_path_env(name: &str) -> Result<PathBuf> {
    let value = required_env(name)?;
    if !value.starts_with('/') {
        bail!("{name} must be an absolute path");
    }
    Ok(PathBuf::from(value))
}

fn optional_legacy_cookie_key() -> Result<Option<(String, String)>> {
    let encoded_key = match std::env::var("PIXIVARCHIVE_PIXIV_COOKIE_KEY") {
        Ok(encoded_key) => encoded_key,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let key_id =
        std::env::var("PIXIVARCHIVE_PIXIV_COOKIE_KEY_ID").unwrap_or_else(|_| "primary".to_owned());
    Ok(Some((key_id, encoded_key)))
}

fn export_openapi(path: &Path) -> Result<()> {
    write_generated(path, &openapi_json()?)
}

fn export_rule_catalog(path: &Path) -> Result<()> {
    let catalog = serde_json::to_string_pretty(&rule_catalog())?;
    let source = format!(
        "import type {{ components }} from './schema';\n\nexport const ruleCatalog: components['schemas']['RuleCatalog'] = {catalog};\n"
    );
    write_generated(path, &source)
}

fn write_generated(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn openapi_json() -> Result<String> {
    let mut json = serde_json::to_string_pretty(&ApiDoc::openapi())?;
    json.push('\n');
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_export_is_version_31() {
        let document: serde_json::Value = serde_json::from_str(&openapi_json().unwrap()).unwrap();
        assert_eq!(document["openapi"], "3.1.0");
    }

    #[test]
    fn rule_catalog_export_is_deterministic_typescript() {
        let catalog = serde_json::to_string_pretty(&rule_catalog()).unwrap();
        assert!(!catalog.contains("\"page_sha256\""));
        assert!(catalog.contains("\"ranking_rank\""));
        assert!(catalog.contains("\"ranking_date\""));
        assert!(catalog.contains("\"value_example\""));
        assert!(catalog.contains("\"requires_value\": false"));
        assert!(catalog.contains("\"current_date_time\""));
    }

    #[test]
    fn embedded_migrations_append_revision_sources_to_the_initial_schema() {
        assert!(MIGRATOR.version_exists(1));
        assert!(MIGRATOR.version_exists(2));
        assert!(!MIGRATOR.version_exists(3));
    }
}
