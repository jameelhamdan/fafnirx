import type { CSSProperties } from 'react'
import { recall, remember } from './util'

export type Edge = 'none' | 'outline' | 'shadow'
export type Font = 'sans' | 'serif' | 'mono'

/** How subtitles look. The person watching chooses; it is kept in the browser. */
export interface SubtitleStyle {
  /** Percent of the usual size. */
  size: number
  color: string
  background: string
  /** 0 (none) to 1 (solid). */
  opacity: number
  edge: Edge
  font: Font
  /** Percent of the picture's height between the bottom and the text. */
  position: number
}

export const DEFAULT_STYLE: SubtitleStyle = {
  size: 100,
  color: '#ffffff',
  background: '#000000',
  opacity: 0.65,
  edge: 'none',
  font: 'sans',
  position: 6,
}

export const COLORS = ['#ffffff', '#ffe14d', '#6fe3ff', '#7dff8a', '#ff9ad1']
export const EDGES: Edge[] = ['none', 'outline', 'shadow']
export const FONTS: Font[] = ['sans', 'serif', 'mono']
export const LIMITS = { size: [50, 250], opacity: [0, 1], position: [2, 40] } as const

const clamp = (value: unknown, [low, high]: readonly [number, number], fallback: number) =>
  typeof value === 'number' && Number.isFinite(value) ? Math.min(high, Math.max(low, value)) : fallback

/** What was kept, field by field: anything missing or out of range is the usual. */
function checked(raw: unknown): SubtitleStyle {
  const o = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>
  const color = (v: unknown, fallback: string) => (typeof v === 'string' && /^#[0-9a-f]{6}$/i.test(v) ? v : fallback)
  return {
    size: clamp(o.size, LIMITS.size, DEFAULT_STYLE.size),
    color: color(o.color, DEFAULT_STYLE.color),
    background: color(o.background, DEFAULT_STYLE.background),
    opacity: clamp(o.opacity, LIMITS.opacity, DEFAULT_STYLE.opacity),
    edge: EDGES.find((e) => e === o.edge) ?? DEFAULT_STYLE.edge,
    font: FONTS.find((f) => f === o.font) ?? DEFAULT_STYLE.font,
    position: clamp(o.position, LIMITS.position, DEFAULT_STYLE.position),
  }
}

const styleKey = (prefix: string) => `${prefix}.subtitle-style`

export function loadStyle(prefix: string): SubtitleStyle {
  try {
    return checked(JSON.parse(recall(styleKey(prefix)) ?? 'null'))
  } catch {
    return DEFAULT_STYLE
  }
}

export const saveStyle = (prefix: string, style: SubtitleStyle) => remember(styleKey(prefix), JSON.stringify(style))

const FAMILIES: Record<Font, string> = {
  sans: 'system-ui, -apple-system, "Segoe UI", Roboto, "Noto Sans", "Noto Sans Arabic", "Noto Sans CJK SC", sans-serif',
  serif: 'Georgia, "Times New Roman", "Noto Serif", "Noto Naskh Arabic", serif',
  mono: 'ui-monospace, "Cascadia Mono", Menlo, Consolas, monospace',
}

const EDGE_SHADOWS: Record<Edge, string | undefined> = {
  none: undefined,
  outline: '0 0 2px #000, 0 0 2px #000, 0 0 3px #000, 1px 1px 2px #000, -1px -1px 2px #000',
  shadow: '0 2px 4px rgba(0, 0, 0, 0.95), 0 0 8px rgba(0, 0, 0, 0.6)',
}

/** `#rrggbb` and an opacity as `rgba(...)`. */
export function withOpacity(hex: string, opacity: number) {
  const n = parseInt(hex.slice(1), 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${opacity})`
}

/** The text of one line of subtitles, for a picture `height` pixels tall. */
export function lineStyle(style: SubtitleStyle, height: number): CSSProperties {
  return {
    color: style.color,
    fontFamily: FAMILIES[style.font],
    fontSize: Math.max(12, height * 0.045 * (style.size / 100)),
    textShadow: EDGE_SHADOWS[style.edge],
    backgroundColor: style.opacity > 0 ? withOpacity(style.background, style.opacity) : undefined,
  }
}
