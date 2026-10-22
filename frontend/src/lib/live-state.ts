import { createContext, useContext } from 'react'

export const LiveContext = createContext(false)

/** True while the live update stream is connected. */
export const useLive = () => useContext(LiveContext)
