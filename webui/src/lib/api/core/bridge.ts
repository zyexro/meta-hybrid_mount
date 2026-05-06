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
const DAEMON_WAKE_TIMEOUT_MS = 5000;

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

function hybridMountCommand(binaryPath: string, args: string): string {
  return `"${shellEscapeDoubleQuoted(binaryPath)}" ${args}`;
}

function hybridMountDaemonCommand(binaryPath: string, args: string): string {
  return `${NO_DAEMON_AUTOWAKE} ${hybridMountCommand(binaryPath, args)}`;
}

function withTimeout<T>(
  promise: Promise<T>,
  ms: number,
  message: string,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new AppError(message)), ms);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        window.clearTimeout(timer);
        reject(error);
      },
    );
  });
}

export async function ensureDaemonAwake(binaryPath: string): Promise<void> {
  if (shouldUseMock || !hasExecBridge) return;
  if (daemonReady) return daemonReady;

  daemonReady = (async () => {
    const raw = await withTimeout(
      runCommandExpectOk(hybridMountCommand(binaryPath, "daemon ping")),
      DAEMON_WAKE_TIMEOUT_MS,
      "hybrid-mount daemon wake timed out",
    );
    parseHybridMountJsonOutput(raw);
  })().catch((error) => {
    daemonReady = null;
    throw error;
  });

  return daemonReady;
}

export async function shutdownDaemon(binaryPath: string): Promise<void> {
  if (shouldUseMock || !hasExecBridge) return;
  daemonReady = null;
  try {
    await runCommandExpectOk(hybridMountDaemonCommand(binaryPath, "daemon stop"));
  } catch (error) {
    console.debug("hybrid-mount daemon stop skipped", error);
  }
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
  await ensureDaemonAwake(binaryPath);
  try {
    const raw = await runCommandExpectOk(
      hybridMountDaemonCommand(binaryPath, args),
    );
    return parseHybridMountJsonOutput(raw);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (
      message.includes("Failed to connect to daemon socket") ||
      message.includes("No such file or directory") ||
      message.includes("Connection refused")
    ) {
      daemonReady = null;
      await ensureDaemonAwake(binaryPath);
      const raw = await runCommandExpectOk(
        hybridMountDaemonCommand(binaryPath, args),
      );
      return parseHybridMountJsonOutput(raw);
    }
    throw error;
  }
}
