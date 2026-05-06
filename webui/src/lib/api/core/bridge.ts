import { APP_VERSION } from "../../constants_gen";
import { AppError } from "./error";
import { shellEscapeDoubleQuoted } from "./shell";

interface KsuExecResult {
  errno: number;
  stdout: string;
  stderr: string;
}

interface KsuModule {
  exec: (cmd: string, options?: unknown) => Promise<KsuExecResult>;
}

let ksuExec: KsuModule["exec"] | null = null;

function hasKsuBridge(): boolean {
  const bridge = (globalThis as { ksu?: unknown }).ksu;
  return typeof bridge === "object" && bridge !== null && "exec" in bridge;
}

if (hasKsuBridge()) {
  try {
    const ksu = await import("kernelsu").catch(() => null);
    ksuExec = ksu ? ksu.exec : null;
  } catch {}
}

export const shouldUseMock = import.meta.env.MODE === "test";
export const defaultVersion = APP_VERSION;
export const hasExecBridge = Boolean(ksuExec);
const NO_DAEMON_AUTOWAKE = "HYBRID_MOUNT_NO_DAEMON_AUTOWAKE=1";
const DAEMON_READY_ATTEMPTS = 40;
const DAEMON_READY_INTERVAL_MS = 100;

let daemonSession: Promise<KsuExecResult> | null = null;
let daemonReady: Promise<void> | null = null;

function requireExec(): KsuModule["exec"] {
  if (!ksuExec) throw new AppError("No KSU environment");
  return ksuExec;
}

export async function runCommand(command: string): Promise<KsuExecResult> {
  const exec = requireExec();
  return exec(command);
}

export async function runCommandExpectOk(command: string): Promise<string> {
  const { errno, stdout, stderr } = await runCommand(command);
  if (errno === 0) return stdout;
  throw new AppError(stderr || `command failed: ${command}`, errno);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function hybridMountCommand(binaryPath: string, args: string): string {
  return `"${shellEscapeDoubleQuoted(binaryPath)}" ${args}`;
}

function hybridMountDaemonCommand(binaryPath: string, args: string): string {
  return `${NO_DAEMON_AUTOWAKE} ${hybridMountCommand(binaryPath, args)}`;
}

function startDaemonSession(binaryPath: string) {
  if (daemonSession) return;

  daemonSession = runCommand(hybridMountCommand(binaryPath, "daemon serve"))
    .catch((error) => {
      console.error("hybrid-mount daemon session failed", error);
      return {
        errno: 1,
        stdout: "",
        stderr: error instanceof Error ? error.message : String(error),
      };
    })
    .finally(() => {
      daemonSession = null;
      daemonReady = null;
    });
}

async function pingDaemon(binaryPath: string): Promise<boolean> {
  try {
    const { errno, stdout } = await runCommand(
      hybridMountDaemonCommand(binaryPath, "daemon ping"),
    );
    if (errno !== 0) return false;
    parseHybridMountJsonOutput(stdout);
    return true;
  } catch {
    return false;
  }
}

export async function ensureDaemonAwake(binaryPath: string): Promise<void> {
  if (shouldUseMock || !hasExecBridge) return;
  if (daemonReady) return daemonReady;

  daemonReady = (async () => {
    startDaemonSession(binaryPath);
    for (let attempt = 0; attempt < DAEMON_READY_ATTEMPTS; attempt += 1) {
      if (await pingDaemon(binaryPath)) return;
      await sleep(DAEMON_READY_INTERVAL_MS);
    }
    throw new AppError("hybrid-mount daemon did not become ready");
  })().catch((error) => {
    daemonReady = null;
    throw error;
  });

  return daemonReady;
}

function getStructuredError(payload: unknown): string | null {
  if (!payload || typeof payload !== "object") return null;

  const record = payload as Record<string, unknown>;
  if (record.type === "error" && typeof record.error === "string") {
    return record.error;
  }
  if (record.ok === false && typeof record.error === "string") {
    return record.error;
  }
  return null;
}

export function parseHybridMountJsonOutput(raw: string): unknown {
  let payload: unknown;
  try {
    payload = JSON.parse(raw) as unknown;
  } catch (error) {
    throw new AppError(
      error instanceof Error
        ? `Failed to parse hybrid-mount JSON output: ${error.message}`
        : "Failed to parse hybrid-mount JSON output",
    );
  }

  const structuredError = getStructuredError(payload);
  if (structuredError) {
    throw new AppError(structuredError);
  }

  return payload;
}

export async function runHybridMountJson(
  args: string,
  binaryPath: string,
): Promise<unknown> {
  const raw = await runCommandExpectOk(
    hybridMountDaemonCommand(binaryPath, args),
  );
  return parseHybridMountJsonOutput(raw);
}
