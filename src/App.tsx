import { Overlay } from "./components/Overlay";
import { useDictationState } from "./hooks/useDictationState";

function App() {
  const state = useDictationState();

  return (
    <div className="flex items-center justify-center w-full h-full">
      <Overlay state={state} />
    </div>
  );
}

export default App;
