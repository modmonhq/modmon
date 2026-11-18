import { copyFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const destination = `${root}dist/gate`;

await mkdir(destination, { recursive: true });
await Promise.all([
  copyFile(`${root}node_modules/@cap.js/widget/cap.min.js`, `${destination}/widget.js`),
  copyFile(
    `${root}node_modules/@cap.js/wasm/browser/cap_wasm_bg.wasm`,
    `${destination}/cap_wasm_bg.wasm`,
  ),
  copyFile(`${root}node_modules/pako/dist/pako_inflate.min.js`, `${destination}/pako.min.js`),
]);
