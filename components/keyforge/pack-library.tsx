import type { PackSummary } from "@/lib/types/runtime";

type PackLibraryProps = { packs: PackSummary[]; pendingAction: string | null; onImport: () => void; onSelect: (packId: string) => void };

export function PackLibrary({ packs, pendingAction, onImport, onSelect }: PackLibraryProps) {
  return (
    <section className="library-panel" aria-labelledby="library-title">
      <div className="section-heading">
        <div><p className="eyebrow">Local library</p><h2 id="library-title">Installed instruments</h2></div>
        <button className="import-button" disabled={pendingAction !== null} onClick={onImport} type="button">Import local pack</button>
      </div>
      <div className="pack-list">
        {packs.map((pack, index) => (
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
