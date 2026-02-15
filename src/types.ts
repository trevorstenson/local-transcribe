export type DictationState =
  | { type: "Idle" }
  | { type: "Recording"; duration_ms: number }
  | { type: "Processing" }
  | { type: "Downloading"; progress: number }
  | { type: "Error"; message: string };
