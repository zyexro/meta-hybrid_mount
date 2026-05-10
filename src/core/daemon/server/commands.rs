// Copyright (C) 2026 YuzakiKokuban <heibanbaize@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    fs,
    net::TcpStream,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    super::protocol::DaemonCommand,
    http::{self, WebuiHttpSession},
};
use crate::{
    conf::{config::Config, schema},
    core::{api, runtime_state::RuntimeState, user_hide_rules},
    defs,
    mount::kasumi as kasumi_mount,
    sys::{kasumi, lkm},
};

pub(super) fn load_runtime_config(config_path: &Path) -> Result<Config> {
    Config::load_optional_from_file(config_path)
        .with_context(|| format!("Failed to load config from path: {}", config_path.display()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_command(
    config: &Config,
    config_path: &Path,
    state: &Arc<Mutex<RuntimeState>>,
    shutdown: &Arc<AtomicBool>,
    webui: &WebuiHttpSession,
    sse_clients: &Arc<Mutex<Vec<TcpStream>>>,
    command: DaemonCommand,
) -> Result<Value> {
    match command {
        DaemonCommand::Ping => to_value(&json!({ "status": "ok" })),
        DaemonCommand::WebuiStart => Ok(webui.session_payload()),
        DaemonCommand::Init => {
            let mut guard = state.lock().expect("daemon state poisoned");
            let status_value = guard.status_value()?.clone();
            let config_value = to_value(config)?;
            let version_value = to_value(&api::build_version_payload())?;
            let kasumi_status_value = build_kasumi_runtime_json(config, &guard)?;
            let system_info_value = to_value(&api::build_system_info_payload(&guard))?;
            to_value(&json!({
                "status": status_value,
                "config": config_value,
                "version": version_value,
                "kasumi_status": kasumi_status_value,
                "system_info": system_info_value,
            }))
        }
        DaemonCommand::Shutdown => {
            shutdown.store(true, Ordering::Relaxed);
            to_value(&json!({ "shutdown": true }))
        }
        DaemonCommand::Status => {
            let mut guard = state.lock().expect("daemon state poisoned");
            Ok(guard.status_value()?.clone())
        }
        DaemonCommand::ApiStorage => {
            let guard = state.lock().expect("daemon state poisoned");
            to_value(&api::build_storage_payload(&guard))
        }
        DaemonCommand::ApiMountStats => {
            let guard = state.lock().expect("daemon state poisoned");
            to_value(&api::build_mount_stats_payload(&guard))
        }
        DaemonCommand::ApiMountTopology => {
            let guard = state.lock().expect("daemon state poisoned");
            to_value(&api::build_mount_topology_payload(config, &guard))
        }
        DaemonCommand::ApiPartitions => to_value(&api::build_partitions_payload(config)),
        DaemonCommand::ApiSystemInfo => {
            let guard = state.lock().expect("daemon state poisoned");
            to_value(&api::build_system_info_payload(&guard))
        }
        DaemonCommand::ApiVersion => to_value(&api::build_version_payload()),
        DaemonCommand::ApiConfigGet => to_value(config),
        DaemonCommand::ApiConfigSet { config: payload } => {
            let config: Config =
                serde_json::from_value(payload).context("Failed to decode config payload")?;
            config.save_to_file(config_path)?;
            refresh_and_to_value(
                &config,
                state,
                sse_clients,
                json!({ "saved": true, "config": &config }),
            )
        }
        DaemonCommand::ApiConfigPatch {
            patch,
            apply_runtime,
        } => {
            let config = patch_config_file(config_path, patch)?;
            let applied = if apply_runtime {
                let applied = kasumi_mount::apply_runtime_config(&config)?;
                kasumi::invalidate_status_cache();
                applied
            } else {
                false
            };
            refresh_and_to_value(
                &config,
                state,
                sse_clients,
                json!({
                    "saved": true,
                    "applied": applied,
                    "config": &config,
                }),
            )
        }
        DaemonCommand::ApiConfigReset => {
            let config = Config::default();
            config.save_to_file(config_path)?;
            kasumi_mount::apply_runtime_config(&config)?;
            kasumi::invalidate_status_cache();
            refresh_and_to_value(
                &config,
                state,
                sse_clients,
                json!({ "saved": true, "config": &config }),
            )
        }
        DaemonCommand::ApiModulesList { path } => {
            let snapshot = state.lock().expect("daemon state poisoned").clone();
            to_value(&api::build_modules_payload(
                config,
                &snapshot,
                path.as_deref(),
            )?)
        }
        DaemonCommand::ApiModulesApply { modules } => {
            let payload = api::apply_modules_payload(config_path, &modules)?;
            let config = load_runtime_config(config_path)?;
            refresh_and_to_value(&config, state, sse_clients, payload)
        }
        DaemonCommand::ApiLkm => to_value(&api::build_lkm_payload(config)),
        DaemonCommand::ApiHooks => {
            kasumi_mount::require_live(config, "read Kasumi hooks")?;
            to_value(&kasumi_mount::hook_lines()?)
        }
        DaemonCommand::ApiKernelUname => to_value(&read_kernel_uname_payload()?),
        DaemonCommand::ApiOpenUrl { url } => {
            open_url(&url)?;
            to_value(&json!({ "opened": true }))
        }
        DaemonCommand::ApiReboot => {
            reboot_device()?;
            to_value(&json!({ "reboot": true }))
        }
        DaemonCommand::ApiKasumiMapsAdd { rule } => {
            let updated = add_kasumi_maps_config_rule(config_path, rule)?;
            kasumi_mount::apply_runtime_config(&updated)?;
            kasumi::invalidate_status_cache();
            let count = updated.kasumi.maps_rules.len();
            refresh_and_to_value(
                &updated,
                state,
                sse_clients,
                json!({
                    "saved": true,
                    "config": &updated,
                    "count": count,
                }),
            )
        }
        DaemonCommand::ApiKasumiMapsClear => {
            let mut updated = load_runtime_config(config_path)?;
            updated.kasumi.maps_rules.clear();
            updated.save_to_file(config_path)?;
            kasumi_mount::apply_runtime_config(&updated)?;
            kasumi::invalidate_status_cache();
            refresh_and_to_value(
                &updated,
                state,
                sse_clients,
                json!({
                    "saved": true,
                    "config": &updated,
                    "count": 0,
                }),
            )
        }
        DaemonCommand::KasumiStatus => {
            let guard = state.lock().expect("daemon state poisoned");
            build_kasumi_runtime_json(config, &guard)
        }
        DaemonCommand::KasumiList => {
            let payload = if kasumi_mount::can_operate(config) {
                api::parse_kasumi_rule_listing(&kasumi::list_rules()?)
            } else {
                Vec::new()
            };
            to_value(&payload)
        }
        DaemonCommand::KasumiVersion => {
            let guard = state.lock().expect("daemon state poisoned");
            to_value(&api::build_kasumi_version_payload(config, &guard))
        }
        DaemonCommand::KasumiFeatures => to_value(&api::build_features_payload()),
        DaemonCommand::KasumiHooks => to_value(&kasumi_mount::hook_lines()?),
        DaemonCommand::KasumiApplyConfigRuntime => {
            let applied = kasumi_mount::apply_runtime_config(config)?;
            kasumi::invalidate_status_cache();
            refresh_and_to_value(config, state, sse_clients, json!({ "applied": applied }))
        }
        DaemonCommand::HideList => to_value(&user_hide_rules::load_user_hide_rules()?),
        DaemonCommand::HideAdd { path } => {
            let added = user_hide_rules::add_user_hide_rule(&path)?;
            if added && kasumi_mount::can_operate(config) {
                kasumi::hide_path(&path)?;
            }
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "added": added, "path": path }),
            )
        }
        DaemonCommand::HideRemove { path } => {
            let removed = user_hide_rules::remove_user_hide_rule(&path)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "removed": removed, "path": path }),
            )
        }
        DaemonCommand::HideApply => {
            kasumi_mount::require_live(config, "apply user hide rules")?;
            let (applied, failed) = user_hide_rules::apply_user_hide_rules()?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "applied": applied, "failed": failed }),
            )
        }
        DaemonCommand::LkmStatus => to_value(&api::build_lkm_payload(config)),
        DaemonCommand::LkmLoad => {
            lkm::load(&config.kasumi)?;
            kasumi::invalidate_status_cache();
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "message": "Kasumi LKM loaded." }),
            )
        }
        DaemonCommand::LkmUnload => {
            lkm::unload(&config.kasumi)?;
            kasumi::invalidate_status_cache();
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "message": "Kasumi LKM unloaded." }),
            )
        }
        DaemonCommand::KasumiClear => {
            kasumi::clear_rules()?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "message": "Kasumi rules cleared." }),
            )
        }
        DaemonCommand::KasumiReleaseConnection => {
            kasumi::release_connection();
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "message": "Released cached Kasumi client connection." }),
            )
        }
        DaemonCommand::KasumiInvalidateCache => {
            kasumi::invalidate_status_cache();
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "message": "Invalidated cached Kasumi status." }),
            )
        }
        DaemonCommand::KasumiFixMounts => {
            kasumi::fix_mounts()?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "message": "Kasumi mount ordering fixed." }),
            )
        }
        DaemonCommand::KasumiRestoreUnameGlobal => {
            kasumi::restore_uname_global()?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({ "message": "Kasumi global uname restored." }),
            )
        }
        DaemonCommand::KasumiSetUname {
            mode,
            release,
            version,
        } => {
            let mode = parse_uname_mode(&mode)?;
            apply_uname(mode, &release, &version)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi uname applied.",
                    "mode": display_uname_mode(mode),
                    "release": release,
                    "version": version,
                }),
            )
        }
        DaemonCommand::KasumiClearUname { mode } => {
            let mode = parse_uname_mode(&mode)?;
            match mode {
                schema::KasumiUnameMode::Scoped => {
                    apply_uname(schema::KasumiUnameMode::Scoped, "", "")?
                }
                schema::KasumiUnameMode::Global => kasumi::restore_uname_global()?,
            }
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi uname cleared.",
                    "mode": display_uname_mode(mode),
                }),
            )
        }
        DaemonCommand::KasumiRuleAdd {
            target,
            source,
            file_type,
        } => {
            let file_type = file_type.unwrap_or(detect_rule_file_type(&source)?);
            kasumi::add_rule(&target, &source, file_type)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi ADD rule applied.",
                    "target": target,
                    "source": source,
                    "file_type": file_type,
                }),
            )
        }
        DaemonCommand::KasumiRuleMerge { target, source } => {
            kasumi::add_merge_rule(&target, &source)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi MERGE rule applied.",
                    "target": target,
                    "source": source,
                }),
            )
        }
        DaemonCommand::KasumiRuleHide { path } => {
            kasumi::hide_path(&path)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi HIDE rule applied.",
                    "path": path,
                }),
            )
        }
        DaemonCommand::KasumiRuleDelete { path } => {
            kasumi::delete_rule(&path)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi rule deleted.",
                    "path": path,
                }),
            )
        }
        DaemonCommand::KasumiRuleAddDir {
            target_base,
            source_dir,
        } => {
            kasumi::add_rules_from_directory(&target_base, &source_dir)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi directory rules applied.",
                    "target_base": target_base,
                    "source_dir": source_dir,
                }),
            )
        }
        DaemonCommand::KasumiRuleRemoveDir {
            target_base,
            source_dir,
        } => {
            kasumi::remove_rules_from_directory(&target_base, &source_dir)?;
            refresh_and_to_value(
                config,
                state,
                sse_clients,
                json!({
                    "message": "Kasumi directory rules removed.",
                    "target_base": target_base,
                    "source_dir": source_dir,
                }),
            )
        }
        DaemonCommand::Batch { commands } => {
            let noop_clients = Arc::new(Mutex::new(Vec::new()));
            let mut results: Vec<Value> = Vec::with_capacity(commands.len());
            for cmd in commands {
                let result = match dispatch_command(
                    config,
                    config_path,
                    state,
                    shutdown,
                    webui,
                    &noop_clients,
                    cmd,
                ) {
                    Ok(value) => json!({ "ok": true, "data": value }),
                    Err(err) => json!({ "ok": false, "error": format!("{err}") }),
                };
                results.push(result);
            }
            refresh_runtime_snapshot(config, state, sse_clients)?;
            to_value(&json!({ "results": results }))
        }
    }
}

