import { AppEmbedOverlay, useEmbedStore } from 'voidtower-frontend'

// Single export only: app/def live in global Zustand state.
useEmbedStore.getState().open(
  {
    id: 'dep-1',
    app_id: 'gitea',
    app_name: 'Gitea',
    project_name: 'gitea',
    status: 'running',
    deployed_at: Date.now() / 1000,
    compose_path: '/var/lib/voidtower/apps/gitea/docker-compose.yml',
    primary_port: 3000,
    origin: 'voidtower',
  },
  {
    id: 'gitea',
    name: 'Gitea',
    description: 'Lightweight self-hosted git service',
    category: 'dev-tools',
    icon: 'gitea',
    version_hint: '1.22',
    links: {},
  },
)

export function Default() {
  return (
    <div style={{ position: 'relative', width: 560, height: 360 }}>
      <AppEmbedOverlay />
    </div>
  )
}
