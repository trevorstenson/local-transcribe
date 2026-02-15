import type { DictationState } from "../types";
import { PulseAnimation } from "./PulseAnimation";

interface OverlayProps {
  state: DictationState;
}

export function Overlay({ state }: OverlayProps) {
  if (state.type === "Idle") {
    return null;
  }

  return (
    <div className="inline-flex items-center gap-3 px-5 py-3 bg-black/80 backdrop-blur-xl rounded-2xl border border-white/10">
      {state.type === "Recording" && (
        <>
          <PulseAnimation />
          <span className="text-red-400 text-sm font-medium">Listening...</span>
        </>
      )}

      {state.type === "Processing" && (
        <>
          <div className="w-3 h-3 rounded-full border-2 border-blue-400 border-t-transparent animate-spin" />
          <span className="text-blue-400 text-sm font-medium">Transcribing...</span>
        </>
      )}

      {state.type === "Downloading" && (
        <>
          <div className="w-3 h-3 rounded-full border-2 border-green-400 border-t-transparent animate-spin" />
          <span className="text-green-400 text-sm font-medium">
            Downloading model... {Math.round(state.progress * 100)}%
          </span>
        </>
      )}

      {state.type === "Error" && (
        <>
          <div className="w-2.5 h-2.5 rounded-full bg-yellow-400" />
          <span className="text-yellow-400 text-sm font-medium">{state.message}</span>
        </>
      )}
    </div>
  );
}
