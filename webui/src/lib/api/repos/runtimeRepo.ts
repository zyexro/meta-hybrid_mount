import { PATHS } from "../../constants";
import { AppError } from "../core/error";
import { runDaemonCommand } from "../core/bridge";
import { runtimeStateSchema, type RuntimeStatePayload } from "../schemas";

export type { RuntimeStatePayload };

export async function loadRuntimeState(): Promise<RuntimeStatePayload> {
  const raw = await runDaemonCommand({ type: "status" }, PATHS.BINARY);
  const parsed = runtimeStateSchema.safeParse(raw);
  if (!parsed.success) {
    throw new AppError("daemon status returned invalid payload");
  }
  return parsed.data;
}
