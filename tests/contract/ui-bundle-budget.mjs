import { readdir, stat } from "node:fs/promises";
import path from "node:path";

const assetsDirectory = path.resolve("dist/ui/assets");
const files = await readdir(assetsDirectory);
const javascript = files.filter((file) => file.endsWith(".js"));
const sizes = await Promise.all(
  javascript.map(async (file) => ({
    file,
    bytes: (await stat(path.join(assetsDirectory, file))).size,
  })),
);
const totalBytes = sizes.reduce((total, asset) => total + asset.bytes, 0);
const largestBytes = Math.max(0, ...sizes.map((asset) => asset.bytes));

const TOTAL_BUDGET_BYTES = 4_550_000;
const SINGLE_ASSET_BUDGET_BYTES = 2_250_000;

if (
  totalBytes > TOTAL_BUDGET_BYTES ||
  largestBytes > SINGLE_ASSET_BUDGET_BYTES
) {
  throw new Error(
    `UI bundle budget exceeded: ${totalBytes} total bytes, ${largestBytes} largest asset`,
  );
}

console.log(
  `UI bundle budget passed: ${totalBytes} total bytes, ${largestBytes} largest asset`,
);
