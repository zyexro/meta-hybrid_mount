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

use std::sync::{Mutex, MutexGuard};

/// 获取 Mutex 锁，如果被 poisoned 则恢复内部数据
///
/// Poisoned mutex 通常在持有锁的线程 panic 后发生，但其内部数据可能仍然有效。
/// 此函数允许恢复数据而不是传播 panic。
///
/// # 示例
///
/// ```no_run
/// use std::sync::Mutex;
/// use hybrid_mount::utils::lock_or_recover;
///
/// let mutex = Mutex::new(42);
/// let guard = lock_or_recover(&mutex);
/// assert_eq!(*guard, 42);
/// ```
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
