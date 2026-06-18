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
