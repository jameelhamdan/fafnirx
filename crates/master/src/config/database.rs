//! The configuration file and the database, in both directions: [`apply`] makes the database say what the file
//! says, and [`export`] reads out what the database says.

use super::{Config, DeviceEntry, MirrorEntry, Settings, SourceEntry};
use crate::integrations::api::{remove_source, SOURCE_PREFIX};
use crate::platform::db::{self, setting};
use crate::platform::store::Store;
use anyhow::{bail, Result};
use fafnirx_common::unix_now;
use postgres::{Client, GenericClient};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

const VERSION_COUNT: i64 = 5;
const RETENTION_DAYS: i64 = 30;
const OFFLINE_AFTER_SECS: i64 = 60;

/// Everything the file describes, as the database has it.
pub fn export(conn: &mut Client) -> Result<Config> {
    let settings = Settings {
        version_count: Some(setting(conn, "version_count", VERSION_COUNT)),
        retention_days: Some(setting(conn, "retention_days", RETENTION_DAYS)),
        offline_after_secs: Some(setting(conn, "offline_after_secs", OFFLINE_AFTER_SECS)),
    };

    let devices = conn
        .query(
            "SELECT name, kind, token_hash, revoked_at FROM devices WHERE kind IN ('device', 'local') ORDER BY name",
            &[],
        )?
        .iter()
        .map(|r| {
            let kind: String = r.get(1);
            let device = kind == "device";
            DeviceEntry {
                name: r.get(0),
                token_hash: device.then(|| r.get(2)),
                revoked: device && r.get::<_, Option<i64>>(3).is_some(),
                kind,
                token: None,
            }
        })
        .collect();

    let mirrors = conn
        .query(
            "SELECT name, remote, remote_type, remote_path, options, interval_secs, keep_runs, enabled FROM mirrors ORDER BY name",
            &[],
        )?
        .iter()
        .map(|r| MirrorEntry {
            name: r.get(0),
            remote: r.get(1),
            remote_type: r.get(2),
            remote_path: r.get(3),
            options: serde_json::from_str::<BTreeMap<String, Value>>(&r.get::<_, String>(4)).unwrap_or_default(),
            interval_hours: r.get::<_, i64>(5) as f64 / 3600.0,
            keep_runs: r.get(6),
            enabled: r.get::<_, i64>(7) != 0,
        })
        .collect();

    let sources = conn
        .query("SELECT name, kind, config, interval_secs, enabled FROM sources ORDER BY name", &[])?
        .iter()
        .map(|r| {
            let config: Value = serde_json::from_str(&r.get::<_, String>(2)).unwrap_or(Value::Null);
            SourceEntry {
                name: r.get(0),
                kind: r.get(1),
                folders: config["folders"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
                service_account: config["service_account"].clone(),
                interval_hours: r.get::<_, i64>(3) as f64 / 3600.0,
                enabled: r.get::<_, i64>(4) != 0,
            }
        })
        .collect();

    Ok(Config {
        version: super::VERSION,
        settings: Some(settings),
        devices: Some(devices),
        mirrors: Some(mirrors),
        sources: Some(sources),
    })
}

/// Makes the database say what `config` says: what the file lists is created or changed, what it no longer lists
/// is removed. The exceptions are devices, which are revoked and never deleted (their data stays), and the local
/// folders it does not list, which are folders on the disk and stay.
pub fn apply(conn: &mut Client, store: &Store, config: &Config) -> Result<()> {
    let mut tx = conn.transaction()?;
    if let Some(settings) = &config.settings {
        apply_settings(&mut tx, settings)?;
    }
    if let Some(devices) = &config.devices {
        apply_devices(&mut tx, store, devices)?;
    }
    if let Some(mirrors) = &config.mirrors {
        apply_mirrors(&mut tx, mirrors)?;
    }
    let gone = match &config.sources {
        Some(sources) => apply_sources(&mut tx, store, sources)?,
        None => Vec::new(),
    };
    db::notify(&mut tx, "devices", "config")?;
    db::notify(&mut tx, "jobs", "sources")?;
    db::notify(&mut tx, "jobs", "mirrors")?;
    tx.commit()?;
    // Marking a source's files deleted takes a transaction per file.
    for name in gone {
        remove_source(conn, store, &name)?;
    }
    Ok(())
}

fn apply_settings(tx: &mut impl GenericClient, settings: &Settings) -> Result<()> {
    for (key, value) in [
        ("version_count", settings.version_count),
        ("retention_days", settings.retention_days),
        ("offline_after_secs", settings.offline_after_secs),
    ] {
        if let Some(value) = value {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                &[&key, &value.to_string()],
            )?;
        }
    }
    Ok(())
}

