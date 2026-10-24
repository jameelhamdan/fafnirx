import { lazy } from 'react'

/** The video player joined to the app, loaded when a video is first played (it brings hls.js with it). */
export const AppVideo = lazy(() => import('./app-video'))
