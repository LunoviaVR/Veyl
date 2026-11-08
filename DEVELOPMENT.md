# Development guide

## Toolchain

- Windows 10 21H2+ / Windows 11, x64
- Rust stable (1.85+, tested with 1.99) with the MSVC toolchain and the Windows SDK
  (Visual Studio Build Tools "Desktop development with C++")
- Node.js 20+ (tested with 24) and npm
- WebView2 runtime (preinstalled on Windows 11)

```powershell
npm install
npm run tauri dev        # debug build with hot-reloading UI
npm run tauri build      # release build + NSIS installer
powershell -ExecutionPolicy Bypass -File scripts/check.ps1   # full quality gate
cargo run -p pch-windows --example probe                     # print what every collector reports on this PC
```

Environment variables: `PCHC_DATA_DIR` overrides the data folder; `PCHC_LOG`
sets the log filter (e.g. `debug`, `pch_windows=trace`).

## Rules of the codebase

- OS calls only in `crates/windows-platform`. Pure logic (parsing, statistics,
  rules) in `crates/core`, with unit tests.
- `Option` for anything a source may not provide; never substitute zero.
- Every `unsafe` block carries a `// SAFETY:` comment; every Win32 return code is
  checked; buffers come from the API's size query.
- No `unwrap()` on OS results or external data. `cargo clippy -- -D warnings` must pass.
- Commands validate inputs. Frontend calls go through `src/lib/ipc.ts`.

## Adding a sensor provider

1. Implement `pch_core::HardwareSensorProvider` in `crates/windows-platform/src/<name>.rs`:

```rust
pub struct MyProvider { /* OS handles, owned */ }

impl HardwareSensorProvider for MyProvider {
    fn id(&self) -> &'static str { "my-provider" }
    fn name(&self) -> &'static str { "My provider" }
    fn poll(&mut self, now_ms: i64) -> ProviderPoll {
        // Return Status::new(Availability::ProviderUnavailable, "...") when absent.
        ProviderPoll { status: Status::available(), readings: vec![/* SensorReading */] }
    }
}
```

   - `key` must be stable across polls: `<provider>/<device>/<sensor>`.
   - Use canonical units from `pch_core::unit`; set `value: None` for unreadable sensors.
   - Release handles in `Drop`. Parse external data with bounds-checked pure functions and test them.
2. Register it in the provider list at the top of `src-tauri/src/sampler.rs`. The
   sampler polls it on the sensor interval, isolates panics, tracks min/max/avg and
   records temperatures, fans and power to history automatically.
3. If it reports a value that should feed a canonical metric (CPU package
   temperature, GPU hotspot), extend `pch_core::sensor_map` with the label and a test.

## Adding a diagnostic rule

1. Add a function in `crates/core/src/diagnostics.rs` taking `&DiagnosticInput` and
   `&Thresholds`, pushing `Finding`s. Put numeric limits in `Thresholds`.
2. Every finding must list its evidence (event refs, measured values), state what was
   observed in the title, explain what the pattern is commonly associated with, and use
   `caveat` for what the evidence cannot show.
3. Call it from `run()` and add tests for "no data → no finding", the threshold
   boundary, and the window boundary.
4. If it needs new data, extend `DiagnosticInput` and fill it in
   `src-tauri/src/commands/stability.rs::get_diagnostics`.

## Adding a Windows event

Add a `Rule` to `RULES` in `crates/core/src/events.rs` (channel, provider name exactly
as in `System/Provider/@Name`, IDs, kind). The event-log reader's XPath is generated
from this table. Add a fixture-based test with real event XML (`wevtutil qe System
/q:"*[System[EventID=N]]" /c:1 /f:xml`), with personal data removed.

## Adding a UI page

1. Create `src/pages/<Name>.tsx` using primitives from `src/components/ui.tsx`
   (`PageHeader`, `Panel`, `StatusNote`, `DataTable`, `Chart`).
2. Fetch data with `useQuery(() => api.x(), deps, { intervalMs, events })`; live
   telemetry with `useLive(selector)`.
3. Register the page in `GROUPS` and `pages` in `src/App.tsx`.
4. Render `null` values with the formatters in `src/lib/format.ts` (they show "—").

## Adding a command

1. Add a function in `src-tauri/src/commands/<area>.rs`; validate every argument.
   Blocking work: `#[tauri::command(async)]` or `blocking(...)`.
2. Register it in `generate_handler!` in `src-tauri/src/lib.rs`.
3. Add a typed wrapper in `src/lib/ipc.ts` and the types in `src/lib/types.ts`
   (serde uses camelCase).

## Database migrations

Append a SQL string to `MIGRATIONS` in `crates/history-storage/src/lib.rs`. Never edit
an applied migration. Version is tracked in `PRAGMA user_version`.

## Tests

- `cargo test --workspace`: pure logic, parsers with real-format fixtures, SQLite in
  memory, and collector smoke tests that run against the real Windows APIs.
- `npx vitest run`: formatting, sorting and other pure UI logic.
