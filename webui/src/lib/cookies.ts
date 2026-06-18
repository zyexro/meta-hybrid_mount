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

const ONE_YEAR_SECONDS = 60 * 60 * 24 * 365;

function parseCookies(): Record<string, string> {
  if (typeof document === "undefined" || !document.cookie) return {};

  return document.cookie
    .split(";")
    .reduce<Record<string, string>>((acc, entry) => {
      const [rawName, ...rest] = entry.trim().split("=");
      if (!rawName) return acc;
      acc[decodeURIComponent(rawName)] = decodeURIComponent(rest.join("="));
      return acc;
    }, {});
}

export function getCookie(name: string): string | null {
  const cookies = parseCookies();
  return cookies[name] ?? null;
}

export function setCookie(name: string, value: string): void {
  if (typeof document === "undefined") return;

  document.cookie = `${encodeURIComponent(name)}=${encodeURIComponent(value)}; Max-Age=${ONE_YEAR_SECONDS}; Path=/; SameSite=Lax`;
}
