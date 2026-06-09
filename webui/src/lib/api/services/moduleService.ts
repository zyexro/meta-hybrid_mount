import { PATHS } from "../../constants";
import type { Module, ModuleRules } from "../../types";
import { runDaemonCommand } from "../core/bridge";
import {
  moduleRuntimeEntrySchema,
  type ModuleRuntimeEntryRaw,
} from "../schemas";
import { normalizeMountMode } from "../core/guards";

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

function normalizeMetadataField(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function extractMetadata(entry: ModuleRuntimeEntryRaw): ModuleMetadata {
  const defaults = defaultModuleMetadata(entry.id);
  return {
    name: normalizeMetadataField(entry.name) ?? defaults.name,
    version: normalizeMetadataField(entry.version) ?? defaults.version,
    author: normalizeMetadataField(entry.author) ?? defaults.author,
    description:
      normalizeMetadataField(entry.description) ?? defaults.description,
  };
}

function toModule(
  entry: ModuleRuntimeEntryRaw,
  metadata: ModuleMetadata,
): Module {
  const rules = entry.rules ?? { default_mode: "overlay" as const, paths: {} };
  return {
    id: entry.id,
    name: metadata.name,
    version: metadata.version,
    author: metadata.author,
    description: metadata.description,
    mode: normalizeMountMode(entry.mode),
    is_mounted: entry.is_mounted,
    enabled: entry.enabled,
    source_path: entry.source_path,
    rules: {
      default_mode: normalizeMountMode(rules.default_mode),
      paths: rules.paths ?? {},
    },
    mount_error: entry.mount_error?.trim() || undefined,
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
      paths: module.rules.paths ?? {},
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

  const entries = payload.map((item) => moduleRuntimeEntrySchema.parse(item));
  return entries.map((entry) => toModule(entry, extractMetadata(entry)));
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
      paths: rules.paths ?? {},
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
      paths: moduleRules.paths ?? {},
    },
  })) as Module[];
  await applyModulesPayload(payload);
}
