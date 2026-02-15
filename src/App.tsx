import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Overlay } from "./components/Overlay";
import { ModelSettings } from "./components/ModelSettings";
import { useDictationState } from "./hooks/useDictationState";

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);
  const state = useDictationState();

  useEffect(() => {
    setWindowLabel(getCurrentWindow().label);
  }, []);

  if (windowLabel === null) return null;

  if (windowLabel === "settings") {
    return <ModelSettings />;
  }

  return (
    <div className="flex items-center justify-center w-full h-full">
      <Overlay state={state} />
    </div>
  );
}

export default App;
