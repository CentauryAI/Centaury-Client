import { cn } from "../lib/cn";

export type Column<Row> = {
  header: string;
  cell: (row: Row) => React.ReactNode;
  /** IDs and counters */
  mono?: boolean;
  nowrap?: boolean;
};

/*
 * Dashboard table: muted header, rows separated by a hairline, horizontal
 * scroll below `minWidth` (on mobile the table does not get squashed).
 */
export default function DataTable<Row>({
  columns,
  rows,
  rowKey,
  minWidth = 720,
  className,
}: {
  columns: readonly Column<Row>[];
  rows: readonly Row[];
  rowKey: (row: Row) => string;
  minWidth?: number;
  className?: string;
}) {
  return (
    <div className={cn("overflow-x-auto", className)}>
      <table className="w-full text-left text-sm" style={{ minWidth }}>
        <thead>
          <tr className="text-xs text-muted-foreground">
            {columns.map((c, i) => (
              <th key={i} className="pb-3 pr-4 font-normal">
                {c.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={rowKey(row)} className="border-t border-border/70">
              {columns.map((c, i) => (
                <td
                  key={i}
                  className={cn(
                    "py-4 pr-4",
                    c.mono && "font-mono text-xs",
                    c.nowrap && "whitespace-nowrap"
                  )}
                >
                  {c.cell(row)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
