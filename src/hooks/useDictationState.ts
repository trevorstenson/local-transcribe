import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { DictationState } from "../types";

interface StatePayload {
  state: DictationState;
}

export function useDictationState(): DictationState {
  const [state, setState] = useState<DictationState>({ type: "Idle" });

  useEffect(() => {
    const unlisten = listen<StatePayload>("dictation-state", (event) => {
      setState(event.payload.state);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return state;
}
