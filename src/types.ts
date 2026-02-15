export type DictationState =
  | { type: "Idle" }
  | { type: "Recording"; duration_ms: number; partial_text?: string }
  | { type: "Processing" }
  | { type: "Downloading"; progress: number }
  | { type: "Error"; message: string }
  | { type: "CorrectionPreview"; text: string; original_text: string; corrections: Array<{ original: string; replacement: string; position: number }> };
