import { lazy } from 'react'
import type { FrontendModule } from '@/sdk'
import './style.css'

// Both parts are loaded when they are first shown: the player brings hls.js with it.
export default {
  viewer: { match: (file) => file.kind === 'video', component: lazy(() => import('./viewer')) },
  queue: lazy(() => import('./queue-section')),
} satisfies FrontendModule
