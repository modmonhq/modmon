/** Status meaning shared with the terminal interface: ● ok, ▲ warn, ✗ bad, ◆ info, ○ idle. */
export type Tone = "ok" | "warn" | "bad" | "info" | "idle";

/** A key and what it does, shown as `u start`. */
export type Hint = readonly [key: string, label: string];

/**
 * One line of a terminal transcript. `command` gets a prompt, `comment` is set quietly,
 * and `config` colors the keys of a YAML line. Everything else is output.
 */
export interface TranscriptLine {
  text: string;
  kind?: "command" | "comment" | "config";
  tone?: Tone;
}
