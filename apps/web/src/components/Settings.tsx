import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { Icon, PageHeader, Empty, Pill } from '../design/ui'
import { go, path } from '../lib/navigation'
import type { Theme } from '../lib/navigation'
export function Settings({
  section,
  workspaceName,
  theme,
  setTheme,
  workflowCount,
  connected
}: {
  section: string
  workspaceName: string
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
    <div className={styles.settingsLayout}>
      <aside className={styles.settingsNav}>
        <div className={styles.eyebrow}>ACCOUNT</div>
        {sections.map(([key, label], i) => (
          <span key={key}>
            {i === 2 && <div className={styles.eyebrow}>WORKSPACE</div>}
            {i === 4 && <div className={styles.eyebrow}>DEPLOYMENT</div>}
            <Button
              aria-current={section === key ? 'page' : undefined}
              className={section === key ? 'active' : ''}
              onClick={() => go(path('settings/' + key))}
            >
              {label}
            </Button>
          </span>
        ))}
      </aside>
      <div className={styles.settingsContent}>
        <PageHeader
          eyebrow="SETTINGS"
          title={sections.find((s) => s[0] === section)?.[1] || 'Settings'}
          description="Preferences and configuration for this installation."
        />
        {section === 'profile' ? (
          <>
            <section className={styles.panel}>
              <div className={styles.panelHeading}>
                <h2>Profile</h2>
                <span className={styles.chip}>Browser session</span>
              </div>
              <div className={styles.settingsFields}>
                <div className={styles.profileSummary}>
                  <span className={styles.avatar}>P</span>
                  <div>
                    <strong>{workspaceName}</strong>
                    <p>Private to this browser session</p>
                  </div>
                </div>
                <label>
                  Appearance
                  <span className={styles.segmented}>
                    {(['light', 'dark', 'system'] as Theme[]).map((t) => (
                      <Button
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
                      </Button>
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
            <section className={styles.panel}>
              <div className={styles.panelHeading}>
                <h2>Security</h2>
                <Icon name="lock" />
              </div>
              <div className={styles.settingRow}>
                <div>
                  <strong>Installation authentication</strong>
                  <p>
                    Managed by your host through Basic authentication or
                    Cloudflare Access.
                  </p>
                </div>
                <span className={styles.chip}>Host managed</span>
              </div>
              <div className={styles.settingRow}>
                <div>
                  <strong>Workspace session</strong>
                  <p>
                    An independent HttpOnly session token protects management
                    access. Clearing cookies loses access.
                  </p>
                </div>
                <span className={styles.chip}>Current device</span>
              </div>
            </section>
          </>
        ) : section === 'workspaces' ? (
          <section className={styles.panel}>
            <div className={styles.panelHeading}>
              <h2>Your workspace</h2>
            </div>
            <div className={styles.settingRow}>
              <div className={styles.nameCell}>
                <span className={styles.workspaceAvatar}>P</span>
                <div>
                  <strong>{workspaceName}</strong>
                  <small>Current browser workspace</small>
                </div>
              </div>
              <span>{workflowCount} workflows</span>
              <Pill status="active" label="Current" />
            </div>
            <div className={styles.settingsFields}>
              <p>
                Workflow and run data persists on this host. Use the workspace menu
                to create, rename and switch between isolated workspaces. Each
                browser tab keeps its own selected workspace.
              </p>
            </div>
          </section>
        ) : section === 'deploy' ? (
          <>
            <section className={styles.panel}>
              <div className={styles.panelHeading}>
                <h2>Deployment</h2>
                <span className={styles.connectionStatus}>
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
                <div className={styles.settingRow} key={label}>
                  <span>{label}</span>
                  <code>{value}</code>
                </div>
              ))}
            </section>
            <section className={styles.panel}>
              <Empty icon="key" title="Keys are managed by your host">
                Secret values and installation keys are not exposed through the
                browser. Credential-vault key rotation controls are planned.
              </Empty>
            </section>
          </>
        ) : (
          <section className={styles.panel}>
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
