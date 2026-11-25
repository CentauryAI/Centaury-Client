/* Six-step ramp of the stacked columns; the darkest always at the bottom. */
export const SHADES = ["#e9ebf1", "#d4d8e1", "#b4bac7", "#8a90a2", "#5a5f73", "#2c2e3a"] as const;

/*
 * Stacked columns ("Usage Overview"): each column has its label on top and
 * rounded-md segments that use the last n steps of SHADES. `segments`
 * heights are in px. The legend reads from dark to light.
 */
export default function StackedColumns({
  columns,
  legend,
  height = 170,
}: {
  columns: readonly { label: string; segments: readonly number[] }[];
  legend?: readonly string[];
  height?: number;
}) {
  return (
    <div>
      <div className="flex items-end gap-4" style={{ height }}>
        {columns.map(({ label, segments }) => (
          <div key={label} className="flex flex-1 flex-col items-center justify-end">
            <span className="mb-2 text-[11px] font-medium text-muted-foreground">{label}</span>
            <div className="flex w-full flex-col gap-1">
              {segments.map((h, i) => (
                <span
                  key={i}
                  className="w-full rounded-md"
                  style={{ height: h, background: SHADES[SHADES.length - segments.length + i] }}
                />
              ))}
            </div>
          </div>
        ))}
      </div>

      {legend && (
        <ul className="mt-4 flex flex-wrap items-center gap-x-4 gap-y-1">
          {legend.map((name, i) => (
            <li key={name} className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
              <span
                className="h-2 w-2 rounded-[3px]"
                style={{ background: SHADES[SHADES.length - 1 - i] ?? SHADES[0] }}
              />
              {name}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
