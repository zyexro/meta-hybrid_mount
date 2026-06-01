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

use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "android"))]
#[allow(dead_code)]
pub const HYBRID_MOUNT_DIR: &str = "/data/adb/hybrid-mount";
pub const MODULES_DIR: &str = "/data/adb/modules";
#[allow(dead_code)]
pub const HYBRID_MOUNT_MODULE_DIR: &str = "/data/adb/modules/hybrid_mount";

pub const MODULES_IMG_FILE: &str = "/data/adb/hybrid-mount/modules.img";
#[allow(dead_code)]
pub const KASUMI_IMG_FILE: &str = "/data/adb/hybrid-mount/kasumi.img";
pub const RUN_DIR: &str = "/data/adb/hybrid-mount/run/";
pub const STATE_FILE: &str = "/data/adb/hybrid-mount/run/daemon_state.json";
pub const SOCKET_FILE: &str = "/data/adb/hybrid-mount/run/daemon.sock";
pub const PID_FILE: &str = "/data/adb/hybrid-mount/run/daemon.pid";
pub const SYSTEM_RW_DIR: &str = "/data/adb/hybrid-mount/rw";
pub const CONFIG_FILE: &str = "/data/adb/hybrid-mount/config.toml";
pub const MODULE_BLACKLIST_FILE: &str = "/data/adb/hybrid-mount/module_blacklist.toml";
pub const USER_HIDE_RULES_FILE: &str = "/data/adb/hybrid-mount/user_hide_rules.json";
pub const MODULE_PROP_FILE: &str = "/data/adb/modules/hybrid_mount/module.prop";
pub const KASUMI_MIRROR_DIR: &str = "/dev/kasumi_mirror";
pub const KASUMI_LKM_DIR: &str = "/data/adb/modules/hybrid_mount/kasumi_lkm";
pub const KASUMI_LKM_MODULE_NAME: &str = "kasumi_lkm";

pub const DISABLE_FILE_NAME: &str = "disable";
pub const REMOVE_FILE_NAME: &str = "remove";
pub const MOUNT_ERROR_FILE_NAME: &str = "mount_error";
pub const SKIP_MOUNT_FILE_NAME: &str = "skip_mount";
pub const REPLACE_DIR_FILE_NAME: &str = ".replace";
#[cfg(any(target_os = "linux", target_os = "android"))]
pub const REPLACE_DIR_XATTR: &str = "trusted.overlay.opaque";

pub const IGNORE_UMOUNT_PARTITIONS: &[&str] = &[
    "/vendor/lib",
    "/vendor/lib64",
    "/system/lib",
    "/system/lib64",
];

pub fn should_skip_ksu_umount(path: &str) -> bool {
    has_ignored_umount_prefix(path) || is_package_manager_scan_path(path)
}

pub fn should_skip_overlay_ksu_umount(path: &str, lowerdirs: &[impl AsRef<Path>]) -> bool {
    should_skip_ksu_umount(path) || overlay_contains_package_manager_etc(path, lowerdirs)
}

pub fn should_keep_existing_mount_before_overlay(path: &str) -> bool {
    is_package_manager_scan_path(path)
}

fn has_ignored_umount_prefix(path: &str) -> bool {
    IGNORE_UMOUNT_PARTITIONS.iter().any(|ignored| {
        let ignored = ignored.trim_end_matches('/');
        path == ignored
            || path
                .strip_prefix(ignored)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn is_package_manager_scan_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let mut parts = path.trim_start_matches('/').split('/');
    let Some(partition) = parts.next() else {
        return false;
    };

    if partition != "system" && !MANAGED_PARTITIONS.contains(&partition) {
        return false;
    }

    match (parts.next(), parts.next()) {
        // System package APK directories must remain visible after PackageManager
        // records their code paths; otherwise app processes can fail to load
        // classes from /system*/app or /system*/priv-app.
        (Some("app" | "priv-app"), _) => true,
        // Runtime resource overlays are scanned from partition overlay dirs and
        // can be opened again by OverlayManager/idmap2d or resource loading.
        (Some("overlay"), _) => true,
        // Priv-app allowlists and sysconfig entries are read alongside those
        // package directories during boot. Keep them in the same namespace view.
        (
            Some("etc"),
            Some("permissions" | "sysconfig" | "default-permissions" | "preferred-apps"),
        ) => true,
        _ => false,
    }
}

fn overlay_contains_package_manager_etc(path: &str, lowerdirs: &[impl AsRef<Path>]) -> bool {
    if !is_partition_etc_root(path) {
        return false;
    }

    lowerdirs.iter().any(|lowerdir| {
        PACKAGE_MANAGER_ETC_DIRS
            .iter()
            .any(|child| lowerdir.as_ref().join(child).exists())
    })
}

fn is_partition_etc_root(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let mut parts = path.trim_start_matches('/').split('/');
    let Some(partition) = parts.next() else {
        return false;
    };

    (partition == "system" || MANAGED_PARTITIONS.contains(&partition))
        && matches!((parts.next(), parts.next()), (Some("etc"), None))
}

const PACKAGE_MANAGER_ETC_DIRS: &[&str] = &[
    "permissions",
    "sysconfig",
    "default-permissions",
    "preferred-apps",
];

pub const MANAGED_PARTITIONS: &[&str] = &[
    "odm",
    "product",
    "system_ext",
    "vendor",
    "apex",
    "mi_ext",
    "my_bigball",
    "my_carrier",
    "my_company",
    "my_engineering",
    "my_heytap",
    "my_manifest",
    "my_preload",
    "my_product",
    "my_region",
    "my_reserve",
    "my_stock",
    "oem",
    "optics",
    "prism",
];

pub const MAX_MERGE_JSON_DEPTH: usize = 64;

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn overlay_etc_root_skips_ksu_umount_when_pm_config_is_present() {
        let temp = TempDir::new().unwrap();
        let lowerdir = temp.path().join("Slide/system/etc");
        fs::create_dir_all(lowerdir.join("permissions")).unwrap();

        assert!(should_skip_overlay_ksu_umount(
            "/system/etc",
            &[lowerdir.as_path()]
        ));
    }

    #[test]
    fn overlay_etc_root_does_not_skip_ksu_umount_for_unrelated_children() {
        let temp = TempDir::new().unwrap();
        let lowerdir = temp.path().join("module/system/etc");
        fs::create_dir_all(lowerdir.join("init")).unwrap();

        assert!(!should_skip_overlay_ksu_umount(
            "/system/etc",
            &[lowerdir.as_path()]
        ));
    }

    #[test]
    fn overlay_non_etc_scan_path_skips_ksu_umount_without_content_probe() {
        let temp = TempDir::new().unwrap();
        let lowerdir = temp.path().join("Slide/system/priv-app");

        assert!(should_skip_overlay_ksu_umount(
            "/system/priv-app",
            &[lowerdir.as_path()]
        ));
        assert!(should_skip_overlay_ksu_umount(
            "/product/overlay",
            &[lowerdir.as_path()]
        ));
        assert!(should_skip_overlay_ksu_umount(
            "/my_company/overlay/CustomOplusFwkResOverlay.apk",
            &[lowerdir.as_path()]
        ));
    }

    #[test]
    fn package_manager_scan_paths_keep_existing_mount_before_overlay() {
        assert!(should_keep_existing_mount_before_overlay("/system/app"));
        assert!(should_keep_existing_mount_before_overlay(
            "/system/priv-app"
        ));
        assert!(should_keep_existing_mount_before_overlay(
            "/product/overlay"
        ));
        assert!(!should_keep_existing_mount_before_overlay("/system/bin"));
    }
}
