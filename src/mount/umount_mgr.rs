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
use std::{
    collections::HashSet,
    sync::{LazyLock, Mutex},
};

use anyhow::Result;
#[cfg(any(target_os = "linux", target_os = "android"))]
use ksu::{TryUmount, TryUmountFlags};
#[cfg(any(target_os = "linux", target_os = "android"))]
use rustix::path::Arg;

#[cfg(any(target_os = "linux", target_os = "android"))]
use crate::defs;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub static LIST: LazyLock<Mutex<TryUmount>> = LazyLock::new(|| Mutex::new(TryUmount::new()));
#[cfg(any(target_os = "linux", target_os = "android"))]
static HISTORY: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

#[cfg(any(target_os = "linux", target_os = "android"))]
fn is_ignored_partition(path: &str) -> bool {
    // Keep paths that app processes or PackageManager later dereference visible
    // in their namespaces. KSU detach is still used for less fragile paths.
    defs::should_skip_ksu_umount(path)
}

pub fn send_umountable<P>(target: P) -> Result<()>
where
    P: AsRef<Path>,
{
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = target;
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        if !crate::utils::KSU.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }

        let target = target.as_ref();
        let path = normalize_umount_path(target.as_str()?);

        if is_ignored_partition(&path) {
            crate::scoped_log!(
                debug,
                "umount",
                "skip: path={}, reason=ignore_unmount_partition",
                path
            );
            return Ok(());
        }

        let mut history = HISTORY
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to lock history mutex"))?;

        if !history.insert(path.clone()) {
            return Ok(());
        }

        LIST.lock()
            .map_err(|_| anyhow::anyhow!("Failed to lock umount list"))?
            .add(Path::new(&path));
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn normalize_umount_path(path: &str) -> String {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn commit() -> Result<()> {
    if !crate::utils::KSU.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    let mut list = LIST
        .lock()
        .map_err(|_| anyhow::anyhow!("Failed to lock umount list"))?;

    list.format_msg(|p| format!("{p:?} umount successful "));
    list.flags(TryUmountFlags::MNT_DETACH);
    if let Err(e2) = list.umount() {
        crate::scoped_log!(warn, "umount", "commit failed: {:#}", e2);
    } else {
        *list = TryUmount::new();
    }

    Ok(())
}

#[cfg(test)]
#[cfg(any(target_os = "linux", target_os = "android"))]
mod tests {
    use super::{is_ignored_partition, normalize_umount_path};

    #[test]
    fn normalizes_equivalent_umount_paths() {
        assert_eq!(normalize_umount_path("/system/bin/"), "/system/bin");
        assert_eq!(normalize_umount_path("/system/bin///"), "/system/bin");
        assert_eq!(normalize_umount_path("/"), "/");
        assert_eq!(normalize_umount_path("///"), "/");
    }

    #[test]
    fn skips_exact_ignored_partition() {
        assert!(is_ignored_partition("/system/lib"));
        assert!(is_ignored_partition("/system/lib64"));
        assert!(is_ignored_partition("/vendor/lib"));
        assert!(is_ignored_partition("/vendor/lib64"));
    }

    #[test]
    fn skips_descendants_of_ignored_partition() {
        assert!(is_ignored_partition("/system/lib64/foo"));
        assert!(is_ignored_partition("/vendor/lib/arm/libfoo.so"));
    }

    #[test]
    fn skips_package_manager_scan_paths() {
        assert!(is_ignored_partition("/system/app"));
        assert!(is_ignored_partition("/system/priv-app"));
        assert!(is_ignored_partition("/product/app"));
        assert!(is_ignored_partition("/product/priv-app/Example"));
        assert!(is_ignored_partition("/product/overlay"));
        assert!(is_ignored_partition(
            "/my_company/overlay/CustomOplusFwkResOverlay.apk"
        ));
        assert!(is_ignored_partition("/system_ext/app"));
        assert!(is_ignored_partition("/system_ext/etc/permissions"));
        assert!(is_ignored_partition("/system/etc/sysconfig"));
        assert!(is_ignored_partition("/system/etc/default-permissions"));
        assert!(is_ignored_partition("/system/etc/preferred-apps"));
    }

    #[test]
    fn does_not_skip_siblings_with_shared_prefix() {
        // /system/lib should not match /system/lib_extra
        assert!(!is_ignored_partition("/system/lib_extra"));
        assert!(!is_ignored_partition("/system/lib64_other"));
        assert!(!is_ignored_partition("/vendor/library"));
    }

    #[test]
    fn does_not_skip_unrelated_paths() {
        assert!(!is_ignored_partition("/product"));
        assert!(!is_ignored_partition("/system/etc"));
        assert!(!is_ignored_partition("/system/etc/init"));
        assert!(!is_ignored_partition(
            "/data/adb/hybrid-mount/run/staging_x"
        ));
    }
}
