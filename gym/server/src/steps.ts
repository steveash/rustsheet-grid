import { z } from "zod";

/**
 * One user action, as a person would perform it. Cells are A1 addresses resolved to screen
 * positions through the calibrated ScreenMap; `at` is a raw screen point for anything else.
 */
export const Step = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("click"),
    cell: z.string().optional(),
    at: z.object({ x: z.number(), y: z.number() }).optional(),
    modifiers: z.array(z.enum(["Shift", "Control", "Alt", "Meta"])).optional(),
    count: z.number().int().min(1).max(3).optional(),
  }),
  z.object({
    type: z.literal("press"),
    /** Playwright key syntax, e.g. "Control+ArrowDown", "F2", "Enter". */
    keys: z.string().min(1),
    repeat: z.number().int().min(1).max(200).optional(),
  }),
  z.object({ type: z.literal("type"), text: z.string(), delayMs: z.number().int().min(0).max(500).optional() }),
  z.object({
    type: z.literal("drag"),
    from: z.string(),
    to: z.string(),
    /** "fill": start from the fill handle at the bottom-right corner of `from`. */
    handle: z.enum(["center", "fill"]).optional(),
  }),
  z.object({ type: z.literal("wait"), ms: z.number().int().min(0).max(10_000) }),
]);
export type Step = z.infer<typeof Step>;

/** The cells a list of steps refers to, so their geometry can be fetched in one bridge call. */
export function referencedCells(steps: Step[]): string[] {
  const cells = new Set<string>();
  for (const s of steps) {
    if (s.type === "click" && s.cell) cells.add(s.cell);
    if (s.type === "drag") {
      cells.add(s.from);
      cells.add(s.to);
    }
  }
  return [...cells];
}
