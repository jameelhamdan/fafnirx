import { lazy } from 'react'
import type { FrontendModule } from '@/sdk'
import './style.css'

// The viewer is loaded when it is first shown: the player brings hls.js with it.
export default {
  viewer: { match: (file) => file.kind === 'video', component: lazy(() => import('./viewer')) },
} satisfies FrontendModule