fn patch_config_file(config_path: &Path, patch: Value) -> Result<Config> {
    let config = load_runtime_config(config_path)?;
    let mut payload = serde_json::to_value(config).context("Failed to encode current config")?;
    merge_json(&mut payload, patch);

    let config: Config =
        serde_json::from_value(payload).context("Failed to decode patched config")?;
    config.save_to_file(config_path)?;
    Ok(config)
}

fn merge_json(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(&key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, patch) => {
            *target = patch;
        }
    }
}

fn read_kernel_uname_payload() -> Result<Value> {
    let release = fs::read_to_string("/proc/sys/kernel/osrelease")
        .context("failed to read /proc/sys/kernel/osrelease")?
        .trim()
        .to_string();
    let version = fs::read_to_string("/proc/sys/kernel/version")
        .context("failed to read /proc/sys/kernel/version")?
        .trim()
        .to_string();
    to_value(&json!({ "release": release, "version": version }))
}

fn open_url(url: &str) -> Result<()> {
    validate_url(url)?;
    let status = Command::new("am")
        .arg("start")
        .arg("-a")
        .arg("android.intent.action.VIEW")
        .arg("-d")
        .arg(url)
        .status()
        .context("Failed to start Android VIEW intent")?;
    if !status.success() {
        bail!("am start exited with status {status}");
    }
    Ok(())
}

