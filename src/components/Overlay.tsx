import { useState, useRef, useEffect } from "react";
import type { DictationState } from "../types";
import { PulseAnimation } from "./PulseAnimation";
import { Settings } from "./Settings";

interface OverlayProps {
  state: DictationState;
}

export function Overlay({ state }: OverlayProps) {
  const [showSettings, setShowSettings] = useState(false);
  const textRef = useRef<HTMLDivElement>(null);

  const partialText = state.type === "Recording" ? state.partial_text : undefined;

  useEffect(() => {
    if (textRef.current) {
      textRef.current.scrollTop = textRef.current.scrollHeight;
    }
  }, [partialText]);

  if (state.type === "Idle" && !showSettings) {
    return null;
  }

  return (
    <div className="flex flex-col items-center gap-2">
      {state.type !== "Idle" && (
        <div className="flex flex-col gap-2 px-5 py-3 bg-black/80 backdrop-blur-xl rounded-2xl border border-white/10 max-w-[300px]">
          <div className="flex items-center gap-3">
            {state.type === "Recording" && (
              <>
                <PulseAnimation />
                <span className="text-red-400 text-sm font-medium">
                  {state.partial_text ? "Transcribing..." : "Listening..."}
                </span>
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

            <button
              onClick={() => setShowSettings(!showSettings)}
              className="ml-auto text-white/30 hover:text-white/70 transition-colors flex-shrink-0"
              title="Settings"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
            </button>
          </div>

          {state.type === "Recording" && state.partial_text && (
            <div
              ref={textRef}
              className="text-white/90 text-sm leading-relaxed max-h-[280px] overflow-y-auto"
            >
              {state.partial_text}
            </div>
          )}
        </div>
      )}

      <Settings visible={showSettings} onClose={() => setShowSettings(false)} />
    </div>
  );
}
