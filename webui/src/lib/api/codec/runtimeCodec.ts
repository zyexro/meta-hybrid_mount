/*
 * Copyright (C) 2026 YuzakiKokuban <heibanbaize@gmail.com>
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

import type { AppConfig, KasumiStatus, StorageStatus } from "../../types";
import type { RuntimeStatePayload } from "../schemas";
import { ENABLE_KASUMI } from "../../constants_gen";

export function buildModeStats(
  state: RuntimeStatePayload,
): NonNullable<StorageStatus["modeStats"]> {
  const ms = state.mode_stats;
  return {
    overlay: ms?.overlayfs ?? 0,
    magic: ms?.magicmount ?? 0,
    kasumi: ENABLE_KASUMI ? (ms?.kasumi ?? 0) : 0,
    blacklisted: ms?.blacklisted ?? 0,
  };
}

export function buildMountedCount(
  state: RuntimeStatePayload,
  modeStats: NonNullable<StorageStatus["modeStats"]>,
): number {
  const overlay = state.overlay_modules?.length ?? 0;
  const magic = state.magic_modules?.length ?? 0;
  const kasumi = ENABLE_KASUMI ? (state.kasumi_modules?.length ?? 0) : 0;
  const total = overlay + magic + kasumi;
  return total > 0
    ? total
    : modeStats.overlay + modeStats.magic + modeStats.kasumi;
}

export function buildKasumiStatusFromPayload(
  payload: unknown,
  fallbackConfig: AppConfig["kasumi"],
  fallbackState: RuntimeStatePayload,
): KasumiStatus | null {
  if (!payload || typeof payload !== "object") return null;

  const p = payload as Record<string, unknown>;
  const lkmPayload = (
    p.lkm && typeof p.lkm === "object" ? p.lkm : {}
  ) as Record<string, unknown>;
  const runtimePayload = (
    p.runtime && typeof p.runtime === "object" ? p.runtime : {}
  ) as Record<string, unknown>;

  return {
    status:
      typeof p.status === "string"
        ? p.status
        : fallbackConfig.enabled
          ? "unavailable"
          : "disabled",
    available: typeof p.available === "boolean" ? p.available : false,
    kernel_supported:
      typeof p.kernel_supported === "boolean" ? p.kernel_supported : false,
    protocol_version:
      p.protocol_version === null || typeof p.protocol_version === "number"
        ? ((p.protocol_version as number | null | undefined) ?? null)
        : null,
    feature_bits:
      p.feature_bits === null || typeof p.feature_bits === "number"
        ? ((p.feature_bits as number | null | undefined) ?? null)
        : null,
    feature_names: Array.isArray(p.feature_names)
      ? p.feature_names.filter((f): f is string => typeof f === "string")
      : [],
    hooks: Array.isArray(p.hooks)
      ? p.hooks.filter((h): h is string => typeof h === "string")
      : [],
    rule_count:
      typeof p.rule_count === "number"
        ? Math.max(0, Math.trunc(p.rule_count))
        : 0,
    user_hide_rule_count:
      typeof p.user_hide_rule_count === "number"
        ? Math.max(0, Math.trunc(p.user_hide_rule_count))
        : 0,
    mirror_path:
      typeof p.mirror_path === "string"
        ? p.mirror_path
        : fallbackConfig.mirror_path,
    lkm: {
      loaded:
        typeof lkmPayload.loaded === "boolean" ? lkmPayload.loaded : false,
      module_name:
        typeof lkmPayload.module_name === "string"
          ? lkmPayload.module_name
          : undefined,
      autoload:
        typeof lkmPayload.autoload === "boolean"
          ? lkmPayload.autoload
          : fallbackConfig.lkm_autoload,
      kmi_override:
        typeof lkmPayload.kmi_override === "string"
          ? lkmPayload.kmi_override
          : fallbackConfig.lkm_kmi_override,
      current_kmi:
        typeof lkmPayload.current_kmi === "string"
          ? lkmPayload.current_kmi
          : "",
      search_dir:
        typeof lkmPayload.search_dir === "string"
          ? lkmPayload.search_dir
          : fallbackConfig.lkm_dir,
      module_file:
        typeof lkmPayload.module_file === "string"
          ? lkmPayload.module_file
          : undefined,
      last_error:
        lkmPayload.last_error === null ||
        typeof lkmPayload.last_error === "string"
          ? ((lkmPayload.last_error as string | null | undefined) ?? null)
          : null,
    },
    config: fallbackConfig,
    runtime: {
      snapshot: (runtimePayload.snapshot &&
      typeof runtimePayload.snapshot === "object"
        ? runtimePayload.snapshot
        : fallbackState.kasumi && typeof fallbackState.kasumi === "object"
          ? fallbackState.kasumi
          : {}) as Record<string, unknown>,
      kasumi_modules: Array.isArray(runtimePayload.kasumi_modules)
        ? runtimePayload.kasumi_modules.filter(
            (m): m is string => typeof m === "string",
          )
        : Array.isArray(fallbackState.kasumi_modules)
          ? fallbackState.kasumi_modules
          : [],
    },
  };
}