fn validate_url(url: &str) -> Result<()> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("URL must start with http:// or https://");
    }
    if url.contains('\0') || url.contains('\n') || url.contains('\r') {
        bail!("URL contains invalid control characters");
    }
    // Reject URLs that could be misinterpreted as am(1) flags
    if url.contains(" --") {
        bail!("URL contains suspicious argument-like patterns");
    }
    Ok(())
}

fn reboot_device() -> Result<()> {
    let status = Command::new("reboot")
        .status()
        .context("Failed to execute reboot")?;
    if !status.success() {
        bail!("reboot exited with status {status}");
    }
    Ok(())
}

fn add_kasumi_maps_config_rule(config_path: &Path, rule: Value) -> Result<Config> {
    let mut config = load_runtime_config(config_path)?;
    let rule: crate::conf::schema::KasumiMapsRuleConfig =
        serde_json::from_value(rule).context("Failed to decode Kasumi maps rule")?;
    config
        .kasumi
        .maps_rules
        .retain(|item| item.target_ino != rule.target_ino || item.target_dev != rule.target_dev);
    config.kasumi.maps_rules.push(rule);
    config.save_to_file(config_path)?;
    Ok(config)
}

fn refresh_and_to_value<T: Serialize>(
    config: &Config,
    state: &Arc<Mutex<RuntimeState>>,
    sse_clients: &Arc<Mutex<Vec<TcpStream>>>,
    payload: T,
) -> Result<Value> {
    refresh_runtime_snapshot(config, state, sse_clients)?;
    to_value(&payload)
}

