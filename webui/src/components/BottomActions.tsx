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

import { ParentProps } from "solid-js";
import { createRenderEffect, createSignal, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";

export default function BottomActions(props: ParentProps) {
  const [isActivePage, setIsActivePage] = createSignal(true);
  const [keyboardInset, setKeyboardInset] = createSignal(0);
  let anchorRef: HTMLDivElement | undefined;
  let rootRef: HTMLDivElement | undefined;

  onMount(() => {
    const pageEl = anchorRef?.closest(".swipe-page");
    const rootEl = anchorRef?.closest(".main-content");
    if (!pageEl || !rootEl) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        setIsActivePage(entry.isIntersecting && entry.intersectionRatio >= 0.6);
      },
      {
        root: rootEl,
        threshold: [0.6],
      },
    );

    observer.observe(pageEl);
    onCleanup(() => observer.disconnect());
  });

  onMount(() => {
    const viewport = window.visualViewport;
    if (!viewport) return;

    let rafId = 0;
    const updateKeyboardInset = () => {
      if (rafId) return;
      rafId = window.requestAnimationFrame(() => {
        rafId = 0;
        const inset = Math.max(
          0,
          Math.round(window.innerHeight - viewport.height - viewport.offsetTop),
        );
        setKeyboardInset((prev) => (Math.abs(prev - inset) < 2 ? prev : inset));
      });
    };

    updateKeyboardInset();
    viewport.addEventListener("resize", updateKeyboardInset);
    viewport.addEventListener("scroll", updateKeyboardInset);
    window.addEventListener("orientationchange", updateKeyboardInset);

    onCleanup(() => {
      if (rafId) window.cancelAnimationFrame(rafId);
      viewport.removeEventListener("resize", updateKeyboardInset);
      viewport.removeEventListener("scroll", updateKeyboardInset);
      window.removeEventListener("orientationchange", updateKeyboardInset);
    });
  });

  createRenderEffect(() => {
    const inset = keyboardInset();
    const active = isActivePage();
    const root = rootRef;
    if (!root) return;

    root.style.setProperty("--bottom-actions-keyboard-inset", `${inset}px`);
    root.toggleAttribute("inert", !active);
  });

  return (
    <>
      <div class="bottom-actions-anchor" ref={anchorRef} aria-hidden="true" />
      <Portal>
        <div
          ref={rootRef}
          class="bottom-actions-root"
          classList={{ "is-active": isActivePage() }}
        >
          {props.children}
        </div>
      </Portal>
    </>
  );
}
