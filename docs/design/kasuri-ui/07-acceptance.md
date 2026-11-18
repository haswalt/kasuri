# 07 · Acceptance checklist

## Global

- [ ] All colours come from CSS variables in `tokens.css`. `grep -E "#[0-9a-fA-F]{3,6}"` on component styles only matches logo files.
- [ ] Light, dark and system themes work everywhere. The toggle in the account menu and Settings › Theme applies instantly and persists.
- [ ] Inter and JetBrains Mono load, and IDs, URLs, cron, JSON and durations use mono.
- [ ] Layout holds at 1280, 1440 and 1920 wide. Nothing below 1280 is required.
- [ ] Every interactive element is a real `button`, `a` or form control, reachable by Tab with a visible focus ring.
- [ ] Menus and dialogs: Esc closes, focus is trapped in dialogs and restored on close, and `aria-expanded`, `aria-haspopup`, `aria-modal` and `aria-current` are set.
- [ ] Text contrast is at least 4.5:1 in both themes, which the tokens already satisfy. Don't put `--text-3` on `--surface-2` below 12px.
- [ ] `prefers-reduced-motion` disables edge flow, pulses and spinners.
- [ ] Loading skeletons, empty states and error states exist for every list and panel.
- [ ] No secret value ever renders. Redaction comes from the API.

## Shell

- [ ] The sidebar main nav is Workflows, Runs, Connectors, Connections, MCP server and Audit log. Settings is **not** in it.
- [ ] The workspace switcher lists workspaces, shows the current one with a check, and offers Create workspace and Manage workspaces.
- [ ] The account menu holds Settings, Members & roles, Theme, Keyboard shortcuts, Documentation and Sign out, and opens upwards.
- [ ] Only one sidebar menu is open at once. Outside click and Esc close it.

## Workflows

- [ ] The stat strip and the stacked run-state bar match API counts.
- [ ] Row menu actions follow the status table in `04`. Status changes show an Undo toast.
- [ ] Delete requires typing the name, then shows an Undo toast.

## Editor

- [ ] Build and Simulate are one route, and the mode is kept in `?mode=`.
- [ ] Switching mode keeps viewport, zoom and node positions.
- [ ] The inspector is generated from the node schema, and the expression preview resolves against the pinned fixture.
- [ ] In Simulate, Play, Pause, Step, Reset and speed work. Node and edge styling follows the status table, the log and the node I/O update, and the selected node is outlined.
- [ ] The simulation never performs real side effects, and the status bar lists the recorded ones.

## Runs

- [ ] Run history filters by state, workflow, trigger and time, and searches by run, correlation or payload key.
- [ ] Clicking a run ID or name opens the debugger (`?run=`). It opens on the step needing attention, and Previous/Next, the timeline and the step list all move the same cursor.
- [ ] Inputs, outputs and logs are redacted, and large payloads link to their R2 artifact.
- [ ] Pin input as test fixture creates a fixture that is visible in Simulate.
- [ ] The replay dialog offers recorded or edited input and production or sandbox. The reused and run-again lists come from step side-effect metadata. Starting a replay creates a linked run and shows the banner.
- [ ] The run detail Gantt bars match attempt timings, and Resume from offers the checkpoints.

## Connectors, Connections, MCP

- [ ] Connector cards show state badges and capability chips, and the category tabs filter.
- [ ] Connections filter by All, Needs attention, OAuth and MCP servers. The details panel shows the alert, scopes and used-by list, and has Reconnect/Edit, Test and Revoke.
- [ ] MCP connections list tools with kind chips. The policy button cycles Auto → Ask first → Blocked, persists, and is audited.
- [ ] The MCP server page covers only inbound clients: endpoint, exposed tools with policies, client tokens shown once, and connect instructions with copy.

## Settings

- [ ] Five sections are routed via `/settings/:section`.
- [ ] Tokens and client tokens are shown once at creation, then only the prefix.
- [ ] Key rotation shows progress, and Retired keys stay listed until no credential uses them.
