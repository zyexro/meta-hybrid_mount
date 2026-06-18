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

import { createRenderEffect } from "solid-js";
import "./Skeleton.css";

type SkeletonVariant =
  | "hero-label"
  | "hero-title"
  | "hero-caption"
  | "metric"
  | "stats-bar"
  | "info-wide"
  | "info-narrow"
  | "chip-row"
  | "contributor-avatar"
  | "contributor-title"
  | "contributor-body"
  | "module-card"
  | "feature-card"
  | "rule-card";

interface Props {
  variant?: SkeletonVariant;
  width?: string;
  height?: string;
  borderRadius?: string;
  class?: string;
}

export default function Skeleton(props: Props) {
  let rootRef: HTMLDivElement | undefined;

  createRenderEffect(() => {
    const root = rootRef;
    if (!root) return;

    if (props.width) {
      root.style.setProperty("--skeleton-width", props.width);
    } else {
      root.style.removeProperty("--skeleton-width");
    }

    if (props.height) {
      root.style.setProperty("--skeleton-height", props.height);
    } else {
      root.style.removeProperty("--skeleton-height");
    }

    if (props.borderRadius) {
      root.style.setProperty("--skeleton-radius", props.borderRadius);
    } else {
      root.style.removeProperty("--skeleton-radius");
    }
  });

  return (
    <div
      ref={rootRef}
      class={`skeleton ${props.variant ? `skeleton--${props.variant}` : ""} ${
        props.class || ""
      }`.trim()}
    ></div>
  );
}
