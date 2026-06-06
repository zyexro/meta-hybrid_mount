import { PATHS } from "../../constants";
import type { Module, ModuleRules } from "../../types";
import { runDaemonCommand } from "../core/bridge";
import { isBoolean, isRecord, isString } from "../core/guards";
import { normalizeMountMode, normalizeStringMap } from "../codec/configCodec";

interface ModuleRuntimeEntry {
  id: string;
  name?: string;
  version?: string;
  author?: string;
  description?: string;
  mode: Module["mode"];
  is_mounted: boolean;
  enabled: boolean;
  source_path?: string;
  rules: ModuleRules;
  mount_error?: string;
  suggest_ignore?: boolean;
}

interface ModuleMetadata {
  name: string;
  version: string;
  author: string;
  description: string;
}

function defaultModuleMetadata(moduleId: string): ModuleMetadata {
  return {
    name: moduleId,
    version: "unknown",
    author: "unknown",
    description: "No description",
  };
}

function normalizeModuleRuntimeEntry(value: unknown): ModuleRuntimeEntry {
  const payload = isRecord(value) ? value : {};
  const rulesPayload = isRecord(payload.rules) ? payload.rules : {};
  return {
    id: isString(payload.id) ? payload.id : "",
    name: normalizeMetadataField(payload.name),
    version: normalizeMetadataField(payload.version),
    author: normalizeMetadataField(payload.author),
    description: normalizeMetadataField(payload.description),
    mode: normalizeMountMode(payload.mode),
    is_mounted: isBoolean(payload.is_mounted) ? payload.is_mounted : false,
    enabled: isBoolean(payload.enabled) ? payload.enabled : true,
    source_path: isString(payload.source_path)
      ? payload.source_path
      : undefined,
    rules: {
      default_mode: normalizeMountMode(rulesPayload.default_mode),
      paths: normalizeStringMap(rulesPayload.paths),
    },
    mount_error:
      isString(payload.mount_error) && payload.mount_error.trim()
        ? payload.mount_error
        : undefined,
    suggest_ignore: isBoolean(payload.suggest_ignore)
      ? payload.suggest_ignore
      : undefined,
  };
}

function normalizeMetadataField(value: unknown): string | undefined {
  return isString(value) && value.trim() ? value : undefined;
}

function normalizeModuleMetadata(entry: ModuleRuntimeEntry): ModuleMetadata {
  const defaults = defaultModuleMetadata(entry.id);
  return {
    name: entry.name ?? defaults.name,
    version: entry.version ?? defaults.version,
    author: entry.author ?? defaults.author,
    description: entry.description ?? defaults.description,
  };
}

function toModule(entry: ModuleRuntimeEntry, metadata: ModuleMetadata): Module {
  return {
    id: entry.id,
    name: metadata.name,
    version: metadata.version,
    author: metadata.author,
    description: metadata.description,
    mode: entry.mode,
    is_mounted: entry.is_mounted,
    enabled: entry.enabled,
    source_path: entry.source_path,
    rules: entry.rules,
    mount_error: entry.mount_error,
    suggest_ignore: entry.suggest_ignore,
  };
}

async function applyModulesPayload(modules: Module[]): Promise<void> {
  const payload = modules.map((module) => ({
    id: module.id,
    enabled: module.enabled ?? true,
    source_path: module.source_path,
    rules: {
      default_mode: normalizeMountMode(module.rules.default_mode),
      paths: normalizeStringMap(module.rules.paths),
    },
  }));
  await runDaemonCommand(
    { type: "api-modules-apply", modules: payload },
    PATHS.BINARY,
  );
}

export async function scanModules(path?: string): Promise<Module[]> {
  const payload = await runDaemonCommand(
    {
      type: "api-modules-list",
      path: path?.trim() || null,
    },
    PATHS.BINARY,
  );
  if (!Array.isArray(payload)) {
    throw new Error("modules payload is invalid");
  }

  const entries = payload.map(normalizeModuleRuntimeEntry);
  return entries.map((entry) => toModule(entry, normalizeModuleMetadata(entry)));
}

export async function saveModules(modules: Module[]): Promise<void> {
  await applyModulesPayload(modules);
}

export async function saveModuleRules(
  moduleId: string,
  rules: ModuleRules,
): Promise<void> {
  const module = {
    id: moduleId,
    enabled: true,
    rules: {
      default_mode: normalizeMountMode(rules.default_mode),
      paths: normalizeStringMap(rules.paths),
    },
  } as Module;
  await applyModulesPayload([module]);
}

export async function saveAllModuleRules(
  rules: Record<string, ModuleRules>,
): Promise<void> {
  const payload = Object.entries(rules).map(([moduleId, moduleRules]) => ({
    id: moduleId,
    enabled: true,
    rules: {
      default_mode: normalizeMountMode(moduleRules.default_mode),
      paths: normalizeStringMap(moduleRules.paths),
    },
  })) as Module[];
  await applyModulesPayload(payload);
}
