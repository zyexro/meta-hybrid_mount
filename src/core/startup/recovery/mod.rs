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

mod retry_state;
mod skip_markers;

use std::path::Path;

use anyhow::{Context, Result};
#[cfg(any(target_os = "linux", target_os = "android"))]
use rustix::mount::{UnmountFlags, unmount as umount};

use self::retry_state::{RecoveryDecision, RecoveryState};
use crate::{
    conf::config::Config,
    core::{MountController, recovery::ModuleStageFailure},
    sys, utils,
};

pub fn run(config: Config) -> Result<Config> {
    let mut state = RecoveryState::new(&config)?;
    let mut prev_mnt_base: Option<std::path::PathBuf> = None;

    loop {
        let attempt = state.current_attempt();
        let mnt_base = utils::get_mnt();
        sys::fs::ensure_dir_exists(&mnt_base)?;

        // Clean up stale overlays and loop mounts from the previous failed
        // attempt.  Without this, a retry generates a new random mount path
        // but the old overlay mounts still reference the previous (now
        // detached) path, causing "dangling lowerdir" overlays that break
        // idmap2 and module visibility.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if let Some(ref prev) = prev_mnt_base {
            cleanup_stale_attempt(prev, &mnt_base);
        }

        crate::scoped_log!(
            info,
            "recovery",
            "attempt start: attempt={}/{}, mount_base={}",
            attempt,
            state.max_restarts(),
            mnt_base.display()
        );

        let daemon_result = (|| -> Result<()> {
            MountController::new(config.clone(), &mnt_base)
                .init_storage(&mnt_base)
                .context("Failed to initialize storage")?
                .scan_and_prepare_plan()
                .context("Failed to scan modules and prepare mount plan")?
                .execute()
                .context("Failed to execute mount plan")?
                .finalize()
                .context("Failed to finalize boot sequence")?;
            Ok(())
        })();

        match daemon_result {
            Ok(()) => {
                state.log_completion();
                return Ok(config);
            }
            Err(e) => {
                // Save this mount base so the next retry can clean up stale
                // overlays and loop devices left by this attempt.
                prev_mnt_base = Some(mnt_base);
                if let Some(module_failure) = e.downcast_ref::<ModuleStageFailure>() {
                    if module_failure.module_ids.is_empty() {
                        match state.handle_unattributed_failure(module_failure.stage.to_string()) {
                            RecoveryDecision::RetryUnattributed => continue,
                            RecoveryDecision::AbortRetryLimit => {
                                state.abort_on_retry_limit()?;
                                unreachable!();
                            }
                            RecoveryDecision::InspectModules => {}
                        }
                    } else {
                        crate::scoped_log!(
                            warn,
                            "recovery",
                            "module failure: stage={}, modules={}",
                            module_failure.stage,
                            module_failure.module_ids.join(", ")
                        );
                    }

                    let action = state.mark_failed_modules(
                        &module_failure.stage.to_string(),
                        Some(&module_failure.source.to_string()),
                        &module_failure.module_ids,
                    )?;

                    if !action.already_marked.is_empty() {
                        crate::scoped_log!(
                            debug,
                            "recovery",
                            "already marked: modules={}",
                            action.already_marked.join(", ")
                        );
                    }
                    if !action.unknown_modules.is_empty() {
                        crate::scoped_log!(
                            error,
                            "recovery",
                            "unknown modules: stage={}, attempt={}/{}, modules={}",
                            module_failure.stage,
                            attempt,
                            state.max_restarts(),
                            action.unknown_modules.join(",")
                        );
                    }

                    if !action.newly_marked.is_empty() {
                        crate::scoped_log!(
                            warn,
                            "recovery",
                            "mark skip: stage={}, attempt={}/{}, modules={}",
                            module_failure.stage,
                            attempt,
                            state.max_restarts(),
                            action.newly_marked.join(",")
                        );

                        match state.handle_newly_marked_modules(module_failure.stage.to_string()) {
                            RecoveryDecision::RetryUnattributed => continue,
                            RecoveryDecision::AbortRetryLimit => {
                                state.abort_on_retry_limit()?;
                                unreachable!();
                            }
                            RecoveryDecision::InspectModules => continue,
                        }
                    }

                    crate::scoped_log!(
                        error,
                        "recovery",
                        "abort: stage={}, reason=no_newly_marked_modules",
                        module_failure.stage
                    );
                }

                let err_msg = format!("{:#}", e).replace('\n', " -> ");
                crate::scoped_log!(error, "recovery", "unrecoverable: error={}", err_msg);
                crate::core::module_status::update_crash_description(&err_msg);
                return Err(e);
            }
        }
    }
}

/// Clean up the ext4 loop mount and stale directory from a failed previous
/// recovery attempt.  Skipping this step leaves behind overlay mounts whose
/// `lowerdir` references the old (now-detached) path — the dangling lowerdir
/// breaks idmap2 and makes modules invisible to the package manager.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn cleanup_stale_attempt(prev_mnt_base: &Path, current_mnt_base: &Path) {
    if crate::sys::mount::is_mounted(prev_mnt_base) {
        crate::scoped_log!(
            warn,
            "recovery",
            "cleanup stale ext4 mount: path={}",
            prev_mnt_base.display()
        );
        if let Err(e) = umount(prev_mnt_base, UnmountFlags::DETACH) {
            crate::scoped_log!(
                warn,
                "recovery",
                "cleanup stale ext4 mount failed: path={}, error={:#}",
                prev_mnt_base.display(),
                e
            );
        }
    }

    if prev_mnt_base != current_mnt_base && prev_mnt_base.exists() {
        if let Err(e) = std::fs::remove_dir(prev_mnt_base) {
            crate::scoped_log!(
                debug,
                "recovery",
                "cleanup stale mount dir: path={}, error={:#}",
                prev_mnt_base.display(),
                e
            );
        }
    }
}
