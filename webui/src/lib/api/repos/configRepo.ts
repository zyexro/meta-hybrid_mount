import { PATHS } from "../../constants";
import type { AppConfig } from "../../types";
import { runHybridMountJson } from "../core/bridge";
import { isRecord } from "../core/guards";
import { shellEscapeDoubleQuoted } from "../core/shell";
import { normalizeConfig } from "../codec/configCodec";

function extractConfig(payload: unknown): AppConfig {
  if (isRecord(payload) && isRecord(payload.config)) {
    return normalizeConfig(payload.config);
  }
  return normalizeConfig(payload);
}

export async function loadConfigFromFile(): Promise<AppConfig> {
  const payload = await runHybridMountJson("api config-get", PATHS.BINARY);
  return normalizeConfig(payload);
}

export async function saveConfigToFile(config: AppConfig): Promise<void> {
  const normalized = normalizeConfig(config);
  await patchConfigFile({
    moduledir: normalized.moduledir,
    mountsource: normalized.mountsource,
    partitions: [...normalized.partitions],
    overlay_mode: normalized.overlay_mode,
    disable_umount: normalized.disable_umount,
    enable_overlay_fallback: normalized.enable_overlay_fallback,
    default_mode: normalized.default_mode,
  });
}

export async function patchConfigFile(
  patch: Record<string, unknown>,
  options: { applyRuntime?: boolean } = {},
): Promise<AppConfig> {
  const encoded = shellEscapeDoubleQuoted(JSON.stringify(patch));
  const runtimeFlag = options.applyRuntime ? "--apply-runtime " : "";
  const payload = await runHybridMountJson(
    `api config-patch ${runtimeFlag}"${encoded}"`,
    PATHS.BINARY,
  );
  return extractConfig(payload);
}

export async function resetConfigFile(): Promise<AppConfig> {
  const payload = await runHybridMountJson("api config-reset", PATHS.BINARY);
  return extractConfig(payload);
}