fn refresh_runtime_snapshot(
    config: &Config,
    state: &Arc<Mutex<RuntimeState>>,
    sse_clients: &Arc<Mutex<Vec<TcpStream>>>,
) -> Result<()> {
    let mut guard = state.lock().expect("daemon state poisoned");
    guard.kasumi = kasumi_mount::collect_runtime_info(config);
    guard.set_daemon_state(true, defs::SOCKET_FILE);
    guard
        .status_value()
        .map_err(|e| anyhow::anyhow!("Failed to cache status value: {e}"))?;
    guard.save()?;
    drop(guard);
    http::broadcast_sse_event(state, sse_clients, "state_update");
    Ok(())
}

fn parse_uname_mode(mode: &str) -> Result<schema::KasumiUnameMode> {
    match mode {
        "scoped" => Ok(schema::KasumiUnameMode::Scoped),
        "global" => Ok(schema::KasumiUnameMode::Global),
        _ => bail!("invalid uname mode: {mode} (expected scoped or global)"),
    }
}

fn apply_uname(mode: schema::KasumiUnameMode, release: &str, version: &str) -> Result<()> {
    let mut uname = kasumi::KasumiSpoofUname::default();
    if !release.is_empty() {
        uname.set_release(release)?;
    }
    if !version.is_empty() {
        uname.set_version(version)?;
    }

    match mode {
        schema::KasumiUnameMode::Scoped => kasumi::set_uname(&uname),
        schema::KasumiUnameMode::Global => kasumi::set_uname_global(&uname),
    }
}

