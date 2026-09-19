"use client";

import { useState } from "react";

import { SOUND_COLLECTIONS, soundCollectionForPack, type SoundCollection } from "@/lib/sound-collections";
import type { PackSummary } from "@/lib/types/runtime";

type PackLibraryProps = { packs: PackSummary[]; pendingAction: string | null; onImport: () => void; onSelect: (packId: string) => void };

export function PackLibrary({ packs, pendingAction, onImport, onSelect }: PackLibraryProps) {
  const [collection, setCollection] = useState<SoundCollection>("all");
  const visiblePacks = collection === "all"
    ? packs
    : packs.filter((pack) => soundCollectionForPack(pack) === collection);

  return (
    <section className="library-panel" aria-labelledby="library-title">
      <div className="section-heading">
        <div><p className="eyebrow">Local library</p><h2 id="library-title">Installed instruments</h2></div>
        <button className="import-button" disabled={pendingAction !== null} onClick={onImport} type="button">Import local pack</button>
      </div>
      <div aria-label="Sound collections" className="library-filters" role="toolbar">
        {SOUND_COLLECTIONS.map((option) => (
          <button
            aria-pressed={collection === option}
            className="filter-button"
            key={option}
            onClick={() => setCollection(option)}
            type="button"
          >
            {option[0].toUpperCase() + option.slice(1)}
          </button>
        ))}
      </div>
      <div className="pack-list">
        {visiblePacks.length === 0 ? (
          <p className="pack-empty" role="status">
            No {collection} instruments installed.
          </p>
        ) : null}
        {visiblePacks.map((pack, index) => (
          <article className="pack-row" key={pack.id}>
            <span className="pack-index">{String(index + 1).padStart(2, "0")}</span>
            <div className="pack-identity"><h3>{pack.name}</h3><p>{pack.groupCounts.normal} voices{pack.bundled ? " · Included" : " · Local import"}</p></div>
            {pack.active ? <span className="active-label">Active</span> : (
              <button aria-label={`Select ${pack.name}`} className="select-button" disabled={pendingAction !== null} onClick={() => onSelect(pack.id)} type="button">Select</button>
            )}
          </article>
        ))}
      </div>
    </section>
  );
}
