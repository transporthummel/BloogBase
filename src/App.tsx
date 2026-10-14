import { useEffect, useState } from "react";
import { commands, type AppInfo } from "./bindings";
import "./App.css";

// Temporary placeholder until the app shell lands (M0, frontend step).
function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    commands.appInfo().then((res) => {
      if (res.status === "ok") setInfo(res.data);
      else setError(res.error.message);
    });
  }, []);

  return (
    <main className="container">
      <h1>BloogBase</h1>
      {error && <p>Error: {error}</p>}
      {info && (
        <dl>
          <dt>Version</dt>
          <dd>{info.version}</dd>
          <dt>Data directory</dt>
          <dd>{info.dataDir}</dd>
          <dt>Schema version</dt>
          <dd>{info.schemaVersion}</dd>
        </dl>
      )}
    </main>
  );
}

export default App;
