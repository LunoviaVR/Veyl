import { Download } from "lucide-react";
import { useState } from "react";
import { Button, Empty, ErrorNote, Fields, PageHeader, Panel } from "../components/ui";
import { useQuery } from "../lib/hooks";
import { api, errorText } from "../lib/ipc";

export default function Specs() {
  const q = useQuery(() => api.pcSpecs(), []);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const exportAs = (format: "txt" | "csv") => {
    setMsg(null);
    setError(null);
    api.exportPcSpecs(format).then(
      (path) => path && setMsg(`Saved to ${path}`),
      (e) => setError(errorText(e)),
    );
  };

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader
        title="PC Specs"
        subtitle="Hardware and system details read from this PC. Serial numbers and the computer name are left out, so an export is safe to share."
        actions={
          <>
            <Button disabled={!q.data?.length} onClick={() => exportAs("txt")}>
              <Download className="size-3.5" /> Export TXT
            </Button>
            <Button disabled={!q.data?.length} onClick={() => exportAs("csv")}>
              <Download className="size-3.5" /> Export CSV
            </Button>
          </>
        }
      />
      <div className="px-5 pb-5 flex flex-col gap-3">
        <ErrorNote error={q.error ?? error} />
        {msg && <p className="text-[12px] text-muted selectable">{msg}</p>}
        {q.data?.length === 0 && <Empty title="No details available">Windows did not report any hardware details.</Empty>}
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-3 items-start">
          {q.data?.map((s) => (
            <Panel key={s.title} title={s.title}>
              <Fields rows={s.items.map((i) => [i.label, i.value])} />
            </Panel>
          ))}
        </div>
      </div>
    </div>
  );
}
