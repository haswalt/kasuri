import { useEffect, useState } from 'react'
export type Route = {
  page: string
  workflowId?: string
  runId?: string
  section: string
  mode: 'build' | 'simulate'
  query: URLSearchParams
}
export function readRoute(): Route {
  const parts = location.pathname.split('/').filter(Boolean)
  const offset = parts[0] === 'w' ? 2 : 0
  const page =
    parts[0] === 'w' && parts[1] !== 'personal'
      ? 'not-found'
      : parts[offset] || 'workflows'
  const query = new URLSearchParams(location.search)
  return {
    page,
    workflowId:
      page === 'workflows' && parts[offset + 2] === 'edit'
        ? parts[offset + 1]
        : undefined,
    runId: page === 'runs' ? parts[offset + 1] : undefined,
    section: parts[offset + 1] || 'profile',
    mode: query.get('mode') === 'simulate' ? 'simulate' : 'build',
    query
  }
}
export function go(url: string, replace = false) {
  history[replace ? 'replaceState' : 'pushState']({}, '', url)
  dispatchEvent(new PopStateEvent('popstate'))
}
export const path = (page: string) => '/w/personal/' + page
export function useRoute() {
  const [route, setRoute] = useState(readRoute)
  useEffect(() => {
    const update = () => setRoute(readRoute())
    addEventListener('popstate', update)
    return () => removeEventListener('popstate', update)
  }, [])
  return route
}
export type Theme = 'light' | 'dark' | 'system'
export function applyTheme(theme: Theme) {
  if (theme === 'system') document.documentElement.removeAttribute('data-theme')
  else document.documentElement.dataset.theme = theme
  try {
    localStorage.setItem('kasuri.theme', theme)
  } catch {}
}
export function savedTheme(): Theme {
  try {
    const t = localStorage.getItem('kasuri.theme')
    if (t === 'light' || t === 'dark') return t
  } catch {}
  return 'system'
}
