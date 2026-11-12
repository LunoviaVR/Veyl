// Picks the interface for the chosen theme: the standard one, or the Cassette Futurism console.
// The console (src/cassette) is a separate set of components with its own layout, and its code
// loads only when that theme is selected.
import { lazy, Suspense, useEffect, useLayoutEffect } from "react";
import App from "./App";
import { usePref, useQuery } from "./lib/hooks";
import { api } from "./lib/ipc";

const Cassette = lazy(() => import("./cassette/App"));

export default function Root() {
  const settings = useQuery(() => api.settings(), [], { events: ["settings-changed"] });
  // Remembered so the next start paints the right interface before the settings arrive.
  const [remembered, remember] = usePref("cassetteTheme", false);
  const cassette = settings.data ? settings.data.theme === "cassette" : remembered;
  useEffect(() => {
    if (settings.data) remember(cassette);
  }, [settings.data, cassette, remember]);
  // The console's styles are scoped to this attribute. Set before paint, so they never
  // touch the standard interface and never lag behind the console.
  useLayoutEffect(() => {
    if (cassette) document.documentElement.dataset.style = "cassette";
    else delete document.documentElement.dataset.style;
  }, [cassette]);

  return cassette ? (
    <Suspense fallback={null}>
      <Cassette />
    </Suspense>
  ) : (
    <App />
  );
}
