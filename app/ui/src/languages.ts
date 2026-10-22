// Cache of the backend language table (subtitler_core::lang); there is no
// frontend copy on purpose.

import { useEffect, useState } from "react";
import { api } from "./api";
import type { LanguageOption } from "./types";

export type { LanguageOption };

let cache: LanguageOption[] = [];
let inflight: Promise<LanguageOption[]> | null = null;

function sorted(rows: LanguageOption[]): LanguageOption[] {
  return rows.slice().sort((a, b) => a.name.localeCompare(b.name, "tr"));
}

function load(): Promise<LanguageOption[]> {
  inflight ??= api
    .languageOptions()
    .then((rows) => {
      cache = sorted(rows);
      inflight = null;
      return cache;
    })
    .catch((e) => {
      inflight = null;
      throw e;
    });
  return inflight;
}

export function useLanguages(): LanguageOption[] {
  const [rows, setRows] = useState<LanguageOption[]>(cache);
  useEffect(() => {
    if (cache.length) return;
    let alive = true;
    load().then(
      (r) => {
        if (alive) setRows(r);
      },
      () => {},
    );
    return () => {
      alive = false;
    };
  }, []);
  return rows;
}

/** Matches code, Turkish name and endonym. */
export function filterLanguages(rows: LanguageOption[], query: string): LanguageOption[] {
  const q = query.trim().toLocaleLowerCase("tr");
  if (!q) return rows;
  return rows.filter(
    (l) =>
      l.code.toLocaleLowerCase("tr").includes(q) ||
      l.name.toLocaleLowerCase("tr").includes(q) ||
      l.native.toLocaleLowerCase("tr").includes(q),
  );
}

// Reads the cache synchronously and returns the bare code until it loads, so
// callers need `useLanguages()` to re-render when it arrives.
export function langName(code: string): string {
  const hit = cache.find((l) => l.code === code);
  return hit ? `${hit.name} (${hit.code})` : code;
}

export function langNames(codes: string[], max = 3): string {
  const names = codes.map((code) => langName(code).replace(/\s*\([^)]*\)$/, ""));
  if (names.length <= max) return names.join(", ");
  return `${names.slice(0, max).join(", ")} +${names.length - max}`;
}
