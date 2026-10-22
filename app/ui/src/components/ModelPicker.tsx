// Controlled: switching provider rebuilds the list but leaves the value alone.
import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { api } from "../api";
import type { CatalogEntry, ModelCatalog, ModelKind } from "../models";
import {
  contextLabel,
  entriesFor,
  findEntry,
  presetEntries,
  priceShort,
} from "../models";
import { IconCheck, IconChevronDown } from "./icons";

// Shared by the STT and LLM pickers so the catalogue is fetched once.
let cataloguePromise: Promise<ModelCatalog | null> | null = null;
function loadCatalogue(force = false): Promise<ModelCatalog | null> {
  if (force || !cataloguePromise) {
    cataloguePromise = api.modelCatalog(force).catch(() => null);
  }
  return cataloguePromise;
}

interface ModelPickerProps {
  kind: ModelKind;
  provider: string;
  value: string;
  onChange: (id: string) => void;
  id?: string;
}

const byPrice = (a: CatalogEntry, b: CatalogEntry) =>
  (a.in_per_m ?? Number.MAX_SAFE_INTEGER) - (b.in_per_m ?? Number.MAX_SAFE_INTEGER);

export default function ModelPicker({ kind, provider, value, onChange, id }: ModelPickerProps) {
  const autoId = useId();
  const cid = id ?? autoId;

  const [open, setOpen] = useState(false);
  const [catalog, setCatalog] = useState<ModelCatalog | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const [query, setQuery] = useState("");
  const [restOpen, setRestOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [customOpen, setCustomOpen] = useState(false);
  const [custom, setCustom] = useState("");

  const rootRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const selected = useMemo(
    () => findEntry(catalog, kind, provider, value),
    [catalog, kind, provider, value],
  );

  const { presets, rest, restTotal, providerInfo } = useMemo(() => {
    const info = catalog?.providers?.[provider as "groq" | "openrouter"] ?? null;
    const q = query.trim().toLocaleLowerCase("tr");
    const match = (m: CatalogEntry) =>
      !q || m.id.toLocaleLowerCase("tr").includes(q) || m.name.toLocaleLowerCase("tr").includes(q);
    const all = entriesFor(catalog, kind, provider);
    const presetList = presetEntries(catalog, kind, provider);
    const presetIds = new Set(presetList.map((p) => p.id));
    const allRest = all.filter((m) => !presetIds.has(m.id));
    return {
      presets: presetList.filter(match),
      rest: allRest.filter(match).sort(byPrice),
      // Unfiltered total, for the section header.
      restTotal: allRest.length,
      providerInfo: info,
    };
  }, [catalog, kind, provider, query]);

  const searching = query.trim() !== "";
  // Search always covers the full catalogue, even while "All models" is collapsed.
  const restVisible = useMemo<CatalogEntry[]>(
    () => (searching || restOpen ? rest : []),
    [searching, restOpen, rest],
  );

  const flat = useMemo(() => [...presets, ...restVisible], [presets, restVisible]);
  // The custom-id row sits after the last item.
  const customIndex = flat.length;

  const commit = (next: string) => {
    onChange(next);
    setOpen(false);
    setQuery("");
    btnRef.current?.focus();
  };

  const openPicker = () => {
    setCustom(value);
    setCustomOpen(false);
    setQuery("");
    setRestOpen(false);
    setActive(0);
    setLoading(true);
    setFailed(false);
    loadCatalogue()
      .then((c) => {
        setCatalog(c);
        setFailed(!c);
      })
      .finally(() => setLoading(false));
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (rootRef.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    document.addEventListener("click", onDocClick);
    return () => document.removeEventListener("click", onDocClick);
  }, [open]);

  useEffect(() => {
    if (open) searchRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (customOpen) inputRef.current?.focus();
  }, [customOpen]);

  useEffect(() => {
    if (!open) return;
    listRef.current?.querySelector<HTMLElement>("[data-active]")?.scrollIntoView({ block: "nearest" });
  }, [active, open]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  useEffect(() => {
    // Clamp when a section collapses.
    setActive((a) => Math.min(a, flat.length));
  }, [flat]);

  const onListKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const total = flat.length + 1;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => (a + 1) % total);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => (a - 1 + total) % total);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (active < flat.length) commit(flat[active].id);
      else setCustomOpen(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
      btnRef.current?.focus();
    } else if (e.key === "Tab") {
      setOpen(false);
    }
  };

  const refresh = () => {
    setLoading(true);
    loadCatalogue(true)
      .then((c) => {
        setCatalog(c);
        setFailed(!c);
      })
      .finally(() => setLoading(false));
  };

  const renderRow = (m: CatalogEntry, i: number) => (
    <div
      key={m.id}
      id={`${cid}-opt-${i}`}
      role="option"
      aria-selected={m.id === value}
      data-active={i === active || undefined}
      className="mp-opt"
      onClick={() => commit(m.id)}
      onMouseEnter={() => setActive(i)}
    >
      <div className="mp-opt-top">
        <span className="mp-opt-label" title={m.name}>
          {m.name}
        </span>
        <span className="mp-opt-side">
          {m.pricing === "static" && (
            <span className="mp-tag warn" title="Bu sağlayıcı fiyatı API üzerinden yayımlamıyor">
              tarihli
            </span>
          )}
          {contextLabel(m.context) && <span className="mp-ctx">{contextLabel(m.context)}</span>}
          <span className="mp-opt-price">{priceShort(m)}</span>
          {m.id === value && <IconCheck size={13} className="mp-check" />}
        </span>
      </div>
    </div>
  );

  return (
    <div className="mp" ref={rootRef}>
      <button
        ref={btnRef}
        id={cid}
        type="button"
        className="mp-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? setOpen(false) : openPicker())}
      >
        <span className="mp-value">
          {selected ? selected.name : value || "Model seç"}
          {!selected && value && <span className="mp-tag">özel</span>}
        </span>
        <IconChevronDown size={14} className={"mp-chev" + (open ? " open" : "")} />
      </button>

      {!open ? null : (
        <div className="mp-pop" role="dialog" aria-labelledby={cid}>
          <div className="mp-search">
            <input
              ref={searchRef}
              type="search"
              placeholder="Model ara"
              aria-label="Model ara"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={onListKeyDown}
            />
            <button
              type="button"
              className="mp-refresh"
              onClick={refresh}
              disabled={loading}
              title="Listeyi yenile"
            >
              {loading ? "…" : "Yenile"}
            </button>
          </div>

          <div
            className="mp-list"
            role="listbox"
            aria-activedescendant={`${cid}-opt-${active}`}
            tabIndex={-1}
            ref={listRef}
            onKeyDown={onListKeyDown}
          >
            {loading && flat.length === 0 ? <div className="mp-note">Modeller yükleniyor…</div> : null}
            {failed ? (
              <div className="mp-note warn">
                Katalog alınamadı. Listedeki fiyatlar gösterilemiyor.
              </div>
            ) : null}
            {providerInfo && !providerInfo.live && !loading ? (
              <div className="mp-note warn">{provider} · {providerInfo.error}</div>
            ) : null}

            {presets.length > 0 && <div className="mp-group">Önerilen modeller</div>}
            {presets.map((m, i) => renderRow(m, i))}
            {searching ? (
              rest.length > 0 && <div className="mp-group">Tüm modeller</div>
            ) : (
              restTotal > 0 && (
                <button
                  type="button"
                  className="mp-group mp-group-btn"
                  aria-expanded={restOpen}
                  onClick={() => setRestOpen((v) => !v)}
                >
                  <span>{`Tüm modeller (${restTotal.toLocaleString("tr-TR")})`}</span>
                  <IconChevronDown size={12} className={"mp-chev" + (restOpen ? " open" : "")} />
                </button>
              )
            )}
            {restVisible.map((m, i) => renderRow(m, presets.length + i))}

            <div
              id={`${cid}-opt-${customIndex}`}
              role="option"
              aria-selected={!selected}
              data-active={active === customIndex || undefined}
              className="mp-opt mp-opt-escape"
              onClick={() => setCustomOpen(true)}
              onMouseEnter={() => setActive(customIndex)}
            >
              <div className="mp-opt-top">
                <span className="mp-opt-label">Özel model id yaz</span>
              </div>
              <div className="mp-opt-note">Listede olmayan bir model kimliği girin</div>
            </div>

            {customOpen ? (
              <div className="mp-custom">
                <input
                  ref={inputRef}
                  spellCheck={false}
                  placeholder="model kimliği"
                  value={custom}
                  onChange={(e) => setCustom(e.target.value)}
                  onKeyDown={(e) => {
                    // Don't let the listbox handler steal arrows/Enter/Esc from the input.
                    e.stopPropagation();
                    if (e.key === "Enter") {
                      e.preventDefault();
                      const v = custom.trim();
                      if (v) commit(v);
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      setOpen(false);
                      btnRef.current?.focus();
                    } else if (e.key === "Tab") {
                      setOpen(false);
                    }
                  }}
                />
                <button
                  type="button"
                  className="btn-sm"
                  disabled={!custom.trim()}
                  onClick={() => commit(custom.trim())}
                >
                  Uygula
                </button>
              </div>
            ) : null}
          </div>

          <div className="mp-foot">
            {(() => {
              if (!catalog) return "fiyatlar yüklenmedi";
              const day = catalog.fetched_at.slice(0, 10);
              if (!providerInfo?.live) return `${provider} · fiyat alınamadı`;
              return provider === "groq"
                ? `fiyatlar ${day} tarihli (Groq API'si fiyat yayımlamıyor)`
                : `fiyatlar canlı · ${day}`;
            })()}
          </div>
        </div>
      )}
    </div>
  );
}
