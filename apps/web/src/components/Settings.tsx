import { Icon, PageHeader, Empty } from '../design/ui'
import { go, path } from '../lib/navigation'
import type { Theme } from '../lib/navigation'
export function Settings({
  section,
  theme,
  setTheme,
  workflowCount,
  connected
}: {
  section: string
  theme: Theme
  setTheme: (t: Theme) => void
  workflowCount: number
  connected: boolean
}) {
  const sections = [
    ['profile', 'Profile & security'],
    ['tokens', 'API tokens'],
    ['workspaces', 'Workspaces'],
    ['members', 'Members & roles'],
    ['deploy', 'Cloudflare & keys']
  ]
  return (
    <div className="settings-layout">
      <aside className="settings-nav">
        <div className="eyebrow">ACCOUNT</div>
        {sections.map(([key, label], i) => (
          <span key={key}>
            {i === 2 && <div className="eyebrow">WORKSPACE</div>}
            {i === 4 && <div className="eyebrow">DEPLOYMENT</div>}
            <button
              aria-current={section === key ? 'page' : undefined}
              className={section === key ? 'active' : ''}
              onClick={() => go(path('settings/' + key))}
            >
              {label}
            </button>
          </span>
        ))}
      </aside>
      <div className="settings-content">
        <PageHeader
          eyebrow="SETTINGS"
          title={sections.find((s) => s[0] === section)?.[1] || 'Settings'}
          description="Preferences and configuration for this installation."
        />
        {section === 'profile' ? (
          <>
            <section className="panel">
              <div className="panel-heading">
                <h2>Profile</h2>
                <span className="chip">Browser session</span>
              </div>
              <div className="settings-fields">
                <div className="profile-summary">
                  <span className="avatar">P</span>
                  <div>
                    <strong>Personal workspace</strong>
                    <p>Private to this browser session</p>
                  </div>
                </div>
                <label>
                  Appearance
                  <span className="segmented">
                    {(['light', 'dark', 'system'] as Theme[]).map((t) => (
                      <button
                        key={t}
                        aria-pressed={theme === t}
                        onClick={() => setTheme(t)}
                      >
                        <Icon
                          name={
                            t === 'system'
                              ? 'monitor'
                              : t === 'dark'
                                ? 'moon'
                                : 'spark'
                          }
                        />
                        {t[0].toUpperCase() + t.slice(1)}
                      </button>
                    ))}
                  </span>
                  <small>Applies instantly and is saved on this device.</small>
                </label>
                <label>
                  Time zone
                  <input
                    readOnly
                    value={Intl.DateTimeFormat().resolvedOptions().timeZone}
                  />
                  <small>
                    Display times follow this device. Schedule nodes have their
                    own time zone.
                  </small>
                </label>
              </div>
            </section>
            <section className="panel">
              <div className="panel-heading">
                <h2>Security</h2>
                <Icon name="lock" />
              </div>
              <div className="setting-row">
                <div>
                  <strong>Installation authentication</strong>
                  <p>
                    Managed by your host through Basic authentication or
                    Cloudflare Access.
                  </p>
                </div>
                <span className="chip">Host managed</span>
              </div>
              <div className="setting-row">
                <div>
                  <strong>Workspace session</strong>
                  <p>
                    An independent HttpOnly session token protects management
                    access. Clearing cookies loses access.
                  </p>
                </div>
                <span className="chip">Current device</span>
              </div>
            </section>
          </>
        ) : section === 'workspaces' ? (
          <section className="panel">
            <div className="panel-heading">
              <h2>Your workspace</h2>
            </div>
            <div className="setting-row">
              <div className="name-cell">
                <span className="workspace-avatar">P</span>
                <div>
                  <strong>Personal workspace</strong>
                  <small>Current browser workspace</small>
                </div>
              </div>
              <span>{workflowCount} workflows</span>
              <span className="pill active">Current</span>
            </div>
            <div className="settings-fields">
              <p>
                Workflow and run data persists on this host. Named workspace
                membership, switching and workspace administration are planned.
              </p>
            </div>
          </section>
        ) : section === 'deploy' ? (
          <>
            <section className="panel">
              <div className="panel-heading">
                <h2>Deployment</h2>
                <span className="connection-status">
                  <i className={connected ? 'online' : ''} />
                  {connected ? 'API connected' : 'Offline'}
                </span>
              </div>
              {[
                ['Host', location.host],
                ['Backend', 'Rust · WebAssembly'],
                ['State', 'SQLite Durable Objects'],
                ['Hosting', 'Cloudflare or self-hosted workerd']
              ].map(([label, value]) => (
                <div className="setting-row" key={label}>
                  <span>{label}</span>
                  <code>{value}</code>
                </div>
              ))}
            </section>
            <section className="panel">
              <Empty icon="key" title="Keys are managed by your host">
                Secret values and installation keys are not exposed through the
                browser. Credential-vault key rotation controls are planned.
              </Empty>
            </section>
          </>
        ) : (
          <section className="panel">
            <Empty
              icon={section === 'members' ? 'key' : 'lock'}
              title={
                section === 'members'
                  ? 'Workspace roles are planned'
                  : 'API token management is planned'
              }
            >
              This installation currently uses browser workspace sessions. These
              controls will become available when authenticated accounts and
              scoped permissions are implemented.
            </Empty>
          </section>
        )}
      </div>
    </div>
  )
}
