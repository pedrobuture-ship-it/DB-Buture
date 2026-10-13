import { HEADER_H, NODE_WIDTH, ROW_H, nodeHeight, visibleColumns } from "@/lib/erd";
import { columnMarkers } from "@/lib/schemaIndex";
import type { DatabaseSchema, StableId } from "@/types/schema";

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Self contained SVG of the ER diagram (no scripts, no external references, no credentials). */
export function buildErdSvg(
  schema: DatabaseSchema,
  positions: Record<StableId, { x: number; y: number }>,
): string {
  const tables = schema.tables.filter((t) => positions[t.stableId]);
  const boxes = new Map<StableId, Box>();
  for (const t of tables) {
    const p = positions[t.stableId];
    if (p) boxes.set(t.stableId, { x: p.x, y: p.y, w: NODE_WIDTH, h: nodeHeight(t, false) });
  }
  const pad = 40;
  const minX = Math.min(0, ...[...boxes.values()].map((b) => b.x)) - pad;
  const minY = Math.min(0, ...[...boxes.values()].map((b) => b.y)) - pad;
  const maxX = Math.max(NODE_WIDTH, ...[...boxes.values()].map((b) => b.x + b.w)) + pad;
  const maxY = Math.max(HEADER_H, ...[...boxes.values()].map((b) => b.y + b.h)) + pad;
  const width = Math.round(maxX - minX);
  const height = Math.round(maxY - minY);

  const edges: string[] = [];
  for (const t of tables) {
    for (const fk of t.foreignKeys) {
      const a = boxes.get(t.stableId);
      const b = boxes.get(fk.targetTable);
      if (!a || !b) continue;
      const fromRight = a.x + a.w / 2 <= b.x + b.w / 2;
      const x1 = fromRight ? a.x + a.w : a.x;
      const x2 = fromRight ? b.x : b.x + b.w;
      const y1 = a.y + HEADER_H / 2;
      const y2 = b.y + HEADER_H / 2;
      const dx = Math.max(40, Math.abs(x2 - x1) / 2) * (fromRight ? 1 : -1);
      edges.push(
        `<path d="M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}" fill="none" stroke="#2dd4e6" stroke-opacity="0.7" stroke-width="1.2" marker-end="url(#arrow)"><title>${esc(`${t.name}.${fk.sourceColumns.join(",")} → ${fk.targetColumns.join(",")}`)}</title></path>`,
      );
    }
  }

  const nodes: string[] = [];
  for (const t of tables) {
    const b = boxes.get(t.stableId);
    if (!b) continue;
    const { shown, hidden } = visibleColumns(t);
    const rows = shown
      .map((c, i) => {
        const y = b.y + HEADER_H + 3 + i * ROW_H + 14;
        const marks = columnMarkers(t, c.name).join(" ");
        return `<text x="${b.x + 8}" y="${y}" fill="#f2b13c" font-size="9" font-weight="600">${esc(marks)}</text><text x="${b.x + 34}" y="${y}" fill="${c.nullable ? "#94a1af" : "#e7ebef"}" font-size="11">${esc(c.name)}</text><text x="${b.x + b.w - 8}" y="${y}" fill="#5f6b78" font-size="10" text-anchor="end">${esc(c.dataType)}</text>`;
      })
      .join("");
    const more =
      hidden > 0
        ? `<text x="${b.x + 8}" y="${b.y + HEADER_H + 3 + shown.length * ROW_H + 14}" fill="#5f6b78" font-size="10">+ ${hidden} more columns</text>`
        : "";
    nodes.push(
      `<g><rect x="${b.x}" y="${b.y}" width="${b.w}" height="${b.h}" rx="2" fill="#0b0e12" stroke="#2d3846"/><rect x="${b.x}" y="${b.y}" width="${b.w}" height="${HEADER_H}" fill="#171c24" stroke="#2d3846"/><text x="${b.x + 8}" y="${b.y + 19}" fill="#e7ebef" font-size="12" font-weight="600">${esc(t.name)}</text><text x="${b.x + b.w - 8}" y="${b.y + 19}" fill="#5f6b78" font-size="9" text-anchor="end">${esc(t.namespace.toUpperCase())}</text>${rows}${more}</g>`,
    );
  }

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="${Math.round(minX)} ${Math.round(minY)} ${width} ${height}" font-family="ui-monospace, 'JetBrains Mono', Menlo, monospace"><defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#2dd4e6"/></marker></defs><rect x="${Math.round(minX)}" y="${Math.round(minY)}" width="${width}" height="${height}" fill="#07090c"/>${edges.join("")}${nodes.join("")}</svg>`;
}

/** Rasterizes the SVG in the webview and returns base64 PNG data (no data: prefix). */
export async function svgToPngBase64(svg: string, scale = 2): Promise<string> {
  const img = new Image();
  const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  try {
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("could not render the diagram"));
      img.src = url;
    });
    const canvas = document.createElement("canvas");
    canvas.width = Math.min(img.width * scale, 16384);
    canvas.height = Math.min(img.height * scale, 16384);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas is not available");
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    return canvas.toDataURL("image/png").split(",")[1] ?? "";
  } finally {
    URL.revokeObjectURL(url);
  }
}