fn display_uname_mode(mode: schema::KasumiUnameMode) -> &'static str {
    match mode {
        schema::KasumiUnameMode::Scoped => "scoped",
        schema::KasumiUnameMode::Global => "global",
    }
}

fn detect_rule_file_type(path: &Path) -> Result<i32> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to read source metadata for {}", path.display()))?;
    let file_type = metadata.file_type();

    if file_type.is_char_device() && metadata.rdev() == 0 {
        bail!(
            "source {} is a whiteout node; use `kasumi rule hide` instead",
            path.display()
        );
    }

    if file_type.is_file() {
        Ok(libc::DT_REG as i32)
    } else if file_type.is_symlink() {
        Ok(libc::DT_LNK as i32)
    } else {
        bail!(
            "unsupported source type for rule add: {} (use `merge` or `add-dir` for directories)",
            path.display()
        )
    }
}

fn build_kasumi_runtime_json(config: &Config, runtime_state: &RuntimeState) -> Result<Value> {
    let kasumi_info = kasumi_mount::collect_runtime_info(config);
    to_value(&json!({
        "status": kasumi_info.status,
        "available": kasumi_info.available,
        "protocol_version": kasumi_info.protocol_version,
        "feature_bits": kasumi_info.feature_bits,
        "feature_names": kasumi_info.feature_names,
        "hooks": kasumi_info.hooks,
        "rule_count": kasumi_info.rule_count,
        "user_hide_rule_count": kasumi_info.user_hide_rule_count,
        "mirror_path": kasumi_info.mirror_path,
        "lkm": api::build_lkm_payload(config),
        "config": config.kasumi.clone(),
        "runtime": {
            "snapshot": &runtime_state.kasumi,
            "kasumi_modules": &runtime_state.kasumi_modules,
            "active_mounts": &runtime_state.active_mounts,
        }
    }))
}

fn to_value<T: Serialize>(payload: &T) -> Result<Value> {
    serde_json::to_value(payload).context("Failed to encode daemon payload")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_accepts_http_and_https() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://localhost:8080/path?q=1").is_ok());
    }

    #[test]
    fn validate_url_rejects_non_http_schemes() {
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn validate_url_rejects_flag_like_patterns() {
        assert!(validate_url("https://example.com --es extra value").is_err());
    }

    #[test]
    fn validate_url_rejects_control_chars() {
        assert!(validate_url("https://example.com\n").is_err());
        assert!(validate_url("https://example.com\r\n").is_err());
        assert!(validate_url("https://ex\0ample.com").is_err());
    }
}
