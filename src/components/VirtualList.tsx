import { useEffect, useRef, useState, type ReactNode } from "react";

/**
 * Fixed row height windowing. Only the rows in (and just around) the viewport are mounted,
 * so schemas with thousands of objects stay responsive.
 */
export function VirtualList<T>({
  items,
  rowHeight,
  overscan = 8,
  render,
  className,
  scrollToIndex,
  label,
}: {
  items: T[];
  rowHeight: number;
  overscan?: number;
  render: (item: T, index: number) => ReactNode;
  className?: string;
  scrollToIndex?: number | null;
  label?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(400);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    setHeight(el.clientHeight || 400);
    const ro = new ResizeObserver(() => setHeight(el.clientHeight || 400));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (el == null || scrollToIndex == null || scrollToIndex < 0) return;
    const top = scrollToIndex * rowHeight;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + rowHeight > el.scrollTop + el.clientHeight)
      el.scrollTop = top + rowHeight - el.clientHeight;
  }, [scrollToIndex, rowHeight]);

  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const end = Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan);

  return (
    <div
      ref={ref}
      className={className}
      style={{ overflowY: "auto" }}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      role="tree"
      aria-label={label}
    >
      <div style={{ height: items.length * rowHeight, position: "relative" }}>
        {items.slice(start, end).map((item, i) => (
          <div
            key={start + i}
            style={{
              position: "absolute",
              top: (start + i) * rowHeight,
              height: rowHeight,
              left: 0,
              right: 0,
            }}
          >
            {render(item, start + i)}
          </div>
        ))}
      </div>
    </div>
  );
}