fn apply_devices(tx: &mut impl GenericClient, store: &Store, devices: &[DeviceEntry]) -> Result<()> {
    struct Row {
        kind: String,
        token_hash: String,
        revoked: bool,
    }
    let existing: HashMap<String, Row> = tx
        .query("SELECT name, kind, token_hash, revoked_at FROM devices", &[])?
        .iter()
        .map(|r| {
            (r.get(0), Row { kind: r.get(1), token_hash: r.get(2), revoked: r.get::<_, Option<i64>>(3).is_some() })
        })
        .collect();
    let now = unix_now();
    for d in devices {
        let hash = d.token_hash.clone().unwrap_or_else(|| "!".into());
        let revoked_at = d.revoked.then_some(now);
        match existing.get(&d.name) {
            None => {
                tx.execute(
                    "INSERT INTO devices (name, kind, token_hash, created_at, revoked_at) VALUES ($1, $2, $3, $4, $5)",
                    &[&d.name, &d.kind, &hash, &now, &revoked_at],
                )?;
                // The folder exists from the start so that a network share shows it.
                let _ = std::fs::create_dir_all(store.live_dir(&d.name));
                tracing::info!("configuration: device {} added", d.name);
            }
            Some(row) if row.kind != d.kind => bail!("{} already exists as a {}, not a {}", d.name, row.kind, d.kind),
            Some(row) if d.kind == "device" => {
                if row.token_hash != hash {
                    tx.execute("UPDATE devices SET token_hash = $2 WHERE name = $1", &[&d.name, &hash])?;
                    tracing::info!("configuration: token of device {} changed", d.name);
                }
                if d.revoked && !row.revoked {
                    tx.execute("UPDATE devices SET revoked_at = $2 WHERE name = $1", &[&d.name, &now])?;
                    // A revoked device is no longer expected to report, so its offline alert would never clear.
                    db::resolve_alert(tx, "device_offline", &d.name)?;
                } else if !d.revoked && row.revoked {
                    tx.execute("UPDATE devices SET revoked_at = NULL WHERE name = $1", &[&d.name])?;
                }
            }
            Some(_) => {}
        }
    }
    // A device the file does not list can no longer send. Its data stays.
    let listed: HashSet<&str> = devices.iter().map(|d| d.name.as_str()).collect();
    for (name, row) in &existing {
        if row.kind == "device" && !row.revoked && !listed.contains(name.as_str()) {
            tx.execute("UPDATE devices SET revoked_at = $2 WHERE name = $1", &[name, &now])?;
            db::resolve_alert(tx, "device_offline", name)?;
            tracing::info!("configuration: device {name} is no longer listed and was revoked (its data is kept)");
        }
    }
    Ok(())
}

