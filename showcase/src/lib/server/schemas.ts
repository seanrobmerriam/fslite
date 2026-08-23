import { z } from "zod";

import {
  validateGlobPattern,
  validateVirtualPath,
  type VirtualPath,
} from "../shared/path";

export const MAX_TEXT_BYTES = 1024 * 1024;

function addVirtualPathIssue(value: string, context: z.RefinementCtx): void {
  try {
    validateVirtualPath(value);
  } catch (error) {
    context.addIssue({
      code: "custom",
      message: error instanceof Error ? error.message : "invalid virtual path",
    });
  }
}

/** Canonical virtual paths that can safely be forwarded to fixed client routes. */
export const virtualPathSchema = z
  .string()
  .superRefine(addVirtualPathIssue)
  .transform((value) => value as VirtualPath);

const revisionSchema = z.number().int().positive().max(Number.MAX_SAFE_INTEGER);
const boundedTextSchema = z.string().superRefine((value, context) => {
  if (new TextEncoder().encode(value).byteLength > MAX_TEXT_BYTES) {
    context.addIssue({
      code: "custom",
      message: `text must not exceed ${MAX_TEXT_BYTES} UTF-8 bytes`,
    });
  }
});
const boundedStringSchema = z.string().min(1).max(1024);
const trashIdSchema = z
  .string()
  .regex(
    /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    "trash ID must be a canonical UUIDv7",
  );
const confirmedNameSchema = z
  .string()
  .min(1)
  .max(255)
  .refine(
    (value) => !value.includes("/") && !value.includes("\0"),
    "confirmation must be a file name",
  );
const globPatternSchema = z.string().superRefine((value, context) => {
  try {
    validateGlobPattern(value);
  } catch (error) {
    context.addIssue({
      code: "custom",
      message: error instanceof Error ? error.message : "invalid glob pattern",
    });
  }
});

const treeOperationSchema = z
  .object({ kind: z.literal("tree"), path: virtualPathSchema })
  .strict();
const readFileOperationSchema = z
  .object({ kind: z.literal("read_file"), path: virtualPathSchema })
  .strict();
const writeFileOperationSchema = z
  .object({
    kind: z.literal("write_file"),
    path: virtualPathSchema,
    text: boundedTextSchema,
    expectedRevision: revisionSchema.optional(),
  })
  .strict();
const mkdirOperationSchema = z
  .object({
    kind: z.literal("mkdir"),
    path: virtualPathSchema,
    parents: z.boolean(),
  })
  .strict();
const copyOperationSchema = z
  .object({
    kind: z.literal("copy"),
    from: virtualPathSchema,
    to: virtualPathSchema,
    recursive: z.boolean(),
  })
  .strict();
const moveOperationSchema = z
  .object({
    kind: z.literal("move"),
    from: virtualPathSchema,
    to: virtualPathSchema,
  })
  .strict();
const trashOperationSchema = z
  .object({
    kind: z.literal("trash"),
    path: virtualPathSchema,
    expectedRevision: revisionSchema.optional(),
  })
  .strict();
const removeOperationSchema = z
  .object({
    kind: z.literal("remove"),
    path: virtualPathSchema,
    recursive: z.boolean(),
    confirmedPath: z.string(),
  })
  .strict()
  .superRefine((operation, context) => {
    if (operation.path === "/") {
      context.addIssue({
        code: "custom",
        path: ["path"],
        message: "the workspace root cannot be permanently removed",
      });
    }
    if (operation.confirmedPath !== operation.path) {
      context.addIssue({
        code: "custom",
        path: ["confirmedPath"],
        message: "permanent removal requires confirming the exact path",
      });
    }
  });
const listTrashOperationSchema = z
  .object({ kind: z.literal("list_trash") })
  .strict();
const restoreOperationSchema = z
  .object({
    kind: z.literal("restore"),
    trashId: trashIdSchema,
    destination: virtualPathSchema.optional(),
  })
  .strict();
const purgeOperationSchema = z
  .object({
    kind: z.literal("purge"),
    trashId: trashIdSchema,
    confirmedName: confirmedNameSchema,
  })
  .strict();
const globOperationSchema = z
  .object({ kind: z.literal("glob"), pattern: globPatternSchema })
  .strict();
const findOperationSchema = z
  .object({
    kind: z.literal("find"),
    root: virtualPathSchema,
    nameContains: boundedStringSchema,
  })
  .strict();
const searchContentOperationSchema = z
  .object({
    kind: z.literal("search_content"),
    root: virtualPathSchema,
    text: boundedTextSchema,
  })
  .strict();
const changesOperationSchema = z
  .object({ kind: z.literal("changes"), after: boundedStringSchema.optional() })
  .strict();
const usageOperationSchema = z.object({ kind: z.literal("usage") }).strict();

/** The complete browser-facing filesystem allowlist. */
export const publicOperationSchema = z.discriminatedUnion("kind", [
  treeOperationSchema,
  readFileOperationSchema,
  writeFileOperationSchema,
  mkdirOperationSchema,
  copyOperationSchema,
  moveOperationSchema,
  trashOperationSchema,
  removeOperationSchema,
  listTrashOperationSchema,
  restoreOperationSchema,
  purgeOperationSchema,
  globOperationSchema,
  findOperationSchema,
  searchContentOperationSchema,
  changesOperationSchema,
  usageOperationSchema,
]);

export type PublicOperation = z.output<typeof publicOperationSchema>;

/** Parses untrusted browser JSON into the only operation forms the gateway accepts. */
export function parsePublicOperation(value: unknown): PublicOperation {
  return publicOperationSchema.parse(value);
}
