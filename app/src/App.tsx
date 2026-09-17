import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Plane =
  { state: "loading" } | { state: "found"; root: string } | { state: "missing"; reason: string };

function App() {
  const [plane, setPlane] = useState<Plane>({ state: "loading" });

  useEffect(() => {
    invoke<string>("plane_root").then(
      (root) => setPlane({ state: "found", root }),
      (reason: unknown) => setPlane({ state: "missing", reason: String(reason) }),
    );
  }, []);

  return (
    <main className="container">
      <h1>charter</h1>
      {plane.state === "loading" && <p>Looking for the plane…</p>}
      {plane.state === "found" && (
        <p>
          Plane: <code>{plane.root}</code>
        </p>
      )}
      {plane.state === "missing" && <p role="alert">No plane: {plane.reason}</p>}
    </main>
  );
}

export default App;