fn apply_mirrors(tx: &mut impl GenericClient, mirrors: &[MirrorEntry]) -> Result<()> {
    struct Row {
        id: i64,
        remote: String,
        remote_type: String,
        remote_path: String,
        options: Value,
    }
    let existing: HashMap<String, Row> = tx
        .query("SELECT name, id, remote, remote_type, remote_path, options FROM mirrors", &[])?
        .iter()
        .map(|r| {
            let options = serde_json::from_str(&r.get::<_, String>(5)).unwrap_or(Value::Null);
            (r.get(0), Row { id: r.get(1), remote: r.get(2), remote_type: r.get(3), remote_path: r.get(4), options })
        })
        .collect();
    for m in mirrors {
        let options = Value::Object(m.options.clone().into_iter().collect());
        let secs = (m.interval_hours * 3600.0).round() as i64;
        let enabled = i64::from(m.enabled);
        match existing.get(&m.name) {
            None => {
                tx.execute(
                    "INSERT INTO mirrors (name, remote, remote_type, options, remote_path, interval_secs, keep_runs, enabled, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                    &[&m.name, &m.remote, &m.remote_type, &options.to_string(), &m.remote_path, &secs, &m.keep_runs, &enabled, &unix_now()],
                )?;
                tracing::info!("configuration: mirror {} added", m.name);
            }
            Some(row) => {
                tx.execute(
                    "UPDATE mirrors SET remote = $2, remote_type = $3, options = $4, remote_path = $5, interval_secs = $6,
                            keep_runs = $7, enabled = $8 WHERE id = $1",
                    &[&row.id, &m.remote, &m.remote_type, &options.to_string(), &m.remote_path, &secs, &m.keep_runs, &enabled],
                )?;
                // Somewhere else is empty: everything has to be sent again.
                let moved = row.remote != m.remote
                    || row.remote_type != m.remote_type
                    || row.remote_path != m.remote_path
                    || row.options != options;
                if moved {
                    tx.execute("DELETE FROM mirror_items WHERE mirror_id = $1", &[&row.id])?;
                    tracing::info!(
                        "configuration: mirror {} points somewhere else, so it sends everything again",
                        m.name
                    );
                }
            }
        }
    }
    let listed: HashSet<&str> = mirrors.iter().map(|m| m.name.as_str()).collect();
    for (name, row) in &existing {
        if !listed.contains(name.as_str()) {
            tx.execute("DELETE FROM mirror_items WHERE mirror_id = $1", &[&row.id])?;
            tx.execute("DELETE FROM mirrors WHERE id = $1", &[&row.id])?;
            db::resolve_alert(tx, "mirror_failed", name)?;
            tracing::info!("configuration: mirror {name} is no longer listed and was removed");
        }
    }
    Ok(())
}

/// Returns the sources that are no longer listed, for the caller to remove once this transaction is done.
fn apply_sources(tx: &mut impl GenericClient, store: &Store, sources: &[SourceEntry]) -> Result<Vec<String>> {
    let existing: HashSet<String> = tx.query("SELECT name FROM sources", &[])?.iter().map(|r| r.get(0)).collect();
    for s in sources {
        let config = json!({ "service_account": s.service_account, "folders": s.folders }).to_string();
        let secs = (s.interval_hours * 3600.0).round() as i64;
        let enabled = i64::from(s.enabled);
        if existing.contains(&s.name) {
            tx.execute(
                "UPDATE sources SET config = $2, interval_secs = $3, enabled = $4 WHERE name = $1",
                &[&s.name, &config, &secs, &enabled],
            )?;
            continue;
        }
        // The source owns a device row, so its files use the same tables as a device's. '!' can never equal a
        // SHA-256 hex digest, so nothing can authenticate as a source.
        let device = format!("{SOURCE_PREFIX}{}", s.name);
        let device_id: i64 = match tx.query_opt(
            "INSERT INTO devices (name, kind, token_hash, created_at) VALUES ($1, 'source', '!', $2)
             ON CONFLICT (name) DO UPDATE SET name = devices.name WHERE devices.kind = 'source' RETURNING id",
            &[&device, &unix_now()],
        )? {
            Some(row) => row.get(0),
            None => bail!("a device named {device} already exists"),
        };
        tx.execute(
            "INSERT INTO sources (name, kind, config, interval_secs, enabled, device_id, created_at) VALUES ($1, 'gdrive', $2, $3, $4, $5, $6)",
            &[&s.name, &config, &secs, &enabled, &device_id, &unix_now()],
        )?;
        let _ = std::fs::create_dir_all(store.live_dir(&device));
        tracing::info!("configuration: source {} added", s.name);
    }
    let listed: HashSet<&str> = sources.iter().map(|s| s.name.as_str()).collect();
    Ok(existing.into_iter().filter(|n| !listed.contains(n.as_str())).collect())
}
