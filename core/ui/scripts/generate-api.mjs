import { readFile, writeFile } from "node:fs/promises";
import Ajv2020 from "ajv/dist/2020.js";
import standaloneCode from "ajv/dist/standalone/index.js";
import { compile } from "json-schema-to-typescript";

const schemaPath = new URL("../src/dashboard.schema.json", import.meta.url);
const typesPath = new URL("../src/api.generated.ts", import.meta.url);
const validatorPath = new URL("../src/api.validator.generated.js", import.meta.url);
const declarationPath = new URL("../src/api.validator.generated.d.ts", import.meta.url);
const schema = JSON.parse(await readFile(schemaPath, "utf8"));
const banner =
  "Generated from dashboard.schema.json by ./scripts/generate-artifacts.sh. Do not edit.";

const types = await compile(schema, "DashboardContract", {
  bannerComment: `/* ${banner} */`,
});
await writeFile(typesPath, types);

const ajv = new Ajv2020({
  code: { esm: true, source: true },
  formats: {
    int32: true,
    int64: true,
    uint16: true,
    uint32: true,
    uint64: true,
    uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
  },
});
for (const name of ["DashboardResponse", "DashboardEvent"]) {
  ajv.addSchema({
    $id: name,
    $schema: schema.$schema,
    $defs: schema.$defs,
    $ref: `#/$defs/${name}`,
  });
}
const validators = standaloneCode(ajv, {
  validateDashboard: "DashboardResponse",
  validateEvent: "DashboardEvent",
});
await writeFile(
  validatorPath,
  `/* ${banner} */\n${validators}`,
);
await writeFile(
  declarationPath,
  `/* ${banner} */\nexport function validateDashboard(value: unknown): boolean;\nexport function validateEvent(value: unknown): boolean;\n`,
);
