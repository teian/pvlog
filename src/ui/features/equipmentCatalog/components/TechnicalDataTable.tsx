import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableRow,
} from "@/shared/components";

/** Displays localized engineering labels and formatted values. @param props - Accessible caption and technical data rows. @returns A compact semantic review table. */
export function TechnicalDataTable({
  caption,
  rows,
}: {
  caption: string;
  rows: readonly (readonly [string, string])[];
}) {
  return (
    <Table>
      <TableCaption>{caption}</TableCaption>
      <TableBody>
        {rows.map(([label, value]) => (
          <TableRow key={label}>
            <TableCell className="font-medium whitespace-normal">
              {label}
            </TableCell>
            <TableCell className="text-right font-mono tabular-nums whitespace-normal">
              {value}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
