export type DictationState =
  | { type: "Idle" }
  | {
      type: "Recording";
      duration_ms: number;
      partial_text?: string;
      partial_translation?: string;
      source_lang: string;
      target_lang: string;
    }
  | { type: "Processing" }
  | { type: "Translating" }
  | { type: "Downloading"; progress: number }
  | { type: "Error"; message: string }
  | {
      type: "CorrectionPreview";
      text: string;
      original_text: string;
      corrections: Array<{
        original: string;
        replacement: string;
        position: number;
      }>;
    }
  | {
      type: "TranslationPreview";
      source_text: string;
      translated_text: string;
      source_lang: string;
      target_lang: string;
    };
