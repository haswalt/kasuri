# UI handoff implementation

The supplied [handoff](design/kasuri-ui/README.md) is preserved as design reference. The React application uses its branding, design tokens, typography, desktop shell and editor layout. Panda CSS v2 generates the styles from typed tokens, component recipes and slot recipes; see [styling the UI](styling.md). It continues to call the Rust API for all workflow execution and persistence.

Implemented surfaces include workflow search and status filters, run history and CSV export, the connector catalogue, routed settings, the node palette and inspector, curved canvas connections, Build/Simulate modes, and a keyboard-accessible run debugger. Existing webhook endpoint management remains available from the trigger inspector. Run now accepts JSON input before starting real execution.

Light, dark and system appearance use the supplied token system. Fonts and branding are bundled locally. The desktop layout targets widths of 1280px and above. Workspace navigation currently represents the browser session's personal workspace; arbitrary workspace URLs do not create additional isolation.

## Backend boundaries

There is no demo mode or seeded production data. Unsupported features have explicit unavailable states:

- Connections, account management, general API tokens and team membership need backend implementation. The MCP portal now manages scoped agent credentials and OAuth consent; the audit view reads actual retained management records.
- Delete/restore and separate draft/live revision storage are not available.
- Runs expose outputs, attempts, errors and durable wake times. Detailed timings, logs, historical step inputs and checkpoint replay are unavailable.
- Simulation uses the Rust engine and plays back its final node snapshots. It is not a live execution event stream. HTTP simulation requires explicit response fixtures.
- Pinning a fixture copies the run input into the current editor session; it does not invent unavailable step input.

## Verification

Frontend tests cover canvas geometry, insertion, connection validation, import/export, run states, connector categorization and safe CSV cells. Production TypeScript/Vite builds pass. Lint reports existing React synchronization/dependency warnings without errors.

Browser checks exercised workflow creation, configuration and saving, simulation, real execution with a durable wait, debugger navigation, input pinning, theme switching, connector search and layouts at 1280, 1440 and 1920 pixels. QA data lives only in the isolated local browser workspace. Self-host runtime tests cover bundled font media types alongside authentication, HTTP policy, timers, cron and webhooks.
